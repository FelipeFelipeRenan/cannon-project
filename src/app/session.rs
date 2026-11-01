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
}

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

    let buffer_size = std::cmp::min(args.workers as usize, 10_000).max(1);

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

                let elapsed = last_time.elapsed().as_secs_f64();

                if elapsed >= 0.1 && total > last_total {
                    let rps = (total - last_total) as f64 / elapsed;
                    pb.set_message(format!("| ⚡ {:.1} RPS", rps));
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
    })
}
