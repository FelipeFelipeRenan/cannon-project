use clap::Parser;
use serde::Deserialize;

#[derive(Parser, Debug)]
#[command(author, version, about = "Cannon - A load testing tool in Rust")]
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
        help = "Force use of HTTP/2 Prior Knowledge (useful for localhost/h2c)"
    )]
    pub http2: bool,

    #[arg(
        long,
        default_value_t = 5000,
        help = "Timeout only for establishing TCP connection (ms)"
    )]
    pub connect_timeout: u64,

    #[arg(
        long,
        default_value = "50,95,99",
        help = "Percentis for the report (ex: 50,95,99,99.9)"
    )]
    pub percentiles: String,

    #[arg(long, help = "Protocol mode: 'http' or 'tcp'", default_value = "http")]
    pub mode: String,

    #[arg(long, default_value_t = 0, help = "Warmup time in seconds")]
    pub warmup: u64,

    #[arg(
        long,
        help = "Saves the current metrics (p99 and RPS) to a baseline JSON file."
    )]
    pub save_baseline: Option<String>,

    #[arg(
        long,
        help = "Compares current test with a saved baseline and fails if worse"
    )]
    pub compare_baseline: Option<String>,

    #[arg(
        long,
        default_value_t = 5.0,
        help = "Tolerance (in %) for latency regression"
    )]
    pub tolerance: f64,

    #[arg(
        long,
        help = "God Mode: Pins Tokio threads to physical cores (Pinning)"
    )]
    pub pin_threads: bool,
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
    pub mode: Option<String>,
    pub warmup: u64,
    pub save_baseline: Option<String>,
    pub compare_baseline: Option<String>,
    pub tolerance: f64,
    pub pin_threads: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_valid_basic_arguments() {
        let args = Args::try_parse_from(["cannon", "-u", "http://localhost", "-c", "100"]).unwrap();

        assert_eq!(args.url.unwrap(), "http://localhost");
        assert_eq!(args.count, 100);
        assert_eq!(args.workers, 10, "O default de workers deve ser 10");
        assert_eq!(args.method, "GET", "O default do método deve ser GET");
    }

    #[test]
    fn test_missing_url_when_not_updating() {
        // if "cannon" is run without a URL and without the "--update" flag, it should fail to parse if the URL is mandatory.
        // since the URL in your code is an `Option<String>` and is validated in `main.rs`, the `clap` parsing succeeds.
        let args = Args::try_parse_from(["cannon"]);
        assert!(args.is_ok());
        assert!(args.unwrap().url.is_none());
    }

    #[test]
    fn test_custom_headers_parsing() {
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
            "Apdex tolerable time should be 50ms by default"
        );
    }
}
