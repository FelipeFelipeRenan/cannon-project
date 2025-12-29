use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about = "Cannon - Uma ferramenta de teste de carga em Rust")]
pub struct Args {
    #[arg(short, long)]
    pub url: String,

    #[arg(short, long, default_value_t = 1)]
    pub count: u32,

    #[arg(short, long, default_value_t = 10)]
    pub workers: u32,

    #[arg(short, long)]
    pub output: Option<String>,

    #[arg(short, long)]
    pub rps: Option<u32>,

    #[arg(short, long)]
    pub body: Option<String>,

    #[arg(short = 'X', long, default_value = "GET")]
    pub method: String,

    #[arg(short = 'H', long = "header")]
    pub headers: Vec<String>,

}