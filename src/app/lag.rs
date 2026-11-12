//! Detects a load generator that cannot keep up with its own work.
//!
//! When the machine running Cannon is saturated, requests wait for a free
//! thread inside Cannon and that wait is counted as latency of the *target*.
//! A tiny task that repeatedly sleeps and measures how late it wakes up shows
//! this directly, on the same scheduler the workers use.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::task::JoinHandle;

const PERIOD: Duration = Duration::from_millis(50);

/// A wake-up later than this is counted as "slow".
const SLOW: Duration = Duration::from_millis(10);

/// Too few checks (a very short run) say nothing reliable.
const MIN_SAMPLES: u64 = 20;

/// Share of slow wake-ups, in percent, from which the run is flagged.
const SLOW_SHARE_PERCENT: u64 = 20;

/// Counters written by the probe task.
#[derive(Default)]
pub struct LagProbe {
    samples: AtomicU64,
    slow: AtomicU64,
    max_micros: AtomicU64,
}

impl LagProbe {
    /// Starts the probe on the current runtime; abort the handle to stop it.
    #[must_use]
    pub fn start() -> (Arc<Self>, JoinHandle<()>) {
        let probe = Arc::new(Self::default());
        let writer = probe.clone();

        let handle = tokio::spawn(async move {
            loop {
                let started = Instant::now();
                tokio::time::sleep(PERIOD).await;
                writer.record(started.elapsed().saturating_sub(PERIOD));
            }
        });

        (probe, handle)
    }

    fn record(&self, lag: Duration) {
        self.samples.fetch_add(1, Ordering::Relaxed);

        if lag >= SLOW {
            self.slow.fetch_add(1, Ordering::Relaxed);
        }

        let micros = u64::try_from(lag.as_micros()).unwrap_or(u64::MAX);
        self.max_micros.fetch_max(micros, Ordering::Relaxed);
    }

    /// A warning to print when the generator was overloaded during the run.
    #[must_use]
    pub fn warning(&self) -> Option<String> {
        overload_warning(
            self.samples.load(Ordering::Relaxed),
            self.slow.load(Ordering::Relaxed),
            self.max_micros.load(Ordering::Relaxed),
        )
    }
}

fn overload_warning(samples: u64, slow: u64, max_micros: u64) -> Option<String> {
    if samples < MIN_SAMPLES || slow * 100 < samples * SLOW_SHARE_PERCENT {
        return None;
    }

    Some(format!(
        "⚠️  Cannon itself was overloaded: {}% of its scheduling checks were late by more \
         than {} ms (worst {:.0} ms). Part of the latency above is time spent waiting for a \
         free thread here, not the target's. Use fewer --workers, close other programs or \
         run from a machine with more cores.",
        slow * 100 / samples,
        SLOW.as_millis(),
        max_micros as f64 / 1000.0
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_healthy_run_is_not_flagged() {
        assert!(overload_warning(200, 3, 12_000).is_none());
    }

    #[test]
    fn a_mostly_late_run_is_flagged_with_the_numbers() {
        let warning = overload_warning(100, 40, 85_000).unwrap();

        assert!(warning.contains("40%"), "{warning}");
        assert!(warning.contains("85 ms"), "{warning}");
    }

    #[test]
    fn very_short_runs_are_never_flagged() {
        assert!(overload_warning(5, 5, 1_000_000).is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn the_probe_counts_wakeups_and_notices_a_blocked_thread() {
        let (probe, handle) = LagProbe::start();

        // Let the probe begin its first sleep before blocking the thread.
        tokio::task::yield_now().await;

        // Block the only thread: the probe cannot wake up on time.
        std::thread::sleep(Duration::from_millis(200));
        tokio::time::sleep(Duration::from_millis(120)).await;
        handle.abort();

        assert!(probe.samples.load(Ordering::Relaxed) >= 1);
        assert!(probe.max_micros.load(Ordering::Relaxed) >= 100_000);
        assert!(probe.slow.load(Ordering::Relaxed) >= 1);
    }
}
