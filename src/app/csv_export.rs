//! Raw per-request export (`--csv`).

use super::AppResult;
use crate::engine::worker::CsvBatch;
use std::io::Write;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// Handle of the writer thread; resolves once every row is on disk.
pub type CsvWriter = JoinHandle<std::io::Result<()>>;

const HEADER: &str = "relative_time_ms,status,latency_ms,error,service_ms\n";

/// Creates the CSV file, writes its header and starts the writer thread.
///
/// Workers send [`CsvBatch`]es through the returned channel; dropping every
/// sender ends the export. Must be called inside a Tokio runtime.
///
/// # Errors
///
/// Returns an error if the file cannot be created or written.
pub fn start(path: &str, buffer: usize) -> AppResult<(mpsc::Sender<CsvBatch>, CsvWriter)> {
    let (tx, mut rx) = mpsc::channel::<CsvBatch>(buffer);

    let mut file = std::io::BufWriter::with_capacity(64 * 1024, std::fs::File::create(path)?);
    file.write_all(HEADER.as_bytes())?;

    // The writer does blocking file I/O, so it gets its own thread instead
    // of stalling the async runtime.
    let handle = tokio::task::spawn_blocking(move || {
        let mut line = Vec::with_capacity(4096);

        while let Some(batch) = rx.blocking_recv() {
            line.clear();

            for record in &batch {
                record.write_row(&mut line);
            }

            file.write_all(&line)?;
        }

        file.flush()
    });

    Ok((tx, handle))
}
