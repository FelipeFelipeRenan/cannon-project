use crate::client::target::{Target, TargetResult};
use crate::payload::generator::PayloadTemplate;
use hdrhistogram::Histogram;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::task::JoinError;

/// Thread-safe metrics shared by all workers during a load test.
///
/// Metrics are stored in atomics so workers can update aggregate counters
/// concurrently without requiring a mutex on the hot path.
///
/// Warm-up requests are intentionally excluded from these counters.
#[derive(Default)]
pub struct SharedMetrics {
    /// Number of successful measured requests.
    pub successes: AtomicU64,
    /// Number of failed measured requests.
    pub failures: AtomicU64,
    /// Total number of bytes sent by measured requests.
    pub bytes_sent: AtomicU64,
    /// Total number of bytes received from measured requests.
    pub bytes_received: AtomicU64,
    /// Number of measured requests completed.
    pub measured_requests: AtomicU64,
}

/// Result produced by a worker after completing a load-test phase.
///
/// A worker accumulates request-level results locally and returns its
/// aggregate measurements when the phase finishes. The engine combines
/// the results from all workers to produce the final report.
pub struct WorkerResult {
    /// Number of requests completed by this worker.
    pub histogram: Histogram<u64>,
    /// Number of requests that completed successfully.
    pub status_counts: HashMap<u16, u64>,
    /// Number of requests that failed.
    pub error_counts: HashMap<String, u64>,
    /// Number if assertions that failed.
    pub assertion_failures: u64,
}

/// A single request result formatted for CSV output.
///
/// The fields are stored as strings because they are written directly to the
/// CSV representation of the load-test results.
pub struct CsvRecord {
    /// Elapsed time since the beginning of the measurement phase, in milliseconds.
    pub relative_ms: String,

    /// Result status of the request.
    pub status: String,

    /// Request latency in milliseconds.
    pub latency_ms: String,

    /// Error description, if the request failed.
    pub error: String,
}

#[derive(Clone, Copy)]
enum Job {
    Warmup,
    Measured,
}

struct PhaseConfig {
    count: Option<u32>,
    duration: Option<Duration>,
    workers: u32,
    template: Option<Arc<PayloadTemplate>>,
    rps: Option<u32>,
    ramp_up_duration: Option<Duration>,
    target: Arc<Target>,
    shared_metrics: Arc<SharedMetrics>,
    csv_tx: Option<mpsc::Sender<CsvRecord>>,
    start_time: Instant,
    job: Job,
}

fn record_result(
    res: &TargetResult,
    is_warmup: bool,
    shared: &SharedMetrics,
    histogram: &mut Histogram<u64>,
    status_counts: &mut HashMap<u16, u64>,
    error_counts: &mut HashMap<String, u64>,
    assertion_failures: &mut u64,
) {
    if is_warmup {
        return;
    }

    shared.measured_requests.fetch_add(1, Ordering::Relaxed);

    if res.success {
        shared.successes.fetch_add(1, Ordering::Relaxed);
    } else {
        shared.failures.fetch_add(1, Ordering::Relaxed);
    }

    shared
        .bytes_sent
        .fetch_add(res.bytes_sent, Ordering::Relaxed);

    shared
        .bytes_received
        .fetch_add(res.bytes_received, Ordering::Relaxed);

    let latency_us = res.duration.as_micros() as u64;
    let latency_us = latency_us.min(60_000_000);

    histogram
        .record(latency_us)
        .expect("latency must fit histogram bounds");

    if let Some(code) = res.status_code {
        *status_counts.entry(code).or_insert(0) += 1;
    }

    if let Some(err) = &res.error {
        *error_counts.entry(err.clone()).or_insert(0) += 1;
    }

    if !res.assertion_success {
        *assertion_failures += 1;
    }
}

fn ramp_up_schedule_seconds(
    request_number: u32,
    target_rps: u32,
    ramp_up_duration: Duration,
) -> f64 {
    let request_number = request_number as f64;
    let target_rps = target_rps as f64;
    let ramp_seconds = ramp_up_duration.as_secs_f64();

    let requests_during_ramp = 0.5 * target_rps * ramp_seconds;

    if request_number <= requests_during_ramp {
        ((2.0 * request_number * ramp_seconds) / target_rps).sqrt()
    } else {
        ramp_seconds + (request_number - requests_during_ramp) / target_rps
    }
}

async fn send_ramp_up_jobs(
    job_tx: &async_channel::Sender<Job>,
    job: Job,
    count: u32,
    target_rps: u32,
    ramp_up_duration: Duration,
) {
    let start = Instant::now();

    for request_number in 1..=count {
        let scheduled_seconds =
            ramp_up_schedule_seconds(request_number, target_rps, ramp_up_duration);

        let deadline = start + Duration::from_secs_f64(scheduled_seconds);

        tokio::time::sleep_until(deadline.into()).await;

        if job_tx.send(job).await.is_err() {
            break;
        }
    }
}

async fn run_phase(config: PhaseConfig) -> Result<Vec<WorkerResult>, JoinError> {
    let PhaseConfig {
        count,
        duration,
        workers,
        template,
        rps,
        ramp_up_duration,
        target,
        shared_metrics,
        csv_tx,
        start_time,
        job,
    } = config;

    let (job_tx, async_job_rx) = async_channel::bounded::<Job>(workers as usize);

    let mut handles = Vec::with_capacity(workers as usize);

    for _ in 0..workers {
        let template = template.clone();
        let rx = async_job_rx.clone();
        let target = target.clone();
        let shared = shared_metrics.clone();
        let csv_tx = csv_tx.clone();

        let handle = tokio::spawn(async move {
            let mut payload_buffer = Vec::with_capacity(1024);

            let mut local_hist = Histogram::<u64>::new_with_bounds(1, 60_000_000, 3).unwrap();

            let mut local_status = HashMap::new();
            let mut local_errors = HashMap::new();
            let mut local_assert_failures = 0;

            while let Ok(job) = rx.recv().await {
                if let Some(tpl) = &template {
                    tpl.render(&mut payload_buffer);
                }

                let payload_ref: &[u8] = if template.is_some() {
                    payload_buffer.as_slice()
                } else {
                    &[]
                };

                let res = target.fire(payload_ref).await;

                record_result(
                    &res,
                    matches!(job, Job::Warmup),
                    &shared,
                    &mut local_hist,
                    &mut local_status,
                    &mut local_errors,
                    &mut local_assert_failures,
                );

                if let Some(tx) = &csv_tx {
                    let rec = CsvRecord {
                        relative_ms: start_time.elapsed().as_millis().to_string(),
                        status: res
                            .status_code
                            .map(|c| c.to_string())
                            .unwrap_or_else(|| "N/A".to_string()),
                        latency_ms: res.duration.as_millis().to_string(),
                        error: res.error.unwrap_or_default(),
                    };

                    let _ = tx.send(rec).await;
                }
            }

            WorkerResult {
                histogram: local_hist,
                status_counts: local_status,
                error_counts: local_errors,
                assertion_failures: local_assert_failures,
            }
        });

        handles.push(handle);
    }

    match (count, duration) {
        // Measurement phase: send exactly `count` requests.
        (Some(count), None) => {
            if let Some(ramp_up_duration) = ramp_up_duration {
                let rps = rps.expect("ramp-up requires rps");

                send_ramp_up_jobs(&job_tx, job, count, rps, ramp_up_duration).await;
            } else if let Some(rps) = rps {
                let mut interval = tokio::time::interval(Duration::from_secs_f64(1.0 / rps as f64));

                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Burst);

                for _ in 0..count {
                    interval.tick().await;

                    if job_tx.send(job).await.is_err() {
                        break;
                    }
                }
            } else {
                for _ in 0..count {
                    if job_tx.send(job).await.is_err() {
                        break;
                    }
                }
            }
        }

        // Warm-up phase: generate traffic for the configured duration.
        (None, Some(duration)) => {
            let deadline = Instant::now() + duration;

            if let Some(rps) = rps {
                let mut interval = tokio::time::interval(Duration::from_secs_f64(1.0 / rps as f64));

                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Burst);

                loop {
                    interval.tick().await;

                    if Instant::now() >= deadline {
                        break;
                    }

                    if job_tx.send(job).await.is_err() {
                        break;
                    }
                }
            } else {
                while Instant::now() < deadline {
                    if job_tx.send(job).await.is_err() {
                        break;
                    }
                }
            }
        }

        _ => {}
    }

    drop(job_tx);

    let mut results = Vec::with_capacity(handles.len());

    for handle in handles {
        results.push(handle.await?);
    }

    Ok(results)
}

/// Runs the configured workers for a load-test phase.
///
/// Workers execute independently and produce a [`WorkerResult`] when the
/// phase finishes. The returned duration represents the elapsed time of
/// this phase.
///
/// The caller is responsible for determining whether the phase represents
/// warm-up traffic or measured traffic.
///
/// # Errors
///
/// Returns a [`tokio::task::JoinError`] if a worker task cannot be joined
/// successfully.
#[allow(clippy::too_many_arguments)]
pub async fn run_workers(
    count: u32,
    workers: u32,
    template: Option<Arc<PayloadTemplate>>,
    rps: Option<u32>,
    target: Arc<Target>,
    shared_metrics: Arc<SharedMetrics>,
    csv_tx: Option<mpsc::Sender<CsvRecord>>,
    start_time: Instant,
    warmup_duration: Duration,
    ramp_up_duration: Option<Duration>,
) -> Result<(Vec<WorkerResult>, Duration), JoinError> {
    if warmup_duration > Duration::ZERO {
        run_phase(PhaseConfig {
            count: None,
            duration: Some(warmup_duration),
            workers,
            template: template.clone(),
            rps,
            ramp_up_duration: None,
            target: target.clone(),
            shared_metrics: shared_metrics.clone(),
            csv_tx: csv_tx.clone(),
            start_time,
            job: Job::Warmup,
        })
        .await?;
    }

    let measurement_start = Instant::now();

    let results = run_phase(PhaseConfig {
        count: Some(count),
        duration: None,
        workers,
        template,
        rps,
        ramp_up_duration,
        target,
        shared_metrics,
        csv_tx,
        start_time,
        job: Job::Measured,
    })
    .await?;

    let measurement_duration = measurement_start.elapsed();

    Ok((results, measurement_duration))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ramp_up_starts_at_zero_and_increases_request_rate() {
        let ramp = Duration::from_secs(10);

        let first = ramp_up_schedule_seconds(1, 1000, ramp);
        let second = ramp_up_schedule_seconds(2, 1000, ramp);
        let third = ramp_up_schedule_seconds(3, 1000, ramp);

        assert!(first > 0.0);
        assert!(second > first);
        assert!(third > second);
    }

    #[test]
    fn ramp_up_reaches_target_rate_at_end_of_ramp() {
        let ramp = Duration::from_secs(10);
        let target_rps = 1000;

        let requests_during_ramp = (0.5 * target_rps as f64 * 10.0) as u32;

        let deadline = ramp_up_schedule_seconds(requests_during_ramp, target_rps, ramp);

        assert!((deadline - 10.0).abs() < 0.001);
    }

    #[test]
    fn ramp_up_continues_at_constant_rate_after_ramp() {
        let ramp = Duration::from_secs(10);
        let target_rps = 1000;

        let requests_during_ramp = 5000;

        let first_after_ramp = ramp_up_schedule_seconds(requests_during_ramp + 1, target_rps, ramp);

        let second_after_ramp =
            ramp_up_schedule_seconds(requests_during_ramp + 2, target_rps, ramp);

        let interval = second_after_ramp - first_after_ramp;

        assert!((interval - 0.001).abs() < 0.000001);
    }

    #[test]
    fn ramp_up_request_count_matches_integrated_rate() {
        let target_rps = 5000;
        let ramp = Duration::from_secs(10);

        let expected_requests = 0.5 * target_rps as f64 * ramp.as_secs_f64();

        assert_eq!(expected_requests, 25000.0);

        let request_number = expected_requests as u32;

        let deadline = ramp_up_schedule_seconds(request_number, target_rps, ramp);

        assert!((deadline - 10.0).abs() < 0.001);
    }

    #[test]
    fn ramp_up_is_continuous_at_transition() {
        let target_rps = 1000;
        let ramp = Duration::from_secs(10);

        let requests_during_ramp = 5000;

        let last_ramp_request = ramp_up_schedule_seconds(requests_during_ramp, target_rps, ramp);

        let first_constant_request =
            ramp_up_schedule_seconds(requests_during_ramp + 1, target_rps, ramp);

        assert!((last_ramp_request - 10.0).abs() < 0.001);
        assert!((first_constant_request - 10.001).abs() < 0.001);
    }
}
