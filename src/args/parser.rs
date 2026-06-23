use clap::Parser;
use serde::Deserialize;

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Cannon - Uma ferramenta de teste de carga em Rust"
)]
pub struct Args {
    #[arg(short, long)]
    pub url: Option<String>,

    #[arg(short = 'f', long)]
    pub config: Option<String>,

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

    #[arg(short, long, default_value_t = 30000)]
    pub timeout: u64,

    #[arg(long)]
    pub expect: Option<String>,

    #[arg(long)]
    pub ramp_up: Option<String>,

    #[arg(short = 'A', long, default_value = "Cannon/1.0")]
    pub user_agent: String,

    #[arg(long)]
    pub update: bool,

    #[arg(long)]
    pub html: Option<String>,

    #[arg(short = 'k', long)]
    pub insecure: bool,

    #[arg(long, default_value_t = 50)]
    pub apdex_t: u64,

    #[arg(long)]
    pub csv: Option<String>,

    #[arg(
        long,
        help = "Força o uso de HTTP/2 Prior Knowledge (útil para localhost/h2c)"
    )]
    pub http2: bool,

    #[arg(
        long,
        default_value_t = 5000,
        help = "Timeout apenas para estabelecer a conexão TCP (ms)"
    )]
    pub connect_timeout: u64,

    #[arg(
        long,
        default_value = "50,95,99",
        help = "Percentis for the report (ex: 50,95,99,99.9)"
    )]
    pub percentiles: String,
}

#[derive(Deserialize, Debug, Default)]
pub struct FileConfig {
    pub url: Option<String>,
    pub workers: Option<u32>,
    pub count: Option<u32>,
    pub rps: Option<u32>,
    pub timeout: Option<u64>,
    pub method: Option<String>,
    pub headers: Option<Vec<String>>,
    pub body: Option<String>,
    pub expect: Option<String>,
    pub apdex_t: Option<u64>,
    pub insecure: Option<bool>,
    pub csv: Option<String>,
    pub http2: Option<bool>,
    pub connect_timeout: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_valid_basic_arguments() {
        // Simula: cannon -u http://localhost -c 100
        let args = Args::try_parse_from(["cannon", "-u", "http://localhost", "-c", "100"]).unwrap();

        assert_eq!(args.url.unwrap(), "http://localhost");
        assert_eq!(args.count, 100);
        assert_eq!(args.workers, 10, "O default de workers deve ser 10");
        assert_eq!(args.method, "GET", "O default do método deve ser GET");
    }

    #[test]
    fn test_missing_url_when_not_updating() {
        // Se rodar "cannon" sem URL e sem a flag "--update", deve falhar no parse se a URL for obrigatória.
        // Como no seu código a URL é Option<String> e validada no main.rs, o parse do clap passa.
        let args = Args::try_parse_from(["cannon"]);
        assert!(args.is_ok());
        assert!(args.unwrap().url.is_none());
    }

    #[test]
    fn test_custom_headers_parsing() {
        // Simula: cannon -u http://localhost -H "Auth: Bearer 123" -H "Accept: application/json"
        let args = Args::try_parse_from([
            "cannon",
            "-u",
            "http://localhost",
            "-H",
            "Auth: Bearer 123",
            "-H",
            "Accept: application/json",
        ])
        .unwrap();

        assert_eq!(args.headers.len(), 2);
        assert_eq!(args.headers[0], "Auth: Bearer 123");
    }

    #[test]
    fn test_apdex_tolerance_default() {
        let args = Args::try_parse_from(["cannon", "-u", "http://localhost"]).unwrap();
        assert_eq!(
            args.apdex_t, 50,
            "O tempo tolerável do Apdex deve ser 50ms por padrão"
        );
    }
}
