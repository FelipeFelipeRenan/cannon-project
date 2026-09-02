use crate::client::target::{Target, TargetResult};
use crate::payload::generator::PayloadTemplate;
use hdrhistogram::Histogram;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

#[derive(Default)]
pub struct SharedMetrics {
    pub successes: AtomicU64,
    pub failures: AtomicU64,
    pub bytes_sent: AtomicU64,
    pub bytes_received: AtomicU64,
    pub measured_requests: AtomicU64,
}

pub struct WorkerResult {
    pub histogram: Histogram<u64>,
    pub status_counts: HashMap<u16, u64>,
    pub error_counts: HashMap<String, u64>,
    pub assertion_failures: u64,
}

pub struct CsvRecord {
    pub relative_ms: String,
    pub status: String,
    pub latency_ms: String,
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

    let _ = histogram.record(res.duration.as_micros() as u64);

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

async fn run_phase(config: PhaseConfig) -> Vec<WorkerResult> {
    let PhaseConfig {
        count,
        duration,
        workers,
        template,
        rps,
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
            if let Some(rps) = rps {
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
        if let Ok(result) = handle.await {
            results.push(result);
        }
    }

    results
}

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
) -> (Vec<WorkerResult>, Duration) {
    if warmup_duration > Duration::ZERO {
        run_phase(PhaseConfig {
            count: None,
            duration: Some(warmup_duration),
            workers,
            template: template.clone(),
            rps,
            target: target.clone(),
            shared_metrics: shared_metrics.clone(),
            csv_tx: csv_tx.clone(),
            start_time,
            job: Job::Warmup,
        })
        .await;
    }

    let measurement_start = Instant::now();

    let results = run_phase(PhaseConfig {
        count: Some(count),
        duration: None,
        workers,
        template,
        rps,
        target,
        shared_metrics,
        csv_tx,
        start_time,
        job: Job::Measured,
    })
    .await;

    let measurement_duration = measurement_start.elapsed();

    (results, measurement_duration)
}
