//! Everything that can be decided and checked before the (possibly long) test
//! starts: the target, the derived settings and the output files.

use super::AppResult;
use crate::args::parser::Args;
use crate::client::target::{StatusFilter, Target, TcpRead};
use crate::payload::generator::PayloadTemplate;
use crate::report::gate::Baseline;
use colored::Colorize;
use std::sync::Arc;
use std::time::Duration;

/// A validated run: the settings derived from [`Args`] plus the resources
/// that were loaded to check them (baseline, payload template).
pub struct RunPlan {
    /// Target URL (HTTP) or `host:port` address (TCP).
    pub url: String,
    /// Whether the target is a raw TCP service.
    pub is_tcp: bool,
    /// Percentiles to report, as quantiles in `0.0..=1.0`.
    pub percentiles: Vec<f64>,
    /// Duration of the ramp-up phase, when configured.
    pub ramp_up: Option<Duration>,
    /// Duration of the (discarded) warm-up phase.
    pub warmup: Duration,
    /// Baseline to compare against, when configured.
    pub baseline: Option<Baseline>,
    /// Parsed request payload, when `--body` was given.
    pub template: Option<Arc<PayloadTemplate>>,
}

impl RunPlan {
    /// Checks everything that can fail before the test starts.
    ///
    /// # Errors
    ///
    /// Returns a description of the first problem found: an invalid target,
    /// an unreadable baseline, an output file that cannot be written or an
    /// invalid payload template.
    pub fn prepare(args: &Args) -> AppResult<Self> {
        let is_tcp = args.mode.eq_ignore_ascii_case("tcp");

        let url = if is_tcp {
            args.url.clone().ok_or(
                "the target address (IP:Port) is required: use -u/--url or set `url` in the config file",
            )?
        } else {
            crate::security::url_validator::validate_and_extract(&args.url)
                .map_err(|error| format!("invalid target URL: {error}"))?
        };

        let percentiles = args
            .percentiles
            .split(',')
            .filter_map(|s| s.trim().parse::<f64>().ok())
            .map(|p| p / 100.0)
            .collect();

        let ramp_up = args
            .ramp_up
            .as_deref()
            .map(crate::args::parser::parse_ramp_up_duration)
            .transpose()?;

        let baseline = args
            .compare_baseline
            .as_deref()
            .map(load_baseline)
            .transpose()?;

        for (path, what) in [
            (&args.output, "JSON report"),
            (&args.html, "HTML report"),
            (&args.csv, "CSV file"),
            (&args.save_baseline, "baseline"),
        ] {
            if let Some(path) = path {
                ensure_writable(path, what)?;
            }
        }

        let template = args
            .body
            .as_deref()
            .map(PayloadTemplate::parse)
            .transpose()?;

        if let Some(template) = &template {
            for warning in template.warnings() {
                eprintln!("{} {warning}", "⚠️ ".yellow());
            }
        }

        Ok(Self {
            url,
            is_tcp,
            percentiles,
            ramp_up,
            warmup: Duration::from_secs(args.warmup),
            baseline,
            template,
        })
    }

    /// Opens the connections (TCP) or builds the HTTP client for the target.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be built, the HTTP method
    /// or status filter is invalid, or the TCP pool cannot be established.
    pub async fn build_target(&self, args: &Args) -> AppResult<Arc<Target>> {
        if self.is_tcp {
            let address = self.url.replace("http://", "").replace("https://", "");

            let read = match &args.tcp_read_until {
                Some(delimiter) => {
                    TcpRead::Until(crate::args::parser::unescape_delimiter(delimiter))
                }
                None => TcpRead::Bytes(args.tcp_read_bytes.unwrap_or(1)),
            };

            let target = Target::new_tcp_with_options(
                &address,
                args.workers,
                Duration::from_millis(args.connect_timeout),
                Duration::from_millis(args.timeout),
                read,
            )
            .await?;

            return Ok(Arc::new(target));
        }

        let client = crate::client::http::build_optimized_client(args)?;

        let accepted_status = args
            .expect_status
            .as_deref()
            .map(StatusFilter::parse)
            .transpose()?
            .map(Arc::new);

        Ok(Arc::new(Target::new_http_with_status(
            client,
            self.url.clone(),
            reqwest::Method::from_bytes(args.method.as_bytes())
                .map_err(|error| format!("invalid HTTP method '{}': {error}", args.method))?,
            Arc::new(args.headers.clone()),
            args.expect.clone().map(Arc::new),
            accepted_status,
        )))
    }
}

/// Fails early (before a long test) if an output file cannot be written.
fn ensure_writable(path: &str, what: &str) -> AppResult<()> {
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map(|_| ())
        .map_err(|error| format!("cannot write {what} '{path}': {error}").into())
}

fn load_baseline(path: &str) -> AppResult<Baseline> {
    let content = std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read baseline '{path}': {error}"))?;

    Baseline::parse(&content).map_err(|error| format!("baseline '{path}': {error}").into())
}
