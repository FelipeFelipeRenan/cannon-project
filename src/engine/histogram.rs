//! A lock-free latency histogram shared by many workers.
//!
//! Recording is a single `fetch_add` on a fixed-size array of counters, so
//! workers never block each other and memory does not depend on how many
//! workers there are. The bucket layout is the same as an `HdrHistogram`
//! with 3 significant figures, which makes converting to a
//! [`hdrhistogram::Histogram`] (for percentiles and merging) exact.

use hdrhistogram::Histogram;
use std::sync::atomic::{AtomicU64, Ordering};

/// Bits of the "half" sub-bucket count: 2^10 = 1024 values per bucket half,
/// which gives 3 significant decimal figures (0.1% resolution).
const HALF_BITS: u32 = 10;
const HALF_COUNT: u64 = 1 << HALF_BITS;
const SUB_BUCKET_COUNT: u64 = HALF_COUNT << 1;

/// Largest value that can be recorded, in microseconds (~71 minutes).
/// Larger values are clamped to it.
pub const MAX_VALUE_US: u64 = u32::MAX as u64;

/// Number of counters needed to cover `0..=MAX_VALUE_US`.
const COUNTERS: usize = (counter_index(MAX_VALUE_US) + 1) as usize;

/// Counter that holds `value` (same layout as `HdrHistogram`).
const fn counter_index(value: u64) -> u64 {
    if value < SUB_BUCKET_COUNT {
        return value;
    }

    // Which power-of-two bucket the value falls in, beyond the first.
    let bucket = (63 - value.leading_zeros()) - HALF_BITS;
    let sub_bucket = value >> bucket;

    ((bucket as u64 + 1) << HALF_BITS) + (sub_bucket - HALF_COUNT)
}

/// Lowest value that maps to counter `index` (inverse of [`counter_index`]).
fn counter_floor(index: u64) -> u64 {
    if index < SUB_BUCKET_COUNT {
        return index;
    }

    let bucket = (index >> HALF_BITS) - 1;
    let sub_bucket = (index & (HALF_COUNT - 1)) + HALF_COUNT;

    sub_bucket << bucket
}

/// Fixed-size, lock-free histogram of microsecond values.
pub struct AtomicHistogram {
    counters: Box<[AtomicU64]>,
}

impl Default for AtomicHistogram {
    fn default() -> Self {
        Self::new()
    }
}

impl AtomicHistogram {
    /// Creates an empty histogram (about 190 KB of zeroed counters).
    pub fn new() -> Self {
        Self {
            counters: (0..COUNTERS).map(|_| AtomicU64::new(0)).collect(),
        }
    }

    /// Records one value. Wait-free: a single relaxed atomic increment.
    #[inline]
    pub fn record(&self, value_us: u64) {
        let index = counter_index(value_us.min(MAX_VALUE_US)) as usize;

        self.counters[index].fetch_add(1, Ordering::Relaxed);
    }

    /// Converts the counters into an `HdrHistogram` (same precision).
    ///
    /// Call it after all writers have finished (for example after joining
    /// the worker tasks) to get a consistent snapshot.
    pub fn export(&self) -> Histogram<u64> {
        let mut histogram =
            Histogram::<u64>::new(3).expect("3 significant figures is a valid histogram precision");

        for (index, counter) in self.counters.iter().enumerate() {
            let count = counter.load(Ordering::Relaxed);

            if count > 0 {
                // An auto-resizing histogram accepts any value, so this
                // cannot fail; ignoring the result avoids a panic path.
                let _ = histogram.record_n(counter_floor(index as u64), count);
            }
        }

        histogram
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn layout_matches_hdrhistogram() {
        // Every recorded value must land in the same equivalent range as in
        // a real HdrHistogram: compare the exported value with it.
        for value in [
            0u64,
            1,
            999,
            1_000,
            2_047,
            2_048,
            2_049,
            4_095,
            4_096,
            12_345,
            99_999,
            1_000_000,
            30_000_000,
            4_000_000_000,
            MAX_VALUE_US,
        ] {
            let atomic = AtomicHistogram::new();

            atomic.record(value);

            let exported = atomic.export();

            let mut reference = Histogram::<u64>::new(3).unwrap();

            reference.record(value).unwrap();

            assert_eq!(
                exported.value_at_quantile(1.0),
                reference.value_at_quantile(1.0),
                "value {value}"
            );
            assert_eq!(exported.len(), 1);
        }
    }

    #[test]
    fn precision_is_one_part_in_a_thousand() {
        for value in [3_000u64, 47_123, 1_234_567, 3_000_000_000] {
            let atomic = AtomicHistogram::new();

            atomic.record(value);

            let got = atomic.export().max();
            let error = (got as f64 - value as f64).abs() / value as f64;

            assert!(error < 0.001, "value {value} came back as {got}");
        }
    }

    #[test]
    fn index_and_floor_are_consistent() {
        for index in (0..COUNTERS as u64).step_by(7) {
            assert_eq!(counter_index(counter_floor(index)), index);
        }

        assert_eq!(COUNTERS as u64 - 1, counter_index(MAX_VALUE_US));
    }

    #[test]
    fn values_above_the_range_are_clamped() {
        let atomic = AtomicHistogram::new();

        atomic.record(u64::MAX);

        assert_eq!(atomic.export().len(), 1);
    }

    #[test]
    fn concurrent_writers_lose_nothing() {
        let histogram = Arc::new(AtomicHistogram::new());

        let threads: Vec<_> = (0..8)
            .map(|id| {
                let histogram = histogram.clone();

                std::thread::spawn(move || {
                    for value in 0..50_000u64 {
                        histogram.record(value % 5_000 + id);
                    }
                })
            })
            .collect();

        for thread in threads {
            thread.join().unwrap();
        }

        assert_eq!(histogram.export().len(), 8 * 50_000);
    }
}
