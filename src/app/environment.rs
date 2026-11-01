//! Process-level setup done before the load test starts: terminal colours,
//! proxy and file-descriptor diagnostics, and the Tokio runtime.

use super::AppResult;
use crate::args::parser::Args;
use colored::Colorize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Applies process-wide settings and prints environment warnings.
pub fn prepare(args: &Args) {
    if args.no_color {
        colored::control::set_override(false);
    }

    if !args.no_proxy && !args.mode.eq_ignore_ascii_case("tcp") {
        let proxy = ["HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY"]
            .iter()
            .find_map(|name| {
                std::env::var(name)
                    .ok()
                    .or_else(|| std::env::var(name.to_lowercase()).ok())
                    .filter(|value| !value.is_empty())
                    .map(|_| *name)
            });

        if let Some(name) = proxy {
            eprintln!(
                "{} {name} is set: requests may go through a proxy, which then becomes part of \
                 what is measured (use --no-proxy to connect directly)",
                "⚠️ ".yellow()
            );
        }
    }

    if let Some(limit) = raise_fd_limit() {
        // Each worker may hold a socket, plus a few descriptors for files.
        if u64::from(args.workers) + 64 > limit {
            eprintln!(
                "{} {} workers may exceed the open-file limit ({limit}); \
                 expect 'File Descriptor Limit' errors or raise `ulimit -n`",
                "⚠️ ".yellow(),
                args.workers
            );
        }
    }
}

/// Raises the soft open-file limit to the hard limit and returns the
/// resulting soft limit (`None` where it cannot be determined).
#[cfg(unix)]
fn raise_fd_limit() -> Option<u64> {
    let mut limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };

    // SAFETY: `limit` is a valid, writable rlimit for the duration of the calls.
    unsafe {
        if libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) != 0 {
            return None;
        }

        if limit.rlim_cur < limit.rlim_max {
            let mut raised = limit;

            // macOS rejects values above OPEN_MAX even when the hard limit is
            // "infinity", so fall back to the previous value on failure.
            #[cfg(target_os = "macos")]
            {
                raised.rlim_cur = raised.rlim_max.min(10_240);
            }
            #[cfg(not(target_os = "macos"))]
            {
                raised.rlim_cur = raised.rlim_max;
            }

            if libc::setrlimit(libc::RLIMIT_NOFILE, &raised) == 0 {
                limit = raised;
            }
        }
    }

    #[allow(clippy::unnecessary_cast)] // `rlim_t` width varies per platform
    Some(limit.rlim_cur as u64)
}

#[cfg(not(unix))]
fn raise_fd_limit() -> Option<u64> {
    None
}

/// Builds the Tokio runtime, optionally pinning its threads to CPU cores.
pub fn build_runtime(args: &Args) -> AppResult<tokio::runtime::Runtime> {
    if !args.pin_threads {
        return Ok(tokio::runtime::Runtime::new()?);
    }

    let core_ids = core_affinity::get_core_ids()
        .filter(|ids| !ids.is_empty())
        .ok_or("could not read the CPU topology required by --pin-threads")?;
    let core_count = core_ids.len();
    let core_idx = Arc::new(AtomicUsize::new(0));

    println!(
        "CPU Pinning active: Applying Strict CPU Affinity ({} cores)...",
        core_count
    );

    Ok(tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(core_count)
        .on_thread_start(move || {
            let idx = core_idx.fetch_add(1, Ordering::SeqCst) % core_count;
            let target_core = core_ids[idx];
            let _ = core_affinity::set_for_current(target_core);
        })
        .build()?)
}
