//! Termination signals.

/// Waits for Ctrl-C (SIGINT) or, on Unix, SIGTERM.
///
/// Returns the conventional exit status for the signal received.
pub async fn shutdown_signal() -> u8 {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        match signal(SignalKind::terminate()) {
            Ok(mut terminate) => {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => 130,
                    _ = terminate.recv() => 143,
                }
            }
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
                130
            }
        }
    }

    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
        130
    }
}
