//! Runs the load test: starts the engine, shows live progress and reacts to
//! termination signals.

use super::csv_export::{self, CsvWriter};
use super::plan::RunPlan;
use super::signals::shutdown_signal;
use super::AppResult;
use crate::args::parser::Args;
use crate::client::target::Target;
use crate::engine::worker::{run_workers, SharedMetrics, WorkerResult};
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// What a finished (or interrupted) load test produced.
pub struct Completed {
    /// Per-shard results of the measurement phase.
    pub worker_results: Vec<WorkerResult>,
    /// Duration of the measurement phase.
    pub duration: Duration,
    /// Counters shared with the workers.
    pub metrics: Arc<SharedMetrics>,
    /// Exit status of the signal that interrupted the run, if any.
    pub interrupted_status: Option<u8>,
    /// Why the run was stopped early by `--abort-on-error-rate`, if it was.
    pub aborted: Option<String>,
}

/// `--abort-on-error-rate` is only evaluated after this many requests, so a
/// couple of early failures cannot stop a healthy test.
const MIN_REQUESTS_TO_ABORT: u64 = 20;

/// Runs the test described by `args` against `target`.
///
/// # Errors
///
/// Returns an error if the progress bar template or the CSV export fails or
/// if the engine task panics.
pub async fn run(args: &Args, plan: &RunPlan, target: Arc<Target>) -> AppResult<Completed> {
    let pb = ProgressBar::new(u64::from(args.count));
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.bold.green} [{elapsed_precise}] {bar:40.magenta/blue} {pos:>7}/{len:7} {msg}")
            .map_err(|error| format!("invalid progress bar template: {error}"))?
            .progress_chars("━╾─"),
    );

    let shared_metrics = Arc::new(SharedMetrics::default());

    // Each slot holds a batch of rows, so a few hundred keep the writer busy.
    let buffer_size = (args.workers as usize / 4).clamp(16, 4_096);

    let mut csv_tx = None;
    let mut csv_handle: Option<CsvWriter> = None;

    if let Some(path) = &args.csv {
        let (tx, handle) = csv_export::start(path, buffer_size)?;

        csv_tx = Some(tx);
        csv_handle = Some(handle);
    }

    let mut engine_handle = tokio::spawn(run_workers(
        args.count,
        args.workers,
        plan.template.clone(),
        args.rps,
        target,
        shared_metrics.clone(),
        csv_tx,
        Instant::now(),
        plan.warmup,
        plan.ramp_up,
    ));

    let shutdown = shutdown_signal();
    tokio::pin!(shutdown);

    let mut interrupted_status: Option<u8> = None;
    let mut aborted: Option<String> = None;
    let mut ticker = tokio::time::interval(Duration::from_millis(500));
    let mut last_total = 0;
    let mut last_time = Instant::now();

    let engine_result = loop {
        tokio::select! {
            result = &mut engine_handle => break result,
            _ = ticker.tick() => {
                let total = shared_metrics.successes.load(Ordering::Relaxed)
                    + shared_metrics.failures.load(Ordering::Relaxed);

                pb.set_position(total);

                if let (Some(limit), None) = (args.abort_on_error_rate, &aborted) {
                    let measured = shared_metrics.measured_requests.load(Ordering::Relaxed);
                    let failed = shared_metrics.failures.load(Ordering::Relaxed);

                    if measured >= MIN_REQUESTS_TO_ABORT
                        && failed as f64 / measured as f64 * 100.0 > limit
                    {
                        aborted = Some(format!(
                            "❌ Aborted: {:.1}% of the first {measured} requests failed, above \
                             --abort-on-error-rate {limit}%",
                            failed as f64 / measured as f64 * 100.0
                        ));

                        shared_metrics.cancel();
                    }
                }

                let elapsed = last_time.elapsed().as_secs_f64();

                if elapsed >= 0.1 && total > last_total {
                    let rps = (total - last_total) as f64 / elapsed;
                    pb.set_message(progress_message(rps, &shared_metrics));
                    last_total = total;
                    last_time = Instant::now();
                }
            }
            status = &mut shutdown, if interrupted_status.is_none() => {
                shared_metrics.cancel();
                interrupted_status = Some(status);

                println!(
                    "\n\n{}",
                    "⚠️ Interruption detected! Stopping and reporting what was measured so far \
                     (signal again to quit immediately)..."
                        .yellow()
                        .bold()
                );

                tokio::spawn(async {
                    shutdown_signal().await;
                    std::process::exit(130);
                });
            }
        }
    };

    let (worker_results, duration) = engine_result??;

    if interrupted_status.is_some() {
        pb.abandon_with_message("Interrupted");
    } else {
        pb.finish_with_message("Finished");
    }

    if let Some(handle) = csv_handle {
        handle.await??;

        if let Some(path) = &args.csv {
            println!("📊 Raw data exported to {}!", path.bright_cyan());
        }
    }

    Ok(Completed {
        worker_results,
        duration,
        metrics: shared_metrics,
        interrupted_status,
        aborted,
    })
}

/// The text next to the progress bar: throughput and, once there is data,
/// the running p50/p99 of response time.
fn progress_message(rps: f64, metrics: &SharedMetrics) -> String {
    match metrics.live_percentiles(&[0.5, 0.99]).as_deref() {
        Some(&[p50, p99]) => format!(
            "| ⚡ {rps:.1} RPS | p50 {} p99 {}",
            format_live_latency(p50),
            format_live_latency(p99)
        ),
        _ => format!("| ⚡ {rps:.1} RPS"),
    }
}

fn format_live_latency(micros: u64) -> String {
    if micros >= 1_000_000 {
        format!("{:.2}s", micros as f64 / 1_000_000.0)
    } else if micros >= 1_000 {
        format!("{:.1}ms", micros as f64 / 1_000.0)
    } else {
        format!("{micros}µs")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_latency_uses_a_readable_unit() {
        assert_eq!(format_live_latency(850), "850µs");
        assert_eq!(format_live_latency(12_340), "12.3ms");
        assert_eq!(format_live_latency(2_500_000), "2.50s");
    }

    #[test]
    fn the_message_has_no_percentiles_before_any_data() {
        assert_eq!(
            progress_message(10.0, &SharedMetrics::default()),
            "| ⚡ 10.0 RPS"
        );
    }
}
