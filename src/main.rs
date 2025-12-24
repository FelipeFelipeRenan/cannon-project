use std::{sync::Arc, time::Instant};

use clap::Parser;
use hdrhistogram::Histogram;
use reqwest;
use tokio::{sync::{Semaphore, mpsc}, task};

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
}

struct ShotResult {
    success: bool,
    duration: std::time::Duration,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let client = Arc::new(reqwest::Client::new());
    let url = Arc::new(args.url);

    let (tx, mut rx) = mpsc::channel(args.count as usize);

    println!("🎯 Alvo: {}", url);
    println!("🚀 Preparando o canhão para {} disparo(s)...", args.count);

    //let start_test = Instant::now();

    let semaphore = Arc::new(Semaphore::new(args.workers as usize));
 
    for _ in 0..args.count {

        let permit = Arc::clone(&semaphore).acquire_owned().await.unwrap();

        let client_clone = Arc::clone(&client);

        let url_clone = Arc::clone(&url);

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

    drop(tx);

    let mut success_count = 0;
    let mut failure_count = 0;
    //let mut total_latency = std::time::Duration::new(0, 0);

    let mut hist = Histogram::<u64>::new_with_bounds(1, 60_000, 3).unwrap();

    while let Some(result) = rx.recv().await {
        if result.success {
            success_count += 1;

            hist.record(result.duration.as_millis() as u64).unwrap();
        } else {
            failure_count += 1;
        }
    }

    println!("\n--- 🏁 RELATÓRIO DO CANNON ---");
    println!("Sucessos:         {}", success_count);
    println!("Falhas:           {}", failure_count);
    println!("Mínimo:           {}ms", hist.min());
    println!("Média:            {:.2}ms", hist.mean());
    println!("p50 (Mediana):    {}ms", hist.value_at_quantile(0.5));
    println!("p95:              {}ms", hist.value_at_quantile(0.95));
    println!("p99:              {}ms", hist.value_at_quantile(0.99));
    println!("Máximo:           {}ms", hist.max());
    Ok(())
}
