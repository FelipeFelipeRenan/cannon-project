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

    println!("DEBUG: Configuração capturada: {:?}", args);

    println!("🎯 Alvo: {}", args.url);
    println!("🚀 Preparando o canhão para {} disparo(s)...", args.count);

    let client = reqwest::Client::new();

    let response = client.get(&args.url).send().await?;

    println!("✅ Status: {}", response.status());

    if response.status().is_success(){
        println!("💥 Tiro certeiro! O servidor respondeu com sucesso.");
    } else {
        println!("⚠️ O servidor recebeu o impacto, mas retornou um erro.");
    }

    Ok(())
}
