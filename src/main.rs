use std::time::Instant;

use clap::Parser;
use reqwest;

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
    let client = reqwest::Client::new();

    println!("DEBUG: Configuração capturada: {:?}", args);

    println!("🎯 Alvo: {}", args.url);
    println!("🚀 Preparando o canhão para {} disparo(s)...", args.count);

    let start_test = Instant::now();

    for i in 1..=args.count {
        let start_request = Instant::now();

        let response = client.get(&args.url).send().await;

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
    }

    let total_duration = start_test.elapsed();
    println!("\n🏁 Teste finalizado em {:?}", total_duration);

    Ok(())
}
