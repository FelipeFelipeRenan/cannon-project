use crate::client::target::{Target, TargetResult};
use crate::payload::generator::PayloadTemplate;
use hdrhistogram::Histogram;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinError;
use tokio::time::Instant as TokioInstant;

/// Thread-safe metrics shared by all workers during a load test.
///
/// Metrics are stored in atomics so workers can update aggregate counters
/// concurrently without requiring a mutex on the hot path.
///
/// Warm-up requests are intentionally excluded from these counters.
///
/// `SharedMetrics` also carries the cancellation signal used to stop a run
/// gracefully (for example on Ctrl-C): see [`SharedMetrics::cancel`].
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
    cancel: watch::Sender<bool>,
}

impl Default for SharedMetrics {
    fn default() -> Self {
        Self {
            successes: AtomicU64::new(0),
            failures: AtomicU64::new(0),
            bytes_sent: AtomicU64::new(0),
            bytes_received: AtomicU64::new(0),
            measured_requests: AtomicU64::new(0),
            cancel: watch::channel(false).0,
        }
    }
}

impl SharedMetrics {
    /// Returns `true` once a graceful stop has been requested.
    pub fn is_cancelled(&self) -> bool {
        *self.cancel.borrow()
    }

    /// Requests a graceful stop of the running load test.
    ///
    /// No new requests are scheduled and requests that are still in flight
    /// are abandoned (they are not counted). Everything that completed
    /// before the call is kept, so a partial report can still be produced.
    pub fn cancel(&self) {
        self.cancel.send_replace(true);
    }

    /// Completes when [`SharedMetrics::cancel`] has been called.
    async fn cancelled(&self) {
        let mut receiver = self.cancel.subscribe();
        let _ = receiver.wait_for(|cancelled| *cancelled).await;
    }
}

/// Result produced by a worker after completing a load-test phase.
///
/// A worker accumulates request-level results locally and returns its
/// aggregate measurements when the phase finishes. The engine combines
/// the results from all workers to produce the final report.
///
/// Both histograms only contain **successful** requests: failures are
/// counted separately so that fast failures (for example a refused
/// connection) cannot make latency look better than it is.
pub struct WorkerResult {
    /// Response time of successful requests, in microseconds.
    ///
    /// When requests are paced (`--rps`) this is measured from the moment the
    /// request was *scheduled* to be sent, so time spent waiting for a free
    /// worker is included (no coordinated omission). Without pacing it is
    /// the plain service time.
    pub histogram: Histogram<u64>,
    /// Service time of successful requests (from sending until the response
    /// was complete), in microseconds.
    pub service_histogram: Histogram<u64>,
    /// Count of responses per HTTP status code.
    pub status_counts: HashMap<u16, u64>,
    /// Count of failures per error description.
    pub error_counts: HashMap<String, u64>,
    /// Number of responses that did not satisfy the configured assertion.
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

    /// Response time in milliseconds (measured from the scheduled send time
    /// when requests are paced, otherwise equal to `service_ms`).
    pub latency_ms: String,

    /// Error description, if the request failed.
    pub error: String,

    /// Service time in milliseconds (from sending until the response was
    /// complete), excluding any time spent waiting to be sent.
    pub service_ms: String,
}

/// A unit of work handed from the scheduler to a worker.
#[derive(Clone, Copy)]
struct Job {
    warmup: bool,
    /// The instant the request was supposed to be sent, when pacing is on.
    scheduled: Option<TokioInstant>,
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
}

/// Per-worker accumulators, merged by the caller after the phase ends.
struct LocalStats {
    histogram: Histogram<u64>,
    service_histogram: Histogram<u64>,
    status_counts: HashMap<u16, u64>,
    error_counts: HashMap<String, u64>,
    assertion_failures: u64,
}

impl LocalStats {
    fn new() -> Self {
        // Auto-resizing histograms start tiny, which keeps thousands of
        // workers cheap (a fixed 24-hour range cost ~0.25 MB per worker).
        let histogram = || {
            Histogram::<u64>::new(3).expect("3 significant figures is a valid histogram precision")
        };

        Self {
            histogram: histogram(),
            service_histogram: histogram(),
            status_counts: HashMap::new(),
            error_counts: HashMap::new(),
            assertion_failures: 0,
        }
    }

    fn into_result(self) -> WorkerResult {
        WorkerResult {
            histogram: self.histogram,
            service_histogram: self.service_histogram,
            status_counts: self.status_counts,
            error_counts: self.error_counts,
            assertion_failures: self.assertion_failures,
        }
    }
}

fn record_result(
    res: &TargetResult,
    response_time: Duration,
    is_warmup: bool,
    shared: &SharedMetrics,
    stats: &mut LocalStats,
) {
    if is_warmup {
        return;
    }

    shared.measured_requests.fetch_add(1, Ordering::Relaxed);

    if res.success {
        shared.successes.fetch_add(1, Ordering::Relaxed);

        // Only successful requests feed the latency histograms. `record`
        // grows the auto-resizing histogram; `saturating_record` would clamp
        // every value to the histogram's *current* maximum (2 us at start).
        let _ = stats.histogram.record(response_time.as_micros() as u64);
        let _ = stats
            .service_histogram
            .record(res.duration.as_micros() as u64);
    } else {
        shared.failures.fetch_add(1, Ordering::Relaxed);
    }

    shared
        .bytes_sent
        .fetch_add(res.bytes_sent, Ordering::Relaxed);

    shared
        .bytes_received
        .fetch_add(res.bytes_received, Ordering::Relaxed);

    if let Some(code) = res.status_code {
        *stats.status_counts.entry(code).or_insert(0) += 1;
    }

    if let Some(err) = &res.error {
        *stats.error_counts.entry(err.clone()).or_insert(0) += 1;
    }

    if !res.assertion_success {
        stats.assertion_failures += 1;
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
    count: u32,
    target_rps: u32,
    ramp_up_duration: Duration,
) {
    let start = TokioInstant::now();

    for request_number in 1..=count {
        let scheduled_seconds =
            ramp_up_schedule_seconds(request_number, target_rps, ramp_up_duration);

        let deadline = start + Duration::from_secs_f64(scheduled_seconds);

        tokio::time::sleep_until(deadline).await;

        let job = Job {
            warmup: false,
            scheduled: Some(deadline),
        };

        if job_tx.send(job).await.is_err() {
            break;
        }
    }
}

/// Creates the pacing timer for `rps` requests per second.
///
/// Missed ticks are delivered in a burst, with their *original* deadlines, so
/// the offered load stays constant and delays show up as latency.
fn paced_interval(rps: u32) -> tokio::time::Interval {
    let period = Duration::from_secs_f64(1.0 / f64::from(rps.max(1))).max(Duration::from_nanos(1));

    let mut interval = tokio::time::interval(period);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Burst);
    interval
}

/// Feeds jobs to the workers according to the configured load shape.
async fn produce(
    job_tx: &async_channel::Sender<Job>,
    count: Option<u32>,
    duration: Option<Duration>,
    rps: Option<u32>,
    ramp_up_duration: Option<Duration>,
) {
    match (count, duration) {
        // Measurement phase: send exactly `count` requests.
        (Some(count), None) => {
            if let Some((ramp_up_duration, rps)) = ramp_up_duration.zip(rps) {
                send_ramp_up_jobs(job_tx, count, rps, ramp_up_duration).await;
            } else if let Some(rps) = rps {
                let mut interval = paced_interval(rps);

                for _ in 0..count {
                    let scheduled = interval.tick().await;

                    let job = Job {
                        warmup: false,
                        scheduled: Some(scheduled),
                    };

                    if job_tx.send(job).await.is_err() {
                        break;
                    }
                }
            } else {
                let job = Job {
                    warmup: false,
                    scheduled: None,
                };

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

            let job = Job {
                warmup: true,
                scheduled: None,
            };

            if let Some(rps) = rps {
                let mut interval = paced_interval(rps);

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
}

/// Executes jobs until the channel is closed (or the future is dropped).
async fn worker_loop(
    rx: async_channel::Receiver<Job>,
    template: Option<Arc<PayloadTemplate>>,
    target: Arc<Target>,
    shared: &SharedMetrics,
    csv_tx: Option<mpsc::Sender<CsvRecord>>,
    start_time: Instant,
    stats: &mut LocalStats,
) {
    let mut payload_buffer = Vec::with_capacity(1024);

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

        let service_time = res.duration;

        // Measuring from the scheduled send time (instead of the actual send
        // time) charges queueing delay to the request, which is what a real
        // client arriving on schedule would have experienced.
        let response_time = job.scheduled.map_or(service_time, |scheduled| {
            scheduled.elapsed().max(service_time)
        });

        record_result(&res, response_time, job.warmup, shared, stats);

        if job.warmup {
            continue;
        }

        if let Some(tx) = &csv_tx {
            let record = CsvRecord {
                relative_ms: start_time.elapsed().as_millis().to_string(),
                status: res
                    .status_code
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "N/A".to_string()),
                latency_ms: format_millis(response_time),
                error: res.error.unwrap_or_default(),
                service_ms: format_millis(service_time),
            };

            let _ = tx.send(record).await;
        }
    }
}

fn format_millis(duration: Duration) -> String {
    format!("{:.3}", duration.as_secs_f64() * 1000.0)
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
    } = config;

    let (job_tx, job_rx) = async_channel::bounded::<Job>(workers.max(1) as usize);

    let mut handles = Vec::with_capacity(workers as usize);

    for _ in 0..workers {
        let template = template.clone();
        let rx = job_rx.clone();
        let target = target.clone();
        let shared = shared_metrics.clone();
        let csv_tx = csv_tx.clone();

        handles.push(tokio::spawn(async move {
            let mut stats = LocalStats::new();

            // A single `select!` per worker (not per request) keeps the hot
            // path free of cancellation overhead. When cancelled, the loop
            // (and the request in flight) is dropped; `stats` keeps
            // everything recorded so far.
            tokio::select! {
                _ = worker_loop(rx, template, target, &shared, csv_tx, start_time, &mut stats) => {}
                _ = shared.cancelled() => {}
            }

            stats.into_result()
        }));
    }

    // Only workers may hold receivers: if they are all gone (cancellation),
    // `send` must fail instead of waiting forever.
    drop(job_rx);

    tokio::select! {
        _ = produce(&job_tx, count, duration, rps, ramp_up_duration) => {}
        _ = shared_metrics.cancelled() => {}
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
/// the measurement phase.
///
/// When `rps` is set the load is *open-loop*: requests are scheduled at a
/// constant rate regardless of how fast the target answers, and latency is
/// measured from the scheduled send time. Without `rps` every worker sends
/// its next request as soon as the previous one completes (*closed-loop*)
/// and latency is the plain service time.
///
/// Calling [`SharedMetrics::cancel`] ends the run early; the results
/// gathered so far are still returned.
///
/// `_start_time` is accepted for API compatibility. CSV timestamps are
/// relative to the beginning of the measurement phase.
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
    _start_time: Instant,
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
            csv_tx: None,
            start_time: Instant::now(),
        })
        .await?;
    }

    if shared_metrics.is_cancelled() {
        return Ok((Vec::new(), Duration::ZERO));
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
        start_time: measurement_start,
    })
    .await?;

    Ok((results, measurement_start.elapsed()))
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

    #[tokio::test]
    async fn run_workers_aggregates_requests_across_workers() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();

        let address = listener.local_addr().unwrap().to_string();

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();

            let responses = [1u8; 10];

            tokio::io::AsyncWriteExt::write_all(&mut stream, &responses)
                .await
                .unwrap();
        });

        let target = Arc::new(Target::new_tcp(&address, 1).await.unwrap());

        let metrics = Arc::new(SharedMetrics::default());

        let (results, duration) = tokio::time::timeout(
            Duration::from_secs(5),
            run_workers(
                10,
                2,
                None,
                None,
                target,
                metrics.clone(),
                None,
                Instant::now(),
                Duration::ZERO,
                None,
            ),
        )
        .await
        .expect("run_workers timed out")
        .unwrap();

        assert_eq!(metrics.measured_requests.load(Ordering::Relaxed), 10);
        assert_eq!(metrics.successes.load(Ordering::Relaxed), 10);
        assert_eq!(metrics.failures.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.bytes_sent.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.bytes_received.load(Ordering::Relaxed), 10);

        assert_eq!(results.len(), 2);

        let total_histogram_samples: u64 =
            results.iter().map(|result| result.histogram.len()).sum();

        assert_eq!(total_histogram_samples, 10);
        assert!(duration > Duration::ZERO);

        server.await.unwrap();
    }
}
