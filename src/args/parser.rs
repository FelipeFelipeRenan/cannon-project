use clap::{ArgAction, Parser, ValueHint};
use serde::Deserialize;

/// Command-line configuration for a Cannon load test.
///
/// `Args` contains the options accepted by the Cannon executable, including
/// target configuration, load generation, request configuration, reporting,
/// performance tuning, and baseline comparison.
///
/// Configuration can also be loaded from a YAML file through [`FileConfig`].
/// The application combines both sources before starting the load test.
#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "High-performance HTTP and TCP load testing",
    long_about = "Cannon is a high-performance load testing tool for HTTP and raw TCP services.\n\n\
                  Generate predictable load, measure latency and throughput, \
                  generate dynamic payloads, and compare results against baselines.",
    after_help = "\
EXAMPLES:
    cannon -u http://localhost:8080 -c 1000

    cannon -u http://localhost:8080 -c 10000 --rps 5000

    cannon -u http://localhost:8080 -c 100000 --warmup 5

    cannon -u http://localhost:8080/users -X POST \
        -H 'Content-Type: application/json' \
        -b '{\"name\":\"{{username}}\"}'

    cannon -u 127.0.0.1:9000 --mode tcp

Run `cannon --help` to see all available options."
)]
pub struct Args {
    // ─────────────────────────────────────────────────────────────────────
    // Target
    // ─────────────────────────────────────────────────────────────────────
    /// Target URL or TCP address.
    #[arg(
        short,
        long,
        value_name = "URL",
        value_hint = ValueHint::Url,
        help_heading = "Target"
    )]
    pub url: Option<String>,

    /// YAML configuration file.
    #[arg(
        short = 'f',
        long,
        value_name = "FILE",
        value_hint = ValueHint::FilePath,
        help_heading = "Target"
    )]
    pub config: Option<String>,

    /// Protocol used by the target.
    #[arg(
        long,
        value_name = "PROTOCOL",
        default_value = "http",
        value_parser = ["http", "tcp"],
        help_heading = "Target"
    )]
    pub mode: String,

    // ─────────────────────────────────────────────────────────────────────
    // Load generation
    // ─────────────────────────────────────────────────────────────────────
    /// Number of measured requests.
    #[arg(
        short,
        long,
        value_name = "COUNT",
        default_value_t = 1,
        help_heading = "Load"
    )]
    pub count: u32,

    /// Number of concurrent workers generating requests.
    #[arg(
        short,
        long,
        value_name = "WORKERS",
        default_value_t = 10,
        help_heading = "Load"
    )]
    pub workers: u32,

    /// Target request rate (requests/second).
    #[arg(short, long, value_name = "RPS", help_heading = "Load")]
    pub rps: Option<u32>,

    /// Duration of the warm-up phase before measurement begins.
    ///
    /// Warm-up requests generate real traffic but are excluded from the
    /// reported measurements.
    #[arg(
        long,
        value_name = "SECONDS",
        default_value_t = 0,
        help_heading = "Load"
    )]
    pub warmup: u64,

    /// Gradually increase the load over the specified duration.
    #[arg(long, value_name = "DURATION", help_heading = "Load")]
    pub ramp_up: Option<String>,

    // ─────────────────────────────────────────────────────────────────────
    // Request
    // ─────────────────────────────────────────────────────────────────────
    /// HTTP request method.
    #[arg(
        short = 'X',
        long,
        value_name = "METHOD",
        default_value = "GET",
        help_heading = "Request"
    )]
    pub method: String,

    /// HTTP request header in `Name: Value` format.
    ///
    /// This option can be specified multiple times.
    #[arg(
        short = 'H',
        long = "header",
        value_name = "HEADER",
        action = ArgAction::Append,
        help_heading = "Request"
    )]
    pub headers: Vec<String>,

    /// Request body or dynamic payload template.
    #[arg(short, long, value_name = "BODY", help_heading = "Request")]
    pub body: Option<String>,

    /// Expected response content used for assertions.
    #[arg(long, value_name = "TEXT", help_heading = "Request")]
    pub expect: Option<String>,

    /// Request timeout in milliseconds.
    #[arg(
        short,
        long,
        value_name = "MILLISECONDS",
        default_value_t = 30000,
        help_heading = "Request"
    )]
    pub timeout: u64,

    /// TCP connection establishment timeout in milliseconds.
    #[arg(
        long,
        value_name = "MILLISECONDS",
        default_value_t = 5000,
        help_heading = "Request"
    )]
    pub connect_timeout: u64,

    /// HTTP User-Agent header.
    #[arg(
        short = 'A',
        long,
        value_name = "AGENT",
        default_value = "Cannon/1.0",
        help_heading = "Request"
    )]
    pub user_agent: String,

    /// Disable TLS certificate verification.
    #[arg(short = 'k', long, help_heading = "Request")]
    pub insecure: bool,

    /// Force HTTP/2 prior knowledge.
    ///
    /// Useful when communicating with localhost or other h2c endpoints.
    #[arg(long, help_heading = "Request")]
    pub http2: bool,

    // ─────────────────────────────────────────────────────────────────────
    // Reporting
    // ─────────────────────────────────────────────────────────────────────
    /// Output report file.
    #[arg(
        short,
        long,
        value_name = "FILE",
        value_hint = ValueHint::FilePath,
        help_heading = "Reporting"
    )]
    pub output: Option<String>,

    /// Generate an HTML report.
    #[arg(
        long,
        value_name = "FILE",
        value_hint = ValueHint::FilePath,
        help_heading = "Reporting"
    )]
    pub html: Option<String>,

    /// Export request metrics to CSV.
    #[arg(
        long,
        value_name = "FILE",
        value_hint = ValueHint::FilePath,
        help_heading = "Reporting"
    )]
    pub csv: Option<String>,

    /// Percentiles included in the latency report.
    ///
    /// Multiple percentiles can be specified as a comma-separated list,
    /// such as `50,95,99,99.9`.
    #[arg(
        long,
        value_name = "PERCENTILES",
        default_value = "50,95,99",
        help_heading = "Reporting"
    )]
    pub percentiles: String,

    /// Apdex satisfied threshold in milliseconds.
    #[arg(
        long,
        value_name = "MILLISECONDS",
        default_value_t = 50,
        help_heading = "Reporting"
    )]
    pub apdex_t: u64,

    // ─────────────────────────────────────────────────────────────────────
    // Baselines
    // ─────────────────────────────────────────────────────────────────────
    /// Save the current p99 latency and RPS as a baseline.
    #[arg(
        long,
        value_name = "FILE",
        value_hint = ValueHint::FilePath,
        help_heading = "Baselines"
    )]
    pub save_baseline: Option<String>,

    /// Compare the current test against a saved baseline.
    #[arg(
        long,
        value_name = "FILE",
        value_hint = ValueHint::FilePath,
        help_heading = "Baselines"
    )]
    pub compare_baseline: Option<String>,

    /// Maximum allowed latency regression percentage.
    #[arg(
        long,
        value_name = "PERCENT",
        default_value_t = 5.0,
        help_heading = "Baselines"
    )]
    pub tolerance: f64,

    // ─────────────────────────────────────────────────────────────────────
    // Runtime
    // ─────────────────────────────────────────────────────────────────────
    /// Pin Tokio worker threads to CPU cores.
    ///
    /// Useful when reducing scheduler migration and CPU placement
    /// variability during performance-sensitive tests.
    #[arg(long, help_heading = "Runtime")]
    pub pin_threads: bool,

    /// Update Cannon to the latest available version.
    #[arg(long, help_heading = "Maintenance")]
    pub update: bool,
}

impl Args {
    /// Validates the final Cannon configuration before starting a load test.
    ///
    /// This validation must run after command-line arguments and YAML
    /// configuration have been merged.
    ///
    /// # Errors
    ///
    /// Returns an error describing the first invalid configuration value.
    pub fn validate(&self) -> Result<(), String> {
        if self.count == 0 {
            return Err("count must be greater than 0".to_string());
        }

        if self.workers == 0 {
            return Err("workers must be greater than 0".to_string());
        }

        if self.rps == Some(0) {
            return Err("rps must be greater than 0".to_string());
        }

        if self.timeout == 0 {
            return Err("timeout must be greater than 0".to_string());
        }

        if self.connect_timeout == 0 {
            return Err("connect_timeout must be greater than 0".to_string());
        }

        if self.apdex_t == 0 {
            return Err("apdex_t must be greater than 0".to_string());
        }

        if self.tolerance < 0.0 {
            return Err("tolerance must be greater than or equal to 0".to_string());
        }

        for percentile in self.percentiles.split(',') {
            let percentile = percentile
                .trim()
                .parse::<f64>()
                .map_err(|_| format!("invalid percentile: '{percentile}'"))?;

            if !(percentile > 0.0 && percentile <= 100.0) {
                return Err(format!(
                    "percentile must be greater than 0 and less than or equal to 100: {percentile}"
                ));
            }
        }

        Ok(())
    }
}

/// Configuration loaded from a Cannon YAML configuration file.
///
/// `FileConfig` contains the options that can be supplied through a YAML
/// configuration file. Optional fields allow a configuration file to
/// specify only the values it provides.
///
/// The application combines this configuration with command-line arguments
/// before starting the load test.
#[derive(Deserialize, Debug, Default)]
pub struct FileConfig {
    /// Target URL or TCP address.
    pub url: Option<String>,

    /// Number of concurrent workers.
    pub workers: Option<u32>,

    /// Number of measured requests.
    pub count: Option<u32>,

    /// Target request rate in requests per second.
    pub rps: Option<u32>,

    /// Request timeout in milliseconds.
    pub timeout: Option<u64>,

    /// HTTP request method.
    pub method: Option<String>,

    /// HTTP request headers.
    pub headers: Option<Vec<String>>,

    /// Request body or dynamic payload template.
    pub body: Option<String>,

    /// Expected response content used for assertions.
    pub expect: Option<String>,

    /// Apdex satisfied threshold in milliseconds.
    pub apdex_t: Option<u64>,

    /// Disable TLS certificate verification.
    pub insecure: Option<bool>,

    /// CSV output file.
    pub csv: Option<String>,

    /// Force HTTP/2 prior knowledge.
    pub http2: Option<bool>,

    /// TCP connection establishment timeout in milliseconds.
    pub connect_timeout: Option<u64>,

    /// Protocol used by the target.
    pub mode: Option<String>,

    /// Duration of the warm-up phase in seconds.
    pub warmup: Option<u64>,

    /// File used to save the performance baseline.
    pub save_baseline: Option<String>,

    /// File containing the baseline used for comparison.
    pub compare_baseline: Option<String>,

    /// Maximum allowed latency regression percentage.
    pub tolerance: Option<f64>,

    /// Pin Tokio worker threads to CPU cores.
    pub pin_threads: Option<bool>,
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

    #[test]
    fn test_validation_accepts_valid_configuration() {
        let args = Args::try_parse_from([
            "cannon",
            "-u",
            "http://localhost",
            "-c",
            "1000",
            "-w",
            "4",
            "--rps",
            "500",
            "--percentiles",
            "50,95,99,99.9",
        ])
        .unwrap();

        assert!(args.validate().is_ok());
    }

    #[test]
    fn test_validation_rejects_zero_count() {
        let mut args = Args::try_parse_from(["cannon", "-u", "http://localhost"]).unwrap();
        args.count = 0;

        assert_eq!(args.validate().unwrap_err(), "count must be greater than 0");
    }

    #[test]
    fn test_validation_rejects_zero_workers() {
        let mut args = Args::try_parse_from(["cannon", "-u", "http://localhost"]).unwrap();
        args.workers = 0;

        assert_eq!(
            args.validate().unwrap_err(),
            "workers must be greater than 0"
        );
    }

    #[test]
    fn test_validation_rejects_zero_rps() {
        let mut args = Args::try_parse_from(["cannon", "-u", "http://localhost"]).unwrap();
        args.rps = Some(0);

        assert_eq!(args.validate().unwrap_err(), "rps must be greater than 0");
    }

    #[test]
    fn test_validation_rejects_zero_timeout() {
        let mut args = Args::try_parse_from(["cannon", "-u", "http://localhost"]).unwrap();
        args.timeout = 0;

        assert_eq!(
            args.validate().unwrap_err(),
            "timeout must be greater than 0"
        );
    }

    #[test]
    fn test_validation_rejects_invalid_percentile() {
        let mut args = Args::try_parse_from(["cannon", "-u", "http://localhost"]).unwrap();
        args.percentiles = "50,95,101".to_string();

        assert!(args.validate().is_err());
    }

    #[test]
    fn test_validation_rejects_negative_tolerance() {
        let mut args = Args::try_parse_from(["cannon", "-u", "http://localhost"]).unwrap();
        args.tolerance = -1.0;

        assert_eq!(
            args.validate().unwrap_err(),
            "tolerance must be greater than or equal to 0"
        );
    }
}
