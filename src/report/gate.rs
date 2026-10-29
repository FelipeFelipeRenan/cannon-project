//! Pass/fail evaluation used to gate CI pipelines.
//!
//! A run can fail the gate for two independent reasons:
//!
//! - its **p99 latency regressed** by more than the tolerance compared with a
//!   saved baseline, or
//! - its **error rate** is higher than allowed.
//!
//! Both checks matter: a service that is completely down answers (with
//! errors) very quickly, so latency alone would make it look *faster*.

use super::summary::RunSummary;
use serde::{Deserialize, Serialize};

/// Version of the baseline file format written by [`Baseline::from_summary`].
pub const BASELINE_SCHEMA_VERSION: u32 = 2;

/// Reference numbers saved from a known-good run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Baseline {
    /// Version of the file format (`0` for files written by older releases).
    #[serde(default)]
    pub schema_version: u32,
    /// p99 response time of successful requests, in milliseconds.
    pub p99_ms: f64,
    /// Requests per second achieved by the run.
    #[serde(default)]
    pub rps: f64,
    /// Percentage of requests that failed (`0` for files from older releases).
    #[serde(default)]
    pub error_rate_percent: f64,
    /// Latency measurement mode (`open-loop` or `closed-loop`), if recorded.
    #[serde(default)]
    pub latency_mode: Option<String>,
}

impl Baseline {
    /// Captures a baseline from a finished run.
    ///
    /// # Errors
    ///
    /// Refuses to build a baseline from a run without any successful request:
    /// it would describe a broken service, not a reference to compare with.
    pub fn from_summary(summary: &RunSummary) -> Result<Self, String> {
        if summary.successes == 0 {
            return Err(
                "cannot save a baseline: no request succeeded, so there is no latency to record"
                    .to_string(),
            );
        }

        Ok(Self {
            schema_version: BASELINE_SCHEMA_VERSION,
            p99_ms: summary.percentile_ms(0.99),
            rps: summary.actual_rps(),
            error_rate_percent: summary.error_rate_percent(),
            latency_mode: Some(summary.latency_mode().to_string()),
        })
    }

    /// Parses and validates the contents of a baseline file.
    ///
    /// # Errors
    ///
    /// Returns a description of the problem if the JSON is malformed or the
    /// recorded values are unusable.
    pub fn parse(json: &str) -> Result<Self, String> {
        let baseline: Self = serde_json::from_str(json)
            .map_err(|error| format!("invalid baseline file: {error}"))?;

        if !baseline.p99_ms.is_finite() || baseline.p99_ms <= 0.0 {
            return Err("baseline 'p99_ms' must be a number greater than 0".to_string());
        }

        if !baseline.error_rate_percent.is_finite() || baseline.error_rate_percent < 0.0 {
            return Err("baseline 'error_rate_percent' must be 0 or greater".to_string());
        }

        Ok(baseline)
    }
}

/// Thresholds applied by [`evaluate`].
#[derive(Debug, Clone, Copy)]
pub struct GateOptions {
    /// Maximum accepted p99 slowdown versus the baseline, in percent.
    pub tolerance_percent: f64,
    /// Maximum accepted share of failed requests, in percent.
    ///
    /// When `None` and a baseline is supplied, the baseline's own error rate
    /// is the limit; without a baseline no error-rate check is made.
    pub max_error_rate_percent: Option<f64>,
}

/// Result of [`evaluate`].
#[derive(Debug, Default, PartialEq)]
pub struct GateOutcome {
    /// Informational lines (improvements, warnings) to show to the user.
    pub notes: Vec<String>,
    /// Reasons why the gate failed. Empty when the run passed.
    pub failures: Vec<String>,
}

impl GateOutcome {
    /// Returns `true` when no check failed.
    pub fn passed(&self) -> bool {
        self.failures.is_empty()
    }
}

/// Decides whether a run passes the configured gate.
pub fn evaluate(
    summary: &RunSummary,
    baseline: Option<&Baseline>,
    options: &GateOptions,
) -> GateOutcome {
    let mut outcome = GateOutcome::default();

    if let Some(baseline) = baseline {
        compare_latency(summary, baseline, options, &mut outcome);
    }

    let allowed_error_rate = options
        .max_error_rate_percent
        .or_else(|| baseline.map(|baseline| baseline.error_rate_percent));

    if let Some(allowed) = allowed_error_rate {
        let actual = summary.error_rate_percent();

        if actual > allowed + 1e-9 {
            outcome.failures.push(format!(
                "❌ Error rate {actual:.2}% exceeds the allowed {allowed:.2}% \
                 ({} of {} requests failed)",
                summary.failures, summary.measured_requests
            ));
        }
    }

    outcome
}

fn compare_latency(
    summary: &RunSummary,
    baseline: &Baseline,
    options: &GateOptions,
    outcome: &mut GateOutcome,
) {
    if summary.successes == 0 {
        outcome.failures.push(
            "❌ Regression detected: no request succeeded, so latency cannot be compared \
             with the baseline"
                .to_string(),
        );

        return;
    }

    if let Some(recorded) = &baseline.latency_mode {
        if recorded != summary.latency_mode() {
            outcome.notes.push(format!(
                "⚠️ The baseline was recorded in {recorded} mode but this run is {}: \
                 the latencies are not directly comparable",
                summary.latency_mode()
            ));
        }
    }

    let current = summary.percentile_ms(0.99);

    if current > baseline.p99_ms {
        let degradation = (current - baseline.p99_ms) / baseline.p99_ms * 100.0;

        outcome.notes.push(format!(
            "⚠️ Performance degradation: {degradation:.2}% slower than baseline \
             (p99 {current:.2}ms vs {:.2}ms)",
            baseline.p99_ms
        ));

        if degradation > options.tolerance_percent {
            outcome.failures.push(format!(
                "❌ Regression detected: p99 is {degradation:.2}% slower than the baseline, \
                 above the tolerance of {:.2}%",
                options.tolerance_percent
            ));
        }
    } else {
        let improvement = (baseline.p99_ms - current) / baseline.p99_ms * 100.0;

        outcome.notes.push(format!(
            "🚀 Performance improvement: {improvement:.2}% faster than baseline \
             (p99 {current:.2}ms vs {:.2}ms)",
            baseline.p99_ms
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hdrhistogram::Histogram;
    use std::collections::HashMap;
    use std::time::Duration;

    fn summary(p99_ms: u64, successes: u64, failures: u64) -> RunSummary {
        let mut histogram = Histogram::<u64>::new(3).unwrap();

        for _ in 0..successes {
            histogram.record(p99_ms * 1000).unwrap();
        }

        RunSummary {
            service_histogram: histogram.clone(),
            histogram,
            status_counts: HashMap::new(),
            error_counts: HashMap::new(),
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

    fn baseline(p99_ms: f64, error_rate_percent: f64) -> Baseline {
        Baseline {
            schema_version: BASELINE_SCHEMA_VERSION,
            p99_ms,
            rps: 100.0,
            error_rate_percent,
            latency_mode: Some("closed-loop".to_string()),
        }
    }

    const OPTIONS: GateOptions = GateOptions {
        tolerance_percent: 10.0,
        max_error_rate_percent: None,
    };

    #[test]
    fn passes_when_latency_is_within_tolerance() {
        let outcome = evaluate(&summary(105, 100, 0), Some(&baseline(100.0, 0.0)), &OPTIONS);

        assert!(outcome.passed(), "{outcome:?}");
        assert!(outcome.notes[0].contains("degradation"));
    }

    #[test]
    fn fails_when_latency_regresses_beyond_tolerance() {
        let outcome = evaluate(&summary(150, 100, 0), Some(&baseline(100.0, 0.0)), &OPTIONS);

        assert!(!outcome.passed());
        assert!(outcome.failures[0].contains("Regression detected"));
        assert!(outcome.failures[0].contains("tolerance of 10.00%"));
    }

    #[test]
    fn improvement_passes() {
        let outcome = evaluate(&summary(50, 100, 0), Some(&baseline(100.0, 0.0)), &OPTIONS);

        assert!(outcome.passed());
        assert!(outcome.notes[0].contains("improvement"));
    }

    #[test]
    fn a_service_that_is_down_never_passes() {
        // Regression test: 100% failures used to be reported as an
        // "improvement" because failed requests are fast.
        let outcome = evaluate(&summary(1, 0, 300), Some(&baseline(100.0, 0.0)), &OPTIONS);

        assert!(!outcome.passed());
        assert!(outcome
            .failures
            .iter()
            .any(|failure| failure.contains("no request succeeded")));
        assert!(outcome
            .failures
            .iter()
            .any(|failure| failure.contains("Error rate 100.00%")));
    }

    #[test]
    fn error_rate_defaults_to_the_baseline_error_rate() {
        // 5% errors against a clean baseline fails even though latency is fine.
        let outcome = evaluate(&summary(100, 95, 5), Some(&baseline(100.0, 0.0)), &OPTIONS);

        assert!(!outcome.passed());
        assert!(outcome.failures[0].contains("Error rate 5.00%"));

        // The same error rate is accepted when the baseline already had it.
        let outcome = evaluate(&summary(100, 95, 5), Some(&baseline(100.0, 5.0)), &OPTIONS);

        assert!(outcome.passed(), "{outcome:?}");
    }

    #[test]
    fn explicit_max_error_rate_overrides_the_baseline() {
        let options = GateOptions {
            max_error_rate_percent: Some(10.0),
            ..OPTIONS
        };

        let outcome = evaluate(&summary(100, 95, 5), Some(&baseline(100.0, 0.0)), &options);

        assert!(outcome.passed(), "{outcome:?}");
    }

    #[test]
    fn max_error_rate_works_without_a_baseline() {
        let options = GateOptions {
            max_error_rate_percent: Some(1.0),
            ..OPTIONS
        };

        assert!(evaluate(&summary(10, 100, 0), None, &options).passed());
        assert!(!evaluate(&summary(10, 90, 10), None, &options).passed());
    }

    #[test]
    fn without_any_threshold_nothing_can_fail() {
        assert!(evaluate(&summary(10, 0, 100), None, &OPTIONS).passed());
    }

    #[test]
    fn mismatched_latency_modes_are_flagged() {
        let mut open = summary(100, 10, 0);
        open.open_loop = true;

        let outcome = evaluate(&open, Some(&baseline(100.0, 0.0)), &OPTIONS);

        assert!(outcome
            .notes
            .iter()
            .any(|note| note.contains("not directly comparable")));
    }

    #[test]
    fn baseline_refuses_a_run_without_successes() {
        let error = Baseline::from_summary(&summary(1, 0, 10)).unwrap_err();

        assert!(error.contains("no request succeeded"));
    }

    #[test]
    fn baseline_round_trips_through_json() {
        let original = Baseline::from_summary(&summary(25, 40, 0)).unwrap();
        let json = serde_json::to_string(&original).unwrap();

        assert_eq!(Baseline::parse(&json).unwrap(), original);
        assert_eq!(original.schema_version, BASELINE_SCHEMA_VERSION);
    }

    #[test]
    fn old_baseline_files_are_still_accepted() {
        let parsed = Baseline::parse(r#"{"p99_ms": 12.5, "rps": 800.0}"#).unwrap();

        assert_eq!(parsed.p99_ms, 12.5);
        assert_eq!(parsed.error_rate_percent, 0.0);
        assert_eq!(parsed.schema_version, 0);
        assert!(parsed.latency_mode.is_none());
    }

    #[test]
    fn invalid_baselines_are_rejected() {
        assert!(Baseline::parse("not json").is_err());
        assert!(Baseline::parse(r#"{"rps": 1.0}"#).is_err());
        assert!(Baseline::parse(r#"{"p99_ms": 0}"#).is_err());
        assert!(Baseline::parse(r#"{"p99_ms": -3}"#).is_err());
        assert!(Baseline::parse(r#"{"p99_ms": 5, "error_rate_percent": -1}"#).is_err());
    }
}
