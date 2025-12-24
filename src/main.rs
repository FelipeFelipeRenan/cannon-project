use std::{sync::Arc, time::Instant};

use clap::Parser;
use reqwest;
use tokio::task;

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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let client = Arc::new(reqwest::Client::new());
    let url = Arc::new(args.url);

    println!("🎯 Alvo: {}", url);
    println!("🚀 Preparando o canhão para {} disparo(s)...", args.count);

    let start_test = Instant::now();

    let mut handles = vec![];

    for i in 1..=args.count {
        let client_clone = Arc::clone(&client);

        let url_clone = Arc::clone(&url);

        let handle = task::spawn(async move {
            let start_request = Instant::now();

            let response = client_clone.get(url_clone.as_str()).send().await;

            match response {
                Ok(res) => {
                    let duration = start_request.elapsed();
                    println!(
                        "Tiro #{}: Status {} - Tempo: {:?}",
                        i,
                        res.status(),
                        duration
                    );
                }
                Err(e) => {
                    println!("Tiro #{}: ❌ FALHA - Erro: {}", i, e);
                }
            }
        });

        handles.push(handle);
    }

    for handle in handles{
        let _ = handle.await;
    }
    
    let total_duration = start_test.elapsed();
    println!("\n🏁 Teste finalizado em {:?}", total_duration);

    Ok(())
}
