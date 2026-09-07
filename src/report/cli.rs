use std::collections::HashMap;

use colored::Colorize;
use hdrhistogram::Histogram;
use serde::Serialize;
use tabled::Tabled;

/// Aggregated results produced by a Cannon load test.
///
/// `FinalReport` contains the measurements collected during the test,
/// including request counts, latency statistics, throughput, network
/// traffic, response status codes, errors, and Apdex score.
///
/// The structure derives [`Serialize`] so that the report can also be
/// exported to formats such as JSON and consumed by the HTML reporter.
/// It also derives [`Tabled`] for tabular terminal output.
#[derive(Serialize, Tabled)]
pub struct FinalReport {
    /// Target URL or address used by the test.
    #[tabled(rename = "Target URL")]
    pub target: String,

    /// Total number of requests executed during the measurement phase.
    #[tabled(rename = "Total")]
    pub total_requests: u32,

    /// Number of concurrent workers used to generate load.
    #[tabled(rename = "Workers")]
    pub concurrency: u32,

    /// Number of requests that completed successfully.
    #[tabled(rename = "Success")]
    pub successes: u64,

    /// Number of requests that failed.
    #[tabled(rename = "Failures")]
    pub failures: u64,

    /// Minimum observed latency in milliseconds.
    pub min_ms: f64,

    /// Mean observed latency in milliseconds.
    pub avg_ms: f64,

    /// 50th percentile latency in milliseconds.
    pub p50_ms: f64,

    /// 95th percentile latency in milliseconds.
    pub p95_ms: f64,

    /// 99th percentile latency in milliseconds.
    pub p99_ms: f64,

    /// Maximum observed latency in milliseconds.
    pub max_ms: f64,

    /// Requests per second actually achieved during the measurement phase.
    pub actual_rps: f64,

    /// Total number of bytes sent to the target.
    pub bytes_sent: u64,

    /// Total number of bytes received from the target.
    pub bytes_received: u64,

    /// Distribution of response status codes observed during the test.
    #[tabled(skip)]
    pub status_codes: HashMap<u16, u64>,

    /// Distribution of errors observed during the test.
    #[tabled(skip)]
    pub errors: HashMap<String, u64>,

    /// Total measurement duration in seconds.
    pub duration_secs: f64,

    /// Apdex score calculated from the test results.
    pub apdex_score: f64,
}

/// A single latency metric formatted for terminal presentation.
///
/// `LatencyMetrics` is used internally by the CLI reporter to transform
/// numerical latency measurements into rows suitable for [`tabled`].
#[derive(Tabled)]
pub struct LatencyMetrics {
    /// Human-readable name of the latency metric.
    pub metric: String,

    /// Formatted value displayed for the metric.
    pub value: String,
}

/// Converts a duration expressed in microseconds to milliseconds.
///
/// Cannon stores latency measurements in microseconds because the
/// underlying histogram operates on integer microsecond values. This
/// helper converts those measurements to milliseconds for human-readable
/// reports.
///
/// # Examples
///
/// ```
/// assert_eq!(cannon::report::cli::to_ms(1_000), 1.0);
/// assert_eq!(cannon::report::cli::to_ms(500), 0.5);
/// assert_eq!(cannon::report::cli::to_ms(1_500_000), 1500.0);
/// ```
pub fn to_ms(us: u64) -> f64 {
    us as f64 / 1000.0
}

/// Renders a latency histogram as an ASCII distribution in the terminal.
///
/// The histogram is divided into approximately ten linear buckets. Each
/// bucket displays its upper latency boundary, request count, percentage
/// of the total observations, and a proportional bar.
///
/// Nothing is printed when the histogram contains no observations.
pub fn render_ascii_histogram(hist: &hdrhistogram::Histogram<u64>) {
    println!("\n{}", "📊 LATENCY DISTRIBUTION".bold().bright_white());

    let min = hist.min();
    let max = hist.max();

    let step = (max - min) / 10;

    let step = if step == 0 { 1 } else { step };

    let mut max_count = 0;

    for bucket in hist.iter_linear(step) {
        if bucket.count_since_last_iteration() > max_count {
            max_count = bucket.count_since_last_iteration();
        }
    }

    if max_count == 0 {
        return;
    }

    for bucket in hist.iter_linear(step) {
        let count = bucket.count_since_last_iteration();
        let percent = (count as f64 / hist.len() as f64) * 100.0;

        let bar_width = (count as f64 / max_count as f64 * 30.0) as usize;
        let bar = "█".repeat(bar_width);

        println!(
            "{:>8.2}ms [{:<30}] {:>6} ({:.1}%)",
            to_ms(bucket.value_iterated_to()),
            bar.cyan(),
            count,
            percent
        );

        if bucket.value_iterated_to() >= max {
            break;
        }
    }
}

/// Generates an HTML report from a serialized report payload.
///
/// Cannon embeds the HTML dashboard template into the binary at compile
/// time and injects the provided JSON payload into the template before
/// writing the resulting document to `path`.
///
/// # Errors
///
/// Returns an [`std::io::Error`] if the generated HTML cannot be written
/// to the specified path.
pub fn generate_html_report(path: &str, report_json: &str) -> std::io::Result<()> {
    let template = include_str!("../../templates/dashboard.html");
    let final_html = template.replace("/*JSON_PAYLOAD*/", report_json);
    std::fs::write(path, final_html)?;
    Ok(())
}

/// Prints the complete load-test summary to the terminal.
///
/// The summary includes:
///
/// - successful and failed requests;
/// - latency statistics and configured percentiles;
/// - latency distribution;
/// - HTTP status-code distribution;
/// - failure details;
/// - assertion failures;
/// - network traffic and throughput;
/// - target and achieved RPS;
/// - total test duration.
///
/// `target_rps` is optional because Cannon can also run without a fixed
/// RPS target. When it is absent, the report displays the measured mean
/// RPS instead.
///
/// # Arguments
///
/// * `successes` - Number of successful requests.
/// * `failures` - Number of failed requests.
/// * `hist` - Latency histogram collected during measurement.
/// * `total` - Total measurement duration.
/// * `target_rps` - Configured target RPS, if constant-rate load is enabled.
/// * `actual_rps` - RPS actually achieved by the test.
/// * `status_counts` - Response status-code distribution.
/// * `error_counts` - Error distribution.
/// * `assertion_failures` - Number of failed response assertions.
/// * `bytes_sent` - Total bytes sent to the target.
/// * `bytes_recv` - Total bytes received from the target.
/// * `percentiles` - Latency quantiles to display.
#[allow(clippy::too_many_arguments)]
pub fn print_summary(
    successes: u64,
    failures: u64,
    hist: &Histogram<u64>,
    total: std::time::Duration,
    target_rps: Option<u32>,
    actual_rps: f64,
    status_counts: std::collections::HashMap<u16, u64>,
    error_counts: std::collections::HashMap<String, u64>,
    assertion_failures: u64,
    bytes_sent: u64,
    bytes_recv: u64,
    percentiles: &[f64],
) {
    println!("\n{}", "--- 🏁 RELATÓRIO DO CANNON ---".bold().underline());
    println!("Successes:     {}", successes);
    println!("Failures:       {}", failures);

    let mut metrics = Vec::new();
    if successes > 0 {
        let to_ms_str = |v| format!("{:.2}ms", to_ms(v));

        metrics.push(LatencyMetrics {
            metric: "Min".to_string(),
            value: to_ms_str(hist.min()),
        });

        metrics.push(LatencyMetrics {
            metric: "Mean".to_string(),
            value: to_ms_str(hist.mean() as u64),
        });

        for &p in percentiles {
            let p_val = p * 100.0;

            let p_label = if p_val.fract() == 0.0 {
                if p_val == 50.0 {
                    "p50 (Median)".to_string()
                } else {
                    format!("p{:.0}", p_val)
                }
            } else {
                format!("p{}", p_val)
            };

            metrics.push(LatencyMetrics {
                metric: p_label,
                value: to_ms_str(hist.value_at_quantile(p)),
            });
        }

        metrics.push(LatencyMetrics {
            metric: "Max".to_string(),
            value: to_ms_str(hist.max()),
        });

        render_ascii_histogram(hist);
        println!("\n{}", "-------------------------".bright_black());
    }

    let table = tabled::Table::new(metrics)
        .with(tabled::settings::Style::modern())
        .to_string();

    println!("{}", table);
    println!(
        "{} {} | {} {} | {} {:?}",
        "✅ Successes:".green().bold(),
        successes.to_string().bright_white(),
        "❌ Failures:".red().bold(),
        failures.to_string().bright_white(),
        "⏱️ Total Time:".cyan().bold(),
        total
    );

    println!("\n{}", "-------------------------".bright_black());

    println!("\n{}", "📊 STATUS CODES DISTRIBUTION".bold().bright_white());

    let mut codes: Vec<_> = status_counts.into_iter().collect();

    codes.sort_by_key(|a| a.0);

    for (code, count) in codes {
        let color_code = match code {
            200..=299 => code.to_string().green(),
            400..=499 => code.to_string().yellow(),
            _ => code.to_string().red(),
        };

        println!("  HTTP {}: {}", color_code, count);
    }

    if !error_counts.is_empty() {
        println!("\n{}", "❌ FAILURES DETAILS".bold().red());

        let mut sorted_errors: Vec<_> = error_counts.into_iter().collect();
        sorted_errors.sort_by_key(|item| std::cmp::Reverse(item.1));
        for (err, count) in sorted_errors {
            let perc = (count as f64 / failures as f64) * 100.0;
            println!(
                "  {:<30} {:>6} ({:>4.1}%)",
                err.yellow(),
                count.to_string().bright_white(),
                perc
            );
        }
    }

    if assertion_failures > 0 {
        println!(
            "❌ Assertion Failures: {}",
            assertion_failures.to_string().red()
        );
    }

    println!("\n{}", "-------------------------".bright_black());

    println!("\n{}", "📈 EFFICIENCY AND NETWORK".bold().bright_white());

    let sent_mb = bytes_sent as f64 / 1_048_576.0;
    let recv_mb = bytes_recv as f64 / 1_048_576.0;

    let total_secs = total.as_secs_f64();

    let throughput_sent = sent_mb / total_secs;
    let throughput_recv = recv_mb / total_secs;

    let sent_mb_str = format!("{:.2}", sent_mb).magenta();
    let throughput_sent_str = format!("{:.2}", throughput_sent).yellow();
    let recv_mb_str = format!("{:.2}", recv_mb).cyan();
    let throughput_recv_str = format!("{:.2}", throughput_recv).yellow();

    println!(
        "📤 Transfered:   {} MB totais ({} MB/s)",
        sent_mb_str, throughput_sent_str
    );
    println!(
        "📥 Received:     {} MB totais ({} MB/s)",
        recv_mb_str, throughput_recv_str
    );
    println!("\n{}", "-------------------------".bright_black());

    println!("\n{}", "📈 CANNON EFFICIENCY".bold().bright_white());

    if let Some(target) = target_rps {
        let efficiency = (actual_rps / target as f64) * 100.0;
        let rps_str = format!("{:.2}", actual_rps).yellow();
        println!("Target RPS:      {}", target.to_string().cyan());
        println!("Real RPS:      {} ({:.1}%)", rps_str, efficiency);
    } else {
        println!(
            "Mean RPS:     {} req/s",
            format!("{:.2}", actual_rps).yellow()
        );
    }

    println!("\n{}", "-------------------------".bright_black());
    println!("Test finished in {}s", total.as_secs());
}

/// Prints Cannon's startup banner to the terminal.
pub fn print_banner() {
    let banner = r#"
      _____          _   _ _   _  ____  _   _ 
     / ____|   /\   | \ | | \ | |/ __ \| \ | |
    | |       /  \  |  \| |  \| | |  | |  \| |
    | |      / /\ \ | . ` | . ` | |  | | . ` |
    | |____ / ____ \| |\  | |\  | |__| | |\  |
     \_____/_/    \_\_| \_|_| \_|\____/|_| \_|
    "#;

    println!("{}", banner.bright_red().bold());
    println!(
        "{}",
        "--- The High-Velocity Load Tester ---"
            .bright_black()
            .italic()
    );
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_ms_conversion() {
        assert_eq!(to_ms(1_000), 1.0);
        assert_eq!(to_ms(500), 0.5);
        assert_eq!(to_ms(1_500_000), 1500.0);
        assert_eq!(to_ms(0), 0.0);
    }
}
