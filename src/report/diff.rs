//! `--diff BEFORE AFTER`: compares two saved results without running a test.
//!
//! Works with the JSON written by `-o` (full numbers) and with baseline files
//! (only p99, throughput and error rate; the rest shows as "-").

use serde::Deserialize;

/// The numbers read from one result or baseline file.
#[derive(Debug, Deserialize, PartialEq)]
pub struct Side {
    #[serde(default)]
    total_requests: Option<u64>,
    #[serde(default)]
    error_rate_percent: Option<f64>,
    #[serde(default)]
    latency_mode: Option<String>,
    #[serde(default, alias = "rps")]
    actual_rps: Option<f64>,
    #[serde(default)]
    avg_ms: Option<f64>,
    #[serde(default)]
    p50_ms: Option<f64>,
    #[serde(default)]
    p95_ms: Option<f64>,
    p99_ms: f64,
    #[serde(default)]
    max_ms: Option<f64>,
}

impl Side {
    /// Parses a result (`-o`) or baseline file.
    ///
    /// # Errors
    ///
    /// Returns a description when the JSON is invalid or has no `p99_ms`.
    pub fn parse(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|error| {
            format!("not a Cannon result or baseline file (needs p99_ms): {error}")
        })
    }
}

/// How a change should be judged.
#[derive(Clone, Copy, PartialEq)]
enum Better {
    Lower,
    Higher,
}

/// The comparison, ready to print, and whether it counts as a regression.
#[derive(Debug, PartialEq)]
pub struct Diff {
    /// Lines of the table, one per metric.
    pub lines: Vec<String>,
    /// Notes the reader should know (different modes, ...).
    pub notes: Vec<String>,
    /// Reasons why the comparison fails the gate.
    pub failures: Vec<String>,
}

/// Compares `after` with `before`; a p99 more than `tolerance_percent` slower,
/// or a higher error rate, is a failure.
#[must_use]
pub fn compare(before: &Side, after: &Side, tolerance_percent: f64) -> Diff {
    let mut lines = vec![format!(
        "{:<14} {:>12} {:>12} {:>10}",
        "metric", "before", "after", "change"
    )];

    let mut row = |name: &str, unit: &str, a: Option<f64>, b: Option<f64>, better: Better| {
        lines.push(format_row(name, unit, a, b, better));
    };

    row("p50", " ms", before.p50_ms, after.p50_ms, Better::Lower);
    row("p95", " ms", before.p95_ms, after.p95_ms, Better::Lower);
    row(
        "p99",
        " ms",
        Some(before.p99_ms),
        Some(after.p99_ms),
        Better::Lower,
    );
    row("avg", " ms", before.avg_ms, after.avg_ms, Better::Lower);
    row("max", " ms", before.max_ms, after.max_ms, Better::Lower);
    row(
        "throughput",
        " req/s",
        before.actual_rps,
        after.actual_rps,
        Better::Higher,
    );
    row(
        "error rate",
        " %",
        before.error_rate_percent,
        after.error_rate_percent,
        Better::Lower,
    );
    row(
        "requests",
        "",
        before.total_requests.map(|n| n as f64),
        after.total_requests.map(|n| n as f64),
        Better::Higher,
    );

    let mut notes = Vec::new();
    let mut failures = Vec::new();

    if let (Some(a), Some(b)) = (&before.latency_mode, &after.latency_mode) {
        if a != b {
            notes.push(format!(
                "⚠️ before is {a} and after is {b}: the latencies are not directly comparable"
            ));
        }
    }

    if before.p99_ms > 0.0 {
        let change = (after.p99_ms - before.p99_ms) / before.p99_ms * 100.0;

        if change > tolerance_percent {
            failures.push(format!(
                "❌ p99 is {change:.2}% slower, above the tolerance of {tolerance_percent:.2}%"
            ));
        }
    }

    if let (Some(a), Some(b)) = (before.error_rate_percent, after.error_rate_percent) {
        if b > a + 1e-9 {
            failures.push(format!("❌ error rate rose from {a:.2}% to {b:.2}%"));
        }
    }

    Diff {
        lines,
        notes,
        failures,
    }
}

fn format_row(name: &str, unit: &str, a: Option<f64>, b: Option<f64>, better: Better) -> String {
    let show = |value: Option<f64>| match value {
        Some(value) => format!("{value:.2}{unit}"),
        None => "-".to_string(),
    };

    let change = match (a, b) {
        (Some(a), Some(b)) if a != 0.0 => {
            let percent = (b - a) / a * 100.0;
            let worse = match better {
                Better::Lower => percent > 0.0,
                Better::Higher => percent < 0.0,
            };
            let marker = if percent.abs() < 0.005 {
                " "
            } else if worse {
                "▲"
            } else {
                "▼"
            };

            format!("{marker}{percent:+.1}%")
        }
        _ => "-".to_string(),
    };

    format!("{name:<14} {:>12} {:>12} {change:>10}", show(a), show(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn side(p99: f64, rps: f64, errors: f64) -> Side {
        Side::parse(&format!(
            r#"{{"p99_ms": {p99}, "actual_rps": {rps}, "error_rate_percent": {errors}}}"#
        ))
        .unwrap()
    }

    #[test]
    fn reads_both_reports_and_baselines() {
        let baseline = Side::parse(r#"{"schema_version":1,"p99_ms":12.5,"rps":900.0}"#).unwrap();

        assert_eq!(baseline.actual_rps, Some(900.0));
        assert_eq!(baseline.p50_ms, None);
        assert!(Side::parse(r#"{"rps": 1}"#).is_err());
        assert!(Side::parse("nope").is_err());
    }

    #[test]
    fn a_slower_p99_beyond_the_tolerance_fails() {
        let diff = compare(&side(10.0, 1000.0, 0.0), &side(12.0, 900.0, 0.0), 5.0);

        assert_eq!(diff.failures.len(), 1, "{:?}", diff.failures);
        assert!(diff.failures[0].contains("20.00% slower"));
    }

    #[test]
    fn small_slowdowns_and_improvements_pass() {
        assert!(compare(&side(10.0, 1.0, 0.0), &side(10.4, 1.0, 0.0), 5.0)
            .failures
            .is_empty());
        assert!(compare(&side(10.0, 1.0, 0.0), &side(8.0, 1.0, 0.0), 5.0)
            .failures
            .is_empty());
    }

    #[test]
    fn a_higher_error_rate_fails_even_if_latency_improves() {
        let diff = compare(&side(10.0, 1.0, 0.0), &side(5.0, 1.0, 3.0), 5.0);

        assert_eq!(diff.failures.len(), 1);
        assert!(diff.failures[0].contains("error rate rose"));
    }

    #[test]
    fn rows_mark_which_direction_is_worse() {
        let diff = compare(&side(10.0, 1000.0, 0.0), &side(12.0, 900.0, 0.0), 50.0);
        let text = diff.lines.join("\n");

        assert!(text.contains("▲+20.0%"), "{text}"); // p99 up = worse
        assert!(text.contains("▲-10.0%"), "{text}"); // throughput down = worse
    }

    #[test]
    fn different_latency_modes_are_flagged() {
        let a = Side::parse(r#"{"p99_ms":1,"latency_mode":"open-loop"}"#).unwrap();
        let b = Side::parse(r#"{"p99_ms":1,"latency_mode":"closed-loop"}"#).unwrap();

        assert_eq!(compare(&a, &b, 5.0).notes.len(), 1);
    }
}
