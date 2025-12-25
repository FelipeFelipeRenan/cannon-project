mod args;
mod engine;
mod report;

use crate::args::Args;
use crate::report::{to_ms, FinalReport};
use clap::Parser;
use hdrhistogram::Histogram;
use indicatif::{ProgressBar, ProgressStyle};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let client = Arc::new(reqwest::Client::new());
    let url = Arc::new(args.url.clone());

    let (tx, mut rx) = mpsc::channel(args.count as usize);

    println!("🎯 Alvo: {}", url);
    println!("🚀 Preparando o canhão para {} disparo(s)...", args.count);

    let start_test = Instant::now();

    // Inicia o motor em background
    tokio::spawn(engine::run_producer(
        args.count,
        args.workers,
        Arc::clone(&url),
        client,
        tx,
    ));

    // Configura UI e Métricas
    let mut success_count = 0;
    let mut failure_count = 0;
    let mut hist = Histogram::<u64>::new_with_bounds(1, 60_000_000, 3)?;

    let pb = ProgressBar::new(args.count as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(
                "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})",
            )?
            .progress_chars("#>-"),
    );

    // Consumidor: Coleta resultados enquanto eles chegam
    while let Some(result) = rx.recv().await {
        pb.inc(1);
        if result.success {
            success_count += 1;
            let _ = hist.record(result.duration.as_micros() as u64);
        } else {
            failure_count += 1;
        }
    }

    pb.finish_with_message("Concluído");

    // Impressão do Relatório
    print_summary(success_count, failure_count, &hist, start_test.elapsed());

    // Exportação JSON
    if let Some(path) = &args.output {
        save_report(
            &path,
            &args,
            &url,
            success_count,
            failure_count,
            &hist,
        )?;
    }

    Ok(())
}

fn print_summary(successes: u64, failures: u64, hist: &Histogram<u64>, total: std::time::Duration) {
    println!("\n--- 🏁 RELATÓRIO DO CANNON ---");
    println!("Sucessos:     {}", successes);
    println!("Falhas:       {}", failures);

    if successes > 0 {
        println!("Mínimo:       {:.2}ms", to_ms(hist.min()));
        println!("Média:        {:.2}ms", to_ms(hist.mean() as u64));
        println!("p50:          {:.2}ms", to_ms(hist.value_at_quantile(0.5)));
        println!("p95:          {:.2}ms", to_ms(hist.value_at_quantile(0.95)));
        println!("p99:          {:.2}ms", to_ms(hist.value_at_quantile(0.99)));
        println!("Máximo:       {:.2}ms", to_ms(hist.max()));
    }
    println!("-------------------------");
    println!("Teste finalizado em {}s", total.as_secs());
}

fn save_report(
    path: &str,
    args: &Args,
    url: &str,
    successes: u64,
    failures: u64,
    hist: &Histogram<u64>,
) -> Result<(), Box<dyn std::error::Error>> {
    let report = FinalReport {
        target: url.to_string(),
        total_requests: args.count,
        concurrency: args.workers,
        successes,
        failures,
        min_ms: to_ms(hist.min()),
        avg_ms: to_ms(hist.mean() as u64),
        p50_ms: to_ms(hist.value_at_quantile(0.5)),
        p95_ms: to_ms(hist.value_at_quantile(0.95)),
        p99_ms: to_ms(hist.value_at_quantile(0.99)),
        max_ms: to_ms(hist.max()),
    };
    let json = serde_json::to_string_pretty(&report)?;
    std::fs::write(path, json)?;
    println!("📂 Relatório salvo com sucesso!");
    Ok(())
}
