//! Aggregation of per-worker results into the numbers used by reports.

use crate::engine::worker::{SharedMetrics, WorkerResult};
use hdrhistogram::Histogram;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::time::Duration;

/// Everything measured during one load test, merged across workers.
///
/// The latency histograms only contain **successful** requests; failures are
/// reflected in the counters, the status-code distribution and the error
/// breakdown instead.
pub struct RunSummary {
    /// Response time of successful requests, in microseconds.
    ///
    /// For open-loop runs (`--rps`) this is measured from the scheduled send
    /// time; otherwise it equals the service time.
    pub histogram: Histogram<u64>,
    /// Service time of successful requests, in microseconds.
    pub service_histogram: Histogram<u64>,
    /// Count of responses per HTTP status code.
    pub status_counts: HashMap<u16, u64>,
    /// Count of failures per error description.
    pub error_counts: HashMap<String, u64>,
    /// Number of responses that did not satisfy the configured assertion.
    pub assertion_failures: u64,
    /// Number of successful measured requests.
    pub successes: u64,
    /// Number of failed measured requests.
    pub failures: u64,
    /// Number of measured requests that completed (successes + failures).
    pub measured_requests: u64,
    /// Total payload bytes sent.
    pub bytes_sent: u64,
    /// Total payload bytes received.
    pub bytes_received: u64,
    /// Duration of the measurement phase.
    pub duration: Duration,
    /// Whether requests were paced at a constant rate (open-loop).
    pub open_loop: bool,
    /// Whether the run was stopped before all requests were sent.
    pub interrupted: bool,
}

impl RunSummary {
    /// Merges the results of all workers.
    ///
    /// # Errors
    ///
    /// Returns a description of the problem if the latency histograms of the
    /// workers cannot be merged.
    pub fn aggregate(
        results: Vec<WorkerResult>,
        metrics: &SharedMetrics,
        duration: Duration,
        open_loop: bool,
    ) -> Result<Self, String> {
        let mut histogram = new_histogram()?;
        let mut service_histogram = new_histogram()?;
        let mut status_counts: HashMap<u16, u64> = HashMap::new();
        let mut error_counts: HashMap<String, u64> = HashMap::new();
        let mut assertion_failures = 0;

        for worker in results {
            histogram
                .add(&worker.histogram)
                .map_err(|error| format!("could not merge latency histograms: {error}"))?;
            service_histogram
                .add(&worker.service_histogram)
                .map_err(|error| format!("could not merge service-time histograms: {error}"))?;

            for (code, count) in worker.status_counts {
                *status_counts.entry(code).or_insert(0) += count;
            }

            for (message, count) in worker.error_counts {
                *error_counts.entry(message).or_insert(0) += count;
            }

            assertion_failures += worker.assertion_failures;
        }

        Ok(Self {
            histogram,
            service_histogram,
            status_counts,
            error_counts,
            assertion_failures,
            successes: metrics.successes.load(Ordering::Relaxed),
            failures: metrics.failures.load(Ordering::Relaxed),
            measured_requests: metrics.measured_requests.load(Ordering::Relaxed),
            bytes_sent: metrics.bytes_sent.load(Ordering::Relaxed),
            bytes_received: metrics.bytes_received.load(Ordering::Relaxed),
            duration,
            open_loop,
            interrupted: metrics.is_cancelled(),
        })
    }

    /// Requests per second actually achieved during the measurement phase.
    pub fn actual_rps(&self) -> f64 {
        let seconds = self.duration.as_secs_f64();

        if seconds > 0.0 {
            self.measured_requests as f64 / seconds
        } else {
            0.0
        }
    }

    /// Percentage (`0.0..=100.0`) of measured requests that failed.
    pub fn error_rate_percent(&self) -> f64 {
        if self.measured_requests == 0 {
            0.0
        } else {
            self.failures as f64 / self.measured_requests as f64 * 100.0
        }
    }

    /// Apdex score for the given satisfied threshold, in milliseconds.
    ///
    /// Requests that failed count as *frustrated*, as the Apdex
    /// specification requires, so errors pull the score down.
    pub fn apdex(&self, threshold_ms: u64) -> f64 {
        if self.measured_requests == 0 {
            return 0.0;
        }

        let threshold_us = threshold_ms.saturating_mul(1000);

        let satisfied = self.histogram.count_between(0, threshold_us);
        let tolerating = self.histogram.count_between(
            threshold_us.saturating_add(1),
            threshold_us.saturating_mul(4),
        );

        (satisfied as f64 + tolerating as f64 / 2.0) / self.measured_requests as f64
    }

    /// Response time at `quantile` (`0.0..=1.0`) in milliseconds, or `0.0`
    /// when no request succeeded.
    pub fn percentile_ms(&self, quantile: f64) -> f64 {
        histogram_percentile_ms(&self.histogram, quantile)
    }

    /// Human-readable name of the latency measurement mode.
    pub fn latency_mode(&self) -> &'static str {
        if self.open_loop {
            "open-loop"
        } else {
            "closed-loop"
        }
    }
}

/// Percentile of `histogram` in milliseconds (`0.0` for an empty histogram).
pub fn histogram_percentile_ms(histogram: &Histogram<u64>, quantile: f64) -> f64 {
    if histogram.is_empty() {
        0.0
    } else {
        histogram.value_at_quantile(quantile) as f64 / 1000.0
    }
}

fn new_histogram() -> Result<Histogram<u64>, String> {
    Histogram::<u64>::new(3).map_err(|error| format!("could not create a histogram: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn histogram_of(values_us: &[u64]) -> Histogram<u64> {
        let mut histogram = new_histogram().unwrap();

        for &value in values_us {
            histogram.record(value).unwrap();
        }

        histogram
    }

    fn summary(successes_us: &[u64], failures: u64) -> RunSummary {
        let successes = successes_us.len() as u64;

        RunSummary {
            histogram: histogram_of(successes_us),
            service_histogram: histogram_of(successes_us),
            status_counts: HashMap::new(),
            error_counts: HashMap::new(),
            assertion_failures: 0,
            successes,
            failures,
            measured_requests: successes + failures,
            bytes_sent: 0,
            bytes_received: 0,
            duration: Duration::from_secs(2),
            open_loop: false,
            interrupted: false,
        }
    }

    #[test]
    fn apdex_counts_failures_as_frustrated() {
        // 60 satisfied (40 ms), 30 tolerating (150 ms), 10 frustrated (300 ms).
        let mut values = vec![40_000; 60];
        values.extend(vec![150_000; 30]);
        values.extend(vec![300_000; 10]);

        let healthy = summary(&values, 0);

        assert!((healthy.apdex(50) - 0.75).abs() < 1e-9);

        // The same latencies plus 100 failed requests: half the traffic is
        // frustrated, so the score must drop to 0.375.
        let with_failures = summary(&values, 100);

        assert!((with_failures.apdex(50) - 0.375).abs() < 1e-9);
    }

    #[test]
    fn apdex_is_zero_when_nothing_was_measured() {
        assert_eq!(summary(&[], 0).apdex(50), 0.0);
    }

    #[test]
    fn error_rate_is_failures_over_measured_requests() {
        let summary = summary(&[1_000; 75], 25);

        assert!((summary.error_rate_percent() - 25.0).abs() < 1e-9);
        assert_eq!(summary.measured_requests, 100);
    }

    #[test]
    fn error_rate_is_zero_without_requests() {
        assert_eq!(summary(&[], 0).error_rate_percent(), 0.0);
    }

    #[test]
    fn actual_rps_uses_the_measurement_duration() {
        assert!((summary(&[1_000; 100], 0).actual_rps() - 50.0).abs() < 1e-9);
    }

    #[test]
    fn percentiles_of_an_empty_histogram_are_zero() {
        let empty = summary(&[], 10);

        assert_eq!(empty.percentile_ms(0.99), 0.0);
        assert_eq!(histogram_percentile_ms(&empty.service_histogram, 0.5), 0.0);
    }

    #[test]
    fn percentile_is_reported_in_milliseconds() {
        let values: Vec<u64> = (1..=100).map(|v| v * 1000).collect();

        // HdrHistogram keeps 3 significant figures, so allow 0.1% of error.
        assert!((summary(&values, 0).percentile_ms(0.5) - 50.0).abs() < 0.1);
        assert!((summary(&values, 0).percentile_ms(0.99) - 99.0).abs() < 0.2);
    }

    #[test]
    fn aggregate_merges_workers_and_counters() {
        let metrics = SharedMetrics::default();
        metrics.successes.store(3, Ordering::Relaxed);
        metrics.failures.store(1, Ordering::Relaxed);
        metrics.measured_requests.store(4, Ordering::Relaxed);

        let worker = |values: &[u64], status: u16, error: &str| WorkerResult {
            histogram: histogram_of(values),
            service_histogram: histogram_of(values),
            status_counts: HashMap::from([(status, values.len() as u64)]),
            error_counts: HashMap::from([(error.to_string(), 1)]),
            assertion_failures: 1,
        };

        let merged = RunSummary::aggregate(
            vec![
                worker(&[1_000, 2_000], 200, "boom"),
                worker(&[3_000], 200, "boom"),
            ],
            &metrics,
            Duration::from_secs(1),
            true,
        )
        .unwrap();

        assert_eq!(merged.histogram.len(), 3);
        assert_eq!(merged.status_counts.get(&200), Some(&3));
        assert_eq!(merged.error_counts.get("boom"), Some(&2));
        assert_eq!(merged.assertion_failures, 2);
        assert_eq!(merged.latency_mode(), "open-loop");
        assert!(!merged.interrupted);

        metrics.cancel();

        let again = RunSummary::aggregate(Vec::new(), &metrics, Duration::ZERO, false).unwrap();

        assert!(again.interrupted);
        assert_eq!(again.latency_mode(), "closed-loop");
    }
}
