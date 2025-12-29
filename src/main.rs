mod args;
mod engine;
mod payload;
mod report;

use crate::args::Args;
use crate::report::LatencyMetrics;
use crate::report::{to_ms, FinalReport};
use clap::Parser;
use colored::Colorize;
use hdrhistogram::Histogram;
use indicatif::{ProgressBar, ProgressStyle};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let client = Arc::new(
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(args.timeout))
            .build()?,
    );
    let url = Arc::new(args.url.clone());
    let headers = Arc::new(args.headers.clone());

    let (tx, mut rx) = mpsc::channel(args.count as usize);

    print_banner();

    println!("🎯 Alvo: {}", url.bright_cyan().bold());
    println!(
        "🚀 {}",
        format!(
            "Preparando o canhão para {} disparo(s) com {} workers...",
            args.count.to_string().cyan(),
            args.workers.to_string().magenta()
        )
        .bold()
    );

    println!("⏱️ Timeout: {}ms", args.timeout.to_string().yellow());

    let start_test = Instant::now();

    // Inicia o motor em background
    tokio::spawn(engine::run_producer(
        args.count,
        args.workers,
        Arc::clone(&url),
        client,
        tx,
        args.rps,
        args.body.clone(),
        args.method.clone(),
        headers,
    ));

    // Configura UI e Métricas
    let mut success_count = 0;
    let mut failure_count = 0;
    let mut hist = Histogram::<u64>::new_with_bounds(1, 60_000_000, 3)?;

    let mut status_counts = std::collections::HashMap::<u16, u64>::new();

    let pb = ProgressBar::new(args.count as u64);
    pb.set_style(
    ProgressStyle::default_bar()
        .template("{spinner:.bold.green} [{elapsed_precise}] {bar:40.magenta/blue} {pos:>7}/{len:7} {msg}")
        .unwrap()
        .progress_chars("━╾─"), // Gradiente de blocos Unicode
);

    // Consumidor: Coleta resultados enquanto eles chegam
    while let Some(result) = rx.recv().await {
        pb.inc(1);
        let elapsed = start_test.elapsed().as_secs_f64();
        if elapsed > 0.1 {
            let rps = (success_count + failure_count) as f64 / elapsed;
            pb.set_message(format!("| ⚡ {:.1} RPS", rps));
        }

        if let Some(code) = result.status_code {
            *status_counts.entry(code).or_insert(0) += 1;
        }
        if result.success {
            success_count += 1;
            let _ = hist.record(result.duration.as_micros() as u64);
        } else {
            failure_count += 1;
        }
    }

    pb.finish_with_message("Concluído");

    // Impressão do Relatório
    print_summary(
        success_count,
        failure_count,
        &hist,
        start_test.elapsed(),
        args.rps,
        status_counts,
    );

    // Exportação JSON
    if let Some(path) = &args.output {
        save_report(&path, &args, &url, success_count, failure_count, &hist)?;
    }

    Ok(())
}

fn print_summary(
    successes: u64,
    failures: u64,
    hist: &Histogram<u64>,
    total: std::time::Duration,
    target_rps: Option<u32>,
    status_counts: std::collections::HashMap<u16, u64>,
) {
    println!("\n{}", "--- 🏁 RELATÓRIO DO CANNON ---".bold().underline());
    println!("Sucessos:     {}", successes);
    println!("Falhas:       {}", failures);

    let mut metrics = Vec::new();
    if successes > 0 {
        let to_ms_str = |v| format!("{:.2}ms", to_ms(v));
        metrics.push(LatencyMetrics {
            metric: "Mínimo".to_string(),
            value: to_ms_str(hist.min()),
        });
        metrics.push(LatencyMetrics {
            metric: "Média".to_string(),
            value: to_ms_str(hist.mean() as u64),
        });
        metrics.push(LatencyMetrics {
            metric: "p50 (Mediana)".to_string(),
            value: to_ms_str(hist.value_at_quantile(0.5)),
        });
        metrics.push(LatencyMetrics {
            metric: "p95".to_string(),
            value: to_ms_str(hist.value_at_quantile(0.95)),
        });
        metrics.push(LatencyMetrics {
            metric: "p99".to_string(),
            value: to_ms_str(hist.value_at_quantile(0.99)),
        });
        metrics.push(LatencyMetrics {
            metric: "Máximo".to_string(),
            value: to_ms_str(hist.max()),
        });

        report::render_ascii_histogram(hist);
        println!("\n{}", "-------------------------".bright_black());
    }

    let table = tabled::Table::new(metrics)
        .with(tabled::settings::Style::modern())
        .to_string();

    println!("{}", table);
    println!(
        "{} {} | {} {} | {} {:?}",
        "✅ Sucessos:".green().bold(),
        successes.to_string().bright_white(),
        "❌ Falhas:".red().bold(),
        failures.to_string().bright_white(),
        "⏱️ Tempo Total:".cyan().bold(),
        total
    );

    let total_secs = total.as_secs_f64();
    let actual_rps = successes as f64 / total_secs;

    println!("\n{}", "-------------------------".bright_black());

    println!(
        "\n{}",
        "📊 DISTRIBUIÇÃO DE STATUS CODES".bold().bright_white()
    );

    let mut codes: Vec<_> = status_counts.into_iter().collect();

    codes.sort_by_key(|a| a.0);

    for (code, count) in codes {
        let color_code = match code {
            200..=299 => code.to_string().green(),
            400..=499 => code.to_string().yellow(),
            _ => code.to_string().red(),
        };

        println!("  HTTP {}: {}", color_code, count);
    }

    println!("\n{}", "-------------------------".bright_black());

    println!("\n{}", "📈 EFICIÊNCIA DO CANHÃO".bold().bright_white());

    if let Some(target) = target_rps {
        let efficiency = (actual_rps / target as f64) * 100.0;
        let rps_str = format!("{:.2}", actual_rps).yellow();
        println!("RPS Alvo:      {}", target.to_string().cyan());
        println!("RPS Real:      {:.2} ({:.1}%)", rps_str, efficiency);
    } else {
        println!(
            "RPS Médio:     {:.2} req/s",
            actual_rps.to_string().yellow()
        );
    }

    println!("\n{}", "-------------------------".bright_black());
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

fn print_banner() {
    let banner = r#"
      _____          _   _ _   _  ____  _   _ 
     / ____|   /\   | \ | | \ | |/ __ \| \ | |
    | |       /  \  |  \| |  \| | |  | |  \| |
    | |      / /\ \ | . ` | . ` | |  | | . ` |
    | |____ / ____ \| |\  | |\  | |__| | |\  |
     \_____/_/    \_\_| \_|_| \_|\____/|_| \_|
    "#;
    println!("{}", banner.bright_red().bold());
    println!(
        "{}",
        "--- The High-Velocity Load Tester ---"
            .bright_black()
            .italic()
    );
    println!();
}
