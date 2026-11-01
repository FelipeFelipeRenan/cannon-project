//! Raw per-request export (`--csv`).

use super::AppResult;
use crate::engine::worker::CsvRecord;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// Handle of the writer thread; resolves once every row is on disk.
pub type CsvWriter = JoinHandle<Result<(), csv::Error>>;

/// Creates the CSV file, writes its header and starts the writer thread.
///
/// Workers send [`CsvRecord`]s through the returned channel; dropping every
/// sender ends the export. Must be called inside a Tokio runtime.
///
/// # Errors
///
/// Returns an error if the file cannot be created or written.
pub fn start(path: &str, buffer: usize) -> AppResult<(mpsc::Sender<CsvRecord>, CsvWriter)> {
    let (tx, mut rx) = mpsc::channel::<CsvRecord>(buffer);

    let mut writer = csv::Writer::from_path(path)?;

    writer.write_record([
        "relative_time_ms",
        "status",
        "latency_ms",
        "error",
        "service_ms",
    ])?;

    // The writer does blocking file I/O, so it gets its own thread instead
    // of stalling the async runtime.
    let handle = tokio::task::spawn_blocking(move || {
        while let Some(record) = rx.blocking_recv() {
            writer.write_record(record.to_row())?;
        }

        writer.flush()?;

        Ok::<(), csv::Error>(())
    });

    Ok((tx, handle))
}
