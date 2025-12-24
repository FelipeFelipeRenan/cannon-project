use std::{sync::Arc, time::Instant};

use clap::Parser;
use reqwest;
use tokio::{sync::mpsc, task};

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

    let start_test = Instant::now();

    for _ in 0..args.count {
        let client_clone = Arc::clone(&client);

        let url_clone = Arc::clone(&url);

        let tx_clone = tx.clone();

        task::spawn(async move {
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
    let mut total_latency = std::time::Duration::new(0, 0);

    while let Some(result) = rx.recv().await {
        if result.success {
            success_count += 1;
            total_latency += result.duration;
        } else {
            failure_count += 1;
        }
    }

    let duration = start_test.elapsed();
    println!("\n--- 🏁 RELATÓRIO DO CANNON ---");
    println!("Tempo total: {:?}", duration);
    println!("Sucessos:    {}", success_count);
    println!("Falhas:      {}", failure_count);
    
    if success_count > 0{
        println!("Média:   {:?}", total_latency / success_count);
    }

    Ok(())
}
