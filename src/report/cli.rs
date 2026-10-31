use std::collections::HashMap;
use std::time::Duration;

use colored::Colorize;
use hdrhistogram::Histogram;
use serde::Serialize;
use tabled::Tabled;

use super::summary::{histogram_percentile_ms, RunSummary};

/// Version of the JSON report layout produced by [`FinalReport`].
pub const REPORT_SCHEMA_VERSION: u32 = 2;

/// Latency statistics in milliseconds.
#[derive(Serialize)]
pub struct LatencyStats {
    /// Minimum observed latency.
    pub min_ms: f64,

    /// Mean observed latency.
    pub avg_ms: f64,

    /// 50th percentile latency.
    pub p50_ms: f64,

    /// 95th percentile latency.
    pub p95_ms: f64,

    /// 99th percentile latency.
    pub p99_ms: f64,

    /// Maximum observed latency.
    pub max_ms: f64,
}

impl LatencyStats {
    /// Computes the statistics of a latency histogram (microseconds).
    ///
    /// An empty histogram (no successful request) yields zeros, because
    /// JSON cannot represent the `NaN` a mean of nothing would produce.
    pub fn from_histogram(histogram: &Histogram<u64>) -> Self {
        if histogram.is_empty() {
            return Self {
                min_ms: 0.0,
                avg_ms: 0.0,
                p50_ms: 0.0,
                p95_ms: 0.0,
                p99_ms: 0.0,
                max_ms: 0.0,
            };
        }

        Self {
            min_ms: to_ms(histogram.min()),
            avg_ms: histogram.mean() / 1000.0,
            p50_ms: to_ms(histogram.value_at_quantile(0.5)),
            p95_ms: to_ms(histogram.value_at_quantile(0.95)),
            p99_ms: to_ms(histogram.value_at_quantile(0.99)),
            max_ms: to_ms(histogram.max()),
        }
    }
}

/// Aggregated results produced by a Cannon load test.
///
/// `FinalReport` contains the measurements collected during the test,
/// including request counts, latency statistics, throughput, network
/// traffic, response status codes, errors, and Apdex score.
///
/// The structure derives [`Serialize`] so that the report can be exported
/// to JSON and consumed by the HTML reporter.
///
/// Latency figures describe **successful** requests only. When the run was
/// paced (`--rps`) they are measured from the scheduled send time
/// (`latency_mode` is `open-loop`); `service_time` always holds the plain
/// request duration without queueing.
#[derive(Serialize)]
pub struct FinalReport {
    /// Version of this report layout.
    pub schema_version: u32,

    /// Target URL or address used by the test.
    pub target: String,

    /// Number of requests that completed during the measurement phase.
    pub total_requests: u64,

    /// Number of concurrent workers used to generate load.
    pub concurrency: u32,

    /// Number of requests that completed successfully.
    pub successes: u64,

    /// Number of requests that failed.
    pub failures: u64,

    /// Percentage of measured requests that failed.
    pub error_rate_percent: f64,

    /// Number of responses that did not satisfy the `--expect` assertion.
    pub assertion_failures: u64,

    /// How latency was measured: `open-loop` or `closed-loop`.
    pub latency_mode: String,

    /// Response-time statistics of successful requests.
    #[serde(flatten)]
    pub latency: LatencyStats,

    /// Service-time statistics of successful requests (no queueing delay).
    pub service_time: LatencyStats,

    /// Configured request rate, if the load was paced.
    pub target_rps: Option<u32>,

    /// Requests per second actually achieved during the measurement phase.
    pub actual_rps: f64,

    /// Total number of payload bytes sent to the target.
    pub bytes_sent: u64,

    /// Total number of payload bytes received from the target.
    pub bytes_received: u64,

    /// Distribution of response status codes observed during the test.
    pub status_codes: HashMap<u16, u64>,

    /// Distribution of errors observed during the test.
    pub errors: HashMap<String, u64>,

    /// Total measurement duration in seconds.
    pub duration_secs: f64,

    /// Apdex score calculated from the test results.
    pub apdex_score: f64,

    /// Apdex satisfied threshold, in milliseconds.
    pub apdex_threshold_ms: u64,

    /// Configured latency percentiles and their measured values in milliseconds.
    pub percentiles: HashMap<String, f64>,

    /// Whether the run was stopped before all requests were sent.
    pub interrupted: bool,
}

impl FinalReport {
    /// Builds the report for a finished run.
    pub fn new(
        summary: &RunSummary,
        target: &str,
        workers: u32,
        target_rps: Option<u32>,
        apdex_threshold_ms: u64,
        percentiles: &[f64],
    ) -> Self {
        Self {
            schema_version: REPORT_SCHEMA_VERSION,
            target: target.to_string(),
            total_requests: summary.measured_requests,
            concurrency: workers,
            successes: summary.successes,
            failures: summary.failures,
            error_rate_percent: summary.error_rate_percent(),
            assertion_failures: summary.assertion_failures,
            latency_mode: summary.latency_mode().to_string(),
            latency: LatencyStats::from_histogram(&summary.histogram),
            service_time: LatencyStats::from_histogram(&summary.service_histogram),
            target_rps,
            actual_rps: summary.actual_rps(),
            bytes_sent: summary.bytes_sent,
            bytes_received: summary.bytes_received,
            status_codes: summary.status_counts.clone(),
            errors: summary.error_counts.clone(),
            duration_secs: summary.duration.as_secs_f64(),
            apdex_score: summary.apdex(apdex_threshold_ms),
            apdex_threshold_ms,
            percentiles: percentiles
                .iter()
                .map(|&quantile| (percentile_label(quantile), summary.percentile_ms(quantile)))
                .collect(),
            interrupted: summary.interrupted,
        }
    }
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

/// Formats a latency quantile (`0.0..=1.0`) as a short label such as `p99`
/// or `p99.9`.
///
/// The value is rounded to six decimal places first so that floating-point
/// noise from the `percent / 100 * 100` round trip never leaks into reports
/// (for example `p7.000000000000001`).
///
/// # Examples
///
/// ```
/// assert_eq!(cannon::report::cli::percentile_label(0.99), "p99");
/// assert_eq!(cannon::report::cli::percentile_label(0.999), "p99.9");
/// assert_eq!(cannon::report::cli::percentile_label(0.07), "p7");
/// ```
pub fn percentile_label(quantile: f64) -> String {
    let percent = (quantile * 100.0 * 1_000_000.0).round() / 1_000_000.0;

    format!("p{percent}")
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
/// Buckets double in width (0.1 ms, 0.2 ms, 0.4 ms, ...), so a few slow
/// outliers cannot flatten the whole distribution into a single bar. Each
/// bucket shows its upper boundary, request count, share of the total and a
/// proportional bar. Empty buckets at either end are omitted.
///
/// Nothing is printed when the histogram contains no observations.
pub fn render_ascii_histogram(hist: &hdrhistogram::Histogram<u64>) {
    if hist.is_empty() {
        return;
    }

    let mut buckets: Vec<(u64, u64)> = hist
        .iter_log(100, 2.0)
        .map(|bucket| {
            (
                bucket.value_iterated_to(),
                bucket.count_since_last_iteration(),
            )
        })
        .collect();

    while buckets.last().is_some_and(|&(_, count)| count == 0) {
        buckets.pop();
    }

    let first = buckets
        .iter()
        .position(|&(_, count)| count > 0)
        .unwrap_or(0);

    let max_count = buckets.iter().map(|&(_, count)| count).max().unwrap_or(0);

    if max_count == 0 {
        return;
    }

    println!("\n{}", "Distribution".bold());

    for &(upper_bound, count) in &buckets[first..] {
        let percent = count as f64 / hist.len() as f64 * 100.0;

        let bar_width = (count as f64 / max_count as f64 * 30.0).round() as usize;
        let bar = "█".repeat(bar_width);

        println!(
            "{:>9.2}ms [{:<30}] {:>8} ({:.1}%)",
            to_ms(upper_bound),
            bar.cyan(),
            count,
            percent
        );
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

    let safe_json = report_json
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029");

    // Chart.js 4.4.1 is embedded so the dashboard works offline and cannot
    // change under us (see assets/chart.js.LICENSE.md). It is substituted
    // before the report data so that data can never be mistaken for a marker.
    let final_html = template
        .replace("/*CHARTJS*/", include_str!("../../assets/chart.umd.js"))
        .replace("/*JSON_PAYLOAD*/", &safe_json);

    std::fs::write(path, final_html)?;
    Ok(())
}

/// Prints the complete load-test summary to the terminal.
///
/// The summary includes:
///
/// - successful and failed requests;
/// - latency statistics and configured percentiles (successful requests);
/// - latency distribution;
/// - HTTP status-code distribution;
/// - failure details and assertion failures;
/// - network traffic and throughput;
/// - target and achieved RPS, and the Apdex score.
///
/// `target_rps` is optional because Cannon can also run without a fixed
/// RPS target. When it is absent, the report displays the measured mean
/// RPS instead. `requested` is the number of requests the run was asked to
/// send, used to describe interrupted runs.
pub fn print_summary(
    summary: &RunSummary,
    requested: u32,
    target_rps: Option<u32>,
    percentiles: &[f64],
    apdex_threshold_ms: u64,
) {
    let successes = summary.successes;
    let failures = summary.failures;
    let hist = &summary.histogram;

    println!("\n{}", "━━━ 🏁 CANNON'S REPORT ━━━".bold().bright_white());

    if summary.interrupted {
        println!(
            "{}",
            format!(
                "⚠️  Run interrupted: partial results ({} of {} requests completed)",
                summary.measured_requests, requested
            )
            .yellow()
            .bold()
        );
    }

    // One-line verdict first: it is what people look for.
    let failure_text = format!(
        "❌ Failures: {} ({:.2}%)",
        failures,
        summary.error_rate_percent()
    );

    println!(
        "\n{}   {}   {}",
        format!("✅ Successes: {successes}").green().bold(),
        if failures == 0 {
            failure_text.bright_black().bold()
        } else {
            failure_text.red().bold()
        },
        format!("⏱️  Duration: {}", format_duration(summary.duration))
            .cyan()
            .bold()
    );

    if let Some(hint) = failure_hint(summary) {
        println!("\n{}", format!("💡 {hint}").yellow());
    }

    print_section("LATENCY");

    if successes > 0 {
        let to_ms_str = |v| format!("{:.2}ms", to_ms(v));

        let mut metrics = vec![
            LatencyMetrics {
                metric: "Min".to_string(),
                value: to_ms_str(hist.min()),
            },
            LatencyMetrics {
                metric: "Mean".to_string(),
                value: format!("{:.2}ms", hist.mean() / 1000.0),
            },
        ];

        for &p in percentiles {
            let label = percentile_label(p);

            metrics.push(LatencyMetrics {
                metric: if label == "p50" {
                    "p50 (Median)".to_string()
                } else {
                    label
                },
                value: to_ms_str(hist.value_at_quantile(p)),
            });
        }

        metrics.push(LatencyMetrics {
            metric: "Max".to_string(),
            value: to_ms_str(hist.max()),
        });

        println!(
            "{}",
            tabled::Table::new(metrics).with(tabled::settings::Style::rounded())
        );

        print_latency_basis(summary);

        render_ascii_histogram(hist);
    } else {
        println!(
            "{}",
            "No request succeeded: latency statistics are not available.".yellow()
        );
    }

    if !summary.status_counts.is_empty() {
        print_section("STATUS CODES");

        let mut codes: Vec<_> = summary.status_counts.iter().collect();

        codes.sort_by_key(|entry| *entry.0);

        let total: u64 = summary.status_counts.values().sum();

        for (code, count) in codes {
            let label = format!("HTTP {code}");

            let label = match code {
                200..=299 => label.green(),
                300..=399 => label.cyan(),
                400..=499 => label.yellow(),
                _ => label.red(),
            };

            let share = *count as f64 / total.max(1) as f64;
            let bar = "█".repeat((share * 20.0).round() as usize);

            println!(
                "  {:<18} {:>10}  {:<20} {:>5.1}%",
                label,
                count,
                bar.bright_black(),
                share * 100.0
            );
        }
    }

    print_failure_details(summary);

    if summary.assertion_failures > 0 {
        println!(
            "\n  ❌ Assertion Failures: {}",
            summary.assertion_failures.to_string().red()
        );
    }

    print_section("THROUGHPUT");

    let total_secs = summary.duration.as_secs_f64();
    let actual_rps = summary.actual_rps();

    if let Some(target) = target_rps {
        let efficiency = actual_rps / f64::from(target.max(1)) * 100.0;

        print_row("Target RPS", target.to_string().cyan());
        print_row(
            "Actual RPS",
            format!("{actual_rps:.2} ({efficiency:.1}% of target)").yellow(),
        );
    } else {
        print_row("Requests/sec", format!("{actual_rps:.2}").yellow());
    }

    let per_second = |bytes: u64| {
        if total_secs > 0.0 {
            bytes as f64 / total_secs
        } else {
            0.0
        }
    };

    print_row(
        "📤 Sent",
        format!(
            "{} ({}/s)",
            format_bytes(summary.bytes_sent as f64),
            format_bytes(per_second(summary.bytes_sent))
        )
        .magenta(),
    );
    print_row(
        "📥 Received",
        format!(
            "{} ({}/s)",
            format_bytes(summary.bytes_received as f64),
            format_bytes(per_second(summary.bytes_received))
        )
        .cyan(),
    );

    let apdex = summary.apdex(apdex_threshold_ms);

    print_row(
        &format!("Apdex (T={apdex_threshold_ms}ms)"),
        format!("{apdex:.3} ({})", apdex_rating(apdex)).yellow(),
    );

    println!(
        "\n{}",
        format!("Finished in {}", format_duration(summary.duration)).bright_black()
    );
}

/// Suggests a likely cause when most requests failed.
///
/// Only shown for runs large enough to be meaningful (at least 10 measured
/// requests) in which more than half of the requests failed.
fn failure_hint(summary: &RunSummary) -> Option<String> {
    if summary.measured_requests < 10 || summary.error_rate_percent() <= 50.0 {
        return None;
    }

    let top_error = summary
        .error_counts
        .iter()
        .max_by_key(|(_, count)| **count)
        .map(|(message, _)| message.as_str());

    let advice = match top_error {
        Some(error) if error.starts_with("Connection Refused") => {
            "Nothing accepted the connection: is the server running and the address/port right?"
        }
        Some(error) if error.starts_with("DNS Error") => {
            "The host name did not resolve: check the URL and your network/DNS."
        }
        Some(error) if error.starts_with("TLS Error") => {
            "The TLS handshake failed: use --cacert for a private CA, or --insecure for a test certificate."
        }
        Some(error) if error.starts_with("Timeout") => {
            "Requests are timing out: the server is overloaded or unreachable; consider a higher --timeout or fewer workers."
        }
        Some(error) if error.starts_with("File Descriptor Limit") => {
            "Cannon ran out of file descriptors: raise `ulimit -n` or use fewer workers."
        }
        Some(error) if error.starts_with("Response Framing") => {
            "The TCP response length does not match: set --tcp-read-bytes or --tcp-read-until."
        }
        Some(_) => "Most requests failed: check the error breakdown below.",
        None => {
            "Most responses were not successful: check the URL, authentication and --expect-status \
             (see the status code distribution below)."
        }
    };

    Some(format!(
        "{:.0}% of the requests failed. {advice}",
        summary.error_rate_percent()
    ))
}

fn print_section(title: &str) {
    println!(
        "\n{} {}",
        format!("▌ {title}").bold().bright_white(),
        "─"
            .repeat(46usize.saturating_sub(title.len()))
            .bright_black()
    );
}

fn print_row(label: &str, value: impl std::fmt::Display) {
    println!("  {:<18} {}", label, value);
}

/// Formats a duration compactly (`850ms`, `12.3s`, `2m05s`).
fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs_f64();

    if seconds < 1.0 {
        format!("{}ms", duration.as_millis())
    } else if seconds < 60.0 {
        format!("{seconds:.1}s")
    } else {
        format!(
            "{}m{:02}s",
            (seconds / 60.0) as u64,
            (seconds % 60.0) as u64
        )
    }
}

/// Formats a byte count with a binary unit.
fn format_bytes(bytes: f64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];

    let mut value = bytes;
    let mut unit = 0;

    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{value:.0} B")
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

/// Standard Apdex quality bands.
fn apdex_rating(score: f64) -> &'static str {
    match score {
        s if s >= 0.94 => "excellent",
        s if s >= 0.85 => "good",
        s if s >= 0.70 => "fair",
        s if s >= 0.50 => "poor",
        _ => "unacceptable",
    }
}

/// Explains how the latency figures above were measured.
fn print_latency_basis(summary: &RunSummary) {
    if summary.open_loop {
        let service = &summary.service_histogram;

        println!(
            "{}",
            "Latency is measured from each request's scheduled send time (open-loop), \
             so queueing delay is included."
                .bright_black()
        );
        println!(
            "{}",
            format!(
                "Service time only (excluding queueing): p50 {:.2}ms | p99 {:.2}ms | max {:.2}ms",
                histogram_percentile_ms(service, 0.5),
                histogram_percentile_ms(service, 0.99),
                to_ms(service.max())
            )
            .bright_black()
        );
    } else {
        println!(
            "{}",
            "Latency is the service time of each request (closed-loop). Use --rps for \
             constant-rate load measured from the scheduled send time."
                .bright_black()
        );
    }
}

fn print_failure_details(summary: &RunSummary) {
    let failures = summary.failures;

    if failures == 0 {
        return;
    }

    print_section("FAILURES");

    let mut sorted_errors: Vec<_> = summary.error_counts.iter().collect();

    sorted_errors.sort_by_key(|item| std::cmp::Reverse(*item.1));

    let explained: u64 = summary.error_counts.values().sum();

    for (err, count) in sorted_errors {
        let perc = (*count as f64 / failures as f64) * 100.0;

        println!(
            "  {:<52} {:>8} ({:>5.1}%)",
            err.yellow(),
            count.to_string().bright_white(),
            perc
        );
    }

    let http_errors = failures.saturating_sub(explained);

    if http_errors > 0 {
        let perc = (http_errors as f64 / failures as f64) * 100.0;

        println!(
            "  {:<52} {:>8} ({:>5.1}%)",
            "HTTP error status (see above)".yellow(),
            http_errors.to_string().bright_white(),
            perc
        );
    }
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
    fn durations_and_sizes_are_human_readable() {
        assert_eq!(format_duration(Duration::from_millis(850)), "850ms");
        assert_eq!(format_duration(Duration::from_millis(12_340)), "12.3s");
        assert_eq!(format_duration(Duration::from_secs(125)), "2m05s");
        assert_eq!(format_bytes(512.0), "512 B");
        assert_eq!(format_bytes(1_572_864.0), "1.50 MB");
        assert_eq!(apdex_rating(0.99), "excellent");
        assert_eq!(apdex_rating(0.2), "unacceptable");
    }

    fn summary_with(failures: u64, successes: u64, error: Option<&str>) -> RunSummary {
        RunSummary {
            histogram: Histogram::<u64>::new(3).unwrap(),
            service_histogram: Histogram::<u64>::new(3).unwrap(),
            status_counts: HashMap::new(),
            error_counts: error
                .map(|message| HashMap::from([(message.to_string(), failures)]))
                .unwrap_or_default(),
            assertion_failures: 0,
            successes,
            failures,
            measured_requests: successes + failures,
            bytes_sent: 0,
            bytes_received: 0,
            duration: Duration::from_secs(1),
            open_loop: false,
            interrupted: false,
        }
    }

    #[test]
    fn failure_hint_points_at_the_likely_cause() {
        let refused = summary_with(90, 10, Some("Connection Refused: tcp connect error"));

        assert!(failure_hint(&refused).unwrap().contains("90%"));
        assert!(failure_hint(&refused).unwrap().contains("server running"));

        let statuses = summary_with(80, 20, None);

        assert!(failure_hint(&statuses).unwrap().contains("--expect-status"));
    }

    #[test]
    fn failure_hint_stays_quiet_for_healthy_or_tiny_runs() {
        assert!(failure_hint(&summary_with(5, 95, None)).is_none());
        assert!(failure_hint(&summary_with(50, 50, None)).is_none());
        assert!(failure_hint(&summary_with(4, 1, None)).is_none());
    }

    #[test]
    fn test_to_ms_conversion() {
        assert_eq!(to_ms(1_000), 1.0);
        assert_eq!(to_ms(500), 0.5);
        assert_eq!(to_ms(1_500_000), 1500.0);
        assert_eq!(to_ms(0), 0.0);
    }

    #[test]
    fn percentile_label_hides_floating_point_noise() {
        assert_eq!(percentile_label(0.5), "p50");
        assert_eq!(percentile_label(0.95), "p95");
        assert_eq!(percentile_label(0.99), "p99");
        assert_eq!(percentile_label(0.999), "p99.9");
        assert_eq!(percentile_label(0.07), "p7");
        assert_eq!(percentile_label(0.57), "p57");
        assert_eq!(percentile_label(1.0), "p100");
    }

    #[test]
    fn html_report_is_self_contained() {
        let path = std::env::temp_dir().join(format!("cannon-offline-{}.html", std::process::id()));

        generate_html_report(path.to_str().unwrap(), "{}").unwrap();

        let html = std::fs::read_to_string(&path).unwrap();

        assert!(!html.contains("cdn.jsdelivr"), "no external script allowed");
        assert!(!html.contains("/*CHARTJS*/"));
        assert!(html.contains("Chart"));

        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn html_report_escapes_script_terminators() {
        let path =
            std::env::temp_dir().join(format!("cannon-html-report-{}.html", std::process::id()));

        let report_json = r#"{"target":"</script><script>alert(1)</script>"}"#;

        generate_html_report(path.to_str().unwrap(), report_json).unwrap();

        let html = std::fs::read_to_string(&path).unwrap();

        assert!(!html.contains("</script><script>alert(1)</script>"));
        assert!(html.contains("\\u003c/script\\u003e"));

        std::fs::remove_file(path).unwrap();
    }
}
