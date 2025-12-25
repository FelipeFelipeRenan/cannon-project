use std::{sync::Arc, time::Instant};

use clap::Parser;
use hdrhistogram::Histogram;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest;
use serde::Serialize;
use tokio::{
    sync::{mpsc, Semaphore},
    task,
};

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Cannon - Uma ferramenta de teste de carga em Rust"
)]
struct Args {
    #[arg(short, long)]
    url: String,

    #[arg(short, long, default_value_t = 1)]
    count: u32,

    #[arg(short, long, default_value_t = 10)]
    workers: u32,

    #[arg(short, long)]
    output: Option<String>,
}

struct ShotResult {
    success: bool,
    duration: std::time::Duration,
}

#[derive(Serialize)]
struct FinalReport {
    target: String,
    total_requests: u32,
    concurrency: u32,
    successes: u64,
    failures: u64,
    min_ms: f64,
    avg_ms: f64,
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    max_ms: f64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let client = Arc::new(reqwest::Client::new());
    let url = Arc::new(args.url);

    let (tx, mut rx) = mpsc::channel(args.count as usize);
    let semaphore = Arc::new(Semaphore::new(args.workers as usize));

    let url_for_producer = Arc::clone(&url);

    println!("🎯 Alvo: {}", url);
    println!("🚀 Preparando o canhão para {} disparo(s)...", args.count);

    let start_test = Instant::now();

    tokio::spawn(async move {
        for _ in 0..args.count {
            let permit = Arc::clone(&semaphore).acquire_owned().await.unwrap();

            let client_clone = Arc::clone(&client);

            let url_clone = Arc::clone(&url_for_producer);

            let tx_clone = tx.clone();

            task::spawn(async move {
                let _permit = permit;
                let start_request = Instant::now();

                let response = client_clone.get(url_clone.as_str()).send().await;

                let success = response.is_ok() && response.unwrap().status().is_success();

                let _ = tx_clone
                    .send(ShotResult {
                        success,
                        duration: start_request.elapsed(),
                    })
                    .await;
            });
        }
    });

    let mut success_count = 0;
    let mut failure_count = 0;
    //let mut total_latency = std::time::Duration::new(0, 0);

    let mut hist = Histogram::<u64>::new_with_bounds(1, 60_000_000, 3).unwrap();
    let progress_bar = ProgressBar::new(args.count as u64);
    progress_bar.set_style(
        ProgressStyle::default_bar()
            .template(
                "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})",
            )
            .unwrap()
            .progress_chars("#>-"),
    );

    while let Some(result) = rx.recv().await {
        progress_bar.inc(1);
        if result.success {
            success_count += 1;

            let micros = result.duration.as_micros() as u64;

            if let Err(e) = hist.record(micros) {
                eprintln!("{}", e);
                eprintln!("Aviso: valor fora do limite: {}us", micros);
            }
        } else {
            failure_count += 1;
        }
    }

    progress_bar.finish_with_message("Concluído");

    println!("\n--- 🏁 RELATÓRIO DO CANNON ---");
    println!("Sucessos:     {}", success_count);
    println!("Falhas:       {}", failure_count);

    if success_count > 0 {
        // Função utilitária interna para facilitar a conversão de us para ms
        let to_ms = |us: u64| us as f64 / 1000.0;

        println!("Mínimo:       {:.2}ms", to_ms(hist.min()));
        println!("Média:        {:.2}ms", to_ms(hist.mean() as u64));
        println!("p50:          {:.2}ms", to_ms(hist.value_at_quantile(0.5)));
        println!("p95:          {:.2}ms", to_ms(hist.value_at_quantile(0.95)));
        println!("p99:          {:.2}ms", to_ms(hist.value_at_quantile(0.99)));
        println!("Máximo:       {:.2}ms", to_ms(hist.max()));
    }

    println!("-------------------------");
    println!("Teste finalizado em {}s", start_test.elapsed().as_secs());

    if let Some(path) = args.output {
        let to_ms = |us: u64| us as f64 / 1000.0;

        let report = FinalReport {
            target: url.to_string(),
            total_requests: args.count,
            concurrency: args.workers,
            successes: success_count,
            failures: failure_count,
            min_ms: to_ms(hist.min()),
            avg_ms: to_ms(hist.mean() as u64),
            p50_ms: to_ms(hist.value_at_quantile(0.5)),
            p95_ms: to_ms(hist.value_at_quantile(0.95)),
            p99_ms: to_ms(hist.value_at_quantile(0.99)),
            max_ms: to_ms(hist.max()),
        };

        let json = serde_json::to_string_pretty(&report)?;
        std::fs::write(path, json)?;
        println!("Relatório salvo com sucesso!");
    }
    Ok(())
}
