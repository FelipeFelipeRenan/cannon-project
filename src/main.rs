#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use cannon::args::parser::Args;
use cannon::engine::worker::{run_workers, CsvRecord, SharedMetrics};
use cannon::report::cli::{generate_html_report, print_banner, print_summary, FinalReport};
use cannon::report::gate::{self, Baseline, GateOptions};
use cannon::report::summary::RunSummary;
use clap::{CommandFactory, FromArgMatches};
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use std::process::ExitCode;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

type AppResult<T> = Result<T, Box<dyn std::error::Error>>;

/// Exit status used when a performance gate (regression or error rate) fails.
const EXIT_GATE_FAILED: u8 = 2;

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{} {error}", "error:".red().bold());
            ExitCode::FAILURE
        }
    }
}

fn run() -> AppResult<ExitCode> {
    let matches = Args::command().get_matches();
    let mut args = Args::from_arg_matches(&matches)?;

    if args.update {
        update()?;
        return Ok(ExitCode::SUCCESS);
    }

    cannon::args::config::merge_with_yaml(&mut args, &matches)?;
    args.validate()
        .map_err(|error| format!("invalid configuration: {error}"))?;

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

    let runtime = build_runtime(&args)?;

    runtime.block_on(run_app(args))
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

fn build_runtime(args: &Args) -> AppResult<tokio::runtime::Runtime> {
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

/// Waits for Ctrl-C (SIGINT) or, on Unix, SIGTERM.
///
/// Returns the conventional exit status for the signal received.
async fn shutdown_signal() -> u8 {
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

/// Fails early (before a long test) if an output file cannot be written.
fn ensure_writable(path: &str, what: &str) -> AppResult<()> {
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map(|_| ())
        .map_err(|error| format!("cannot write {what} '{path}': {error}").into())
}

fn load_baseline(path: &str) -> AppResult<Baseline> {
    let content = std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read baseline '{path}': {error}"))?;

    Baseline::parse(&content).map_err(|error| format!("baseline '{path}': {error}").into())
}

async fn run_app(args: Args) -> AppResult<ExitCode> {
    let is_tcp = args.mode.eq_ignore_ascii_case("tcp");

    let url_str = if is_tcp {
        args.url.clone().ok_or(
            "the target address (IP:Port) is required: use -u/--url or set `url` in the config file",
        )?
    } else {
        cannon::security::url_validator::validate_and_extract(&args.url)
            .map_err(|error| format!("invalid target URL: {error}"))?
    };

    let parsed_percentiles: Vec<f64> = args
        .percentiles
        .split(',')
        .filter_map(|s| s.trim().parse::<f64>().ok())
        .map(|p| p / 100.0)
        .collect();

    let ramp_up_duration = args
        .ramp_up
        .as_deref()
        .map(cannon::args::parser::parse_ramp_up_duration)
        .transpose()?;

    // Fail fast: everything that can be checked before the (possibly long)
    // test starts is checked here, not after it.
    let baseline = args
        .compare_baseline
        .as_deref()
        .map(load_baseline)
        .transpose()?;

    for (path, what) in [
        (&args.output, "JSON report"),
        (&args.html, "HTML report"),
        (&args.csv, "CSV file"),
        (&args.save_baseline, "baseline"),
    ] {
        if let Some(path) = path {
            ensure_writable(path, what)?;
        }
    }

    let http_client = cannon::client::http::build_optimized_client(&args)?;

    let buffer_size = std::cmp::min(args.workers as usize, 10_000).max(1);

    print_banner();

    println!("🎯 Target: {}", url_str.bright_cyan().bold());
    println!(
        "🚀 {}",
        format!(
            "Preparing the Cannon for {} shots with {} workers...",
            args.count.to_string().cyan(),
            args.workers.to_string().magenta()
        )
        .bold()
    );

    println!("⏱️ Timeout: {}ms", args.timeout.to_string().yellow());

    match args.rps {
        Some(rps) => println!(
            "📐 Load: open-loop at {} req/s (latency measured from the scheduled send time)",
            rps.to_string().yellow()
        ),
        None => println!("📐 Load: closed-loop, as fast as the workers can go"),
    }

    let warmup_duration = Duration::from_secs(args.warmup);

    if args.warmup > 0 {
        println!(
            "🔥 Warm-up Mode active: Disregarding the first {}s of metrics...",
            args.warmup.to_string().yellow()
        );
    }

    let template_arc = args
        .body
        .clone()
        .map(|body| cannon::payload::generator::PayloadTemplate::parse(&body))
        .transpose()?;

    if let Some(template) = &template_arc {
        for warning in template.warnings() {
            eprintln!("{} {warning}", "⚠️ ".yellow());
        }
    }

    let expect_arc = args.expect.clone().map(Arc::new);

    let target: Arc<cannon::client::target::Target> = if is_tcp {
        let clean_addr = url_str.replace("http://", "").replace("https://", "");
        let tcp_read = match &args.tcp_read_until {
            Some(delimiter) => cannon::client::target::TcpRead::Until(
                cannon::args::parser::unescape_delimiter(delimiter),
            ),
            None => cannon::client::target::TcpRead::Bytes(args.tcp_read_bytes.unwrap_or(1)),
        };

        let tcp_target = cannon::client::target::Target::new_tcp_with_options(
            &clean_addr,
            args.workers,
            Duration::from_millis(args.connect_timeout),
            Duration::from_millis(args.timeout),
            tcp_read,
        )
        .await?;

        Arc::new(tcp_target)
    } else {
        let accepted_status = args
            .expect_status
            .as_deref()
            .map(cannon::client::target::StatusFilter::parse)
            .transpose()?
            .map(Arc::new);

        let http_target = cannon::client::target::Target::new_http_with_status(
            http_client.clone(),
            url_str.clone(),
            reqwest::Method::from_bytes(args.method.as_bytes())
                .map_err(|error| format!("invalid HTTP method '{}': {error}", args.method))?,
            Arc::new(args.headers.clone()),
            expect_arc,
            accepted_status,
        );
        Arc::new(http_target)
    };

    let pb = ProgressBar::new(args.count as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.bold.green} [{elapsed_precise}] {bar:40.magenta/blue} {pos:>7}/{len:7} {msg}")
            .map_err(|error| format!("invalid progress bar template: {error}"))?
            .progress_chars("━╾─"),
    );

    let shared_metrics = Arc::new(SharedMetrics::default());

    let mut csv_tx = None;
    let mut csv_handle = None;

    if let Some(path) = &args.csv {
        let (tx, mut rx) = mpsc::channel::<CsvRecord>(buffer_size);
        csv_tx = Some(tx);

        let mut writer = csv::Writer::from_path(path)?;

        writer.write_record([
            "relative_time_ms",
            "status",
            "latency_ms",
            "error",
            "service_ms",
        ])?;

        // The writer does blocking file I/O, so it gets its own thread
        // instead of stalling the async runtime.
        csv_handle = Some(tokio::task::spawn_blocking(move || {
            while let Some(rec) = rx.blocking_recv() {
                writer.write_record([
                    rec.relative_ms,
                    rec.status,
                    rec.latency_ms,
                    rec.error,
                    rec.service_ms,
                ])?;
            }

            writer.flush()?;

            Ok::<(), csv::Error>(())
        }));
    }

    let mut engine_handle = tokio::spawn(run_workers(
        args.count,
        args.workers,
        template_arc,
        args.rps,
        target,
        shared_metrics.clone(),
        csv_tx,
        Instant::now(),
        warmup_duration,
        ramp_up_duration,
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

    let (worker_results, measurement_duration) = engine_result??;

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

    let summary = RunSummary::aggregate(
        worker_results,
        &shared_metrics,
        measurement_duration,
        args.rps.is_some(),
    )?;

    print_summary(
        &summary,
        args.count,
        args.rps,
        &parsed_percentiles,
        args.apdex_t,
    );

    // Reports are always written, even when the gate fails, so that CI can
    // attach them to the failed job.
    let report = FinalReport::new(
        &summary,
        &url_str,
        args.workers,
        args.rps,
        args.apdex_t,
        &parsed_percentiles,
    );

    if args.output.is_some() || args.html.is_some() {
        let json_data = serde_json::to_string_pretty(&report)?;

        if let Some(path) = &args.output {
            std::fs::write(path, &json_data)
                .map_err(|error| format!("cannot write JSON report '{path}': {error}"))?;
            println!(
                "📂 JSON report saved successfully to {}!",
                path.bright_cyan()
            );
        }

        if let Some(path) = &args.html {
            generate_html_report(path, &json_data)
                .map_err(|error| format!("cannot write HTML report '{path}': {error}"))?;
            println!(
                "🌐 HTML report saved successfully to {}!",
                path.bright_cyan()
            );
        }
    }

    let mut gate_failures = Vec::new();

    if let Some(path) = &args.save_baseline {
        match Baseline::from_summary(&summary) {
            Ok(baseline) => {
                std::fs::write(path, serde_json::to_string_pretty(&baseline)?)
                    .map_err(|error| format!("cannot write baseline '{path}': {error}"))?;

                println!(
                    "\n💾 Performance baseline saved successfully to: {}",
                    path.bright_green()
                );
            }
            Err(reason) => gate_failures.push(format!("❌ Baseline not saved: {reason}")),
        }
    }

    if let Some(status) = interrupted_status {
        println!("\nGate checks are skipped because the run was interrupted.");

        return Ok(ExitCode::from(status));
    }

    let outcome = gate::evaluate(
        &summary,
        baseline.as_ref(),
        &GateOptions {
            tolerance_percent: args.tolerance,
            max_error_rate_percent: args.max_error_rate,
        },
    );

    for note in &outcome.notes {
        println!("\n{note}");
    }

    gate_failures.extend(outcome.failures);

    if gate_failures.is_empty() {
        return Ok(ExitCode::SUCCESS);
    }

    for failure in &gate_failures {
        println!("\n{}", failure.red().bold());
    }

    eprintln!("{} performance gate failed", "error:".red().bold());

    Ok(ExitCode::from(EXIT_GATE_FAILED))
}

fn update() -> AppResult<()> {
    let target = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "linux-x64",
        ("linux", "aarch64") => "linux-arm64",
        ("macos", "x86_64") => "macos-x64",
        ("macos", "aarch64") => "macos-arm64",
        ("windows", "x86_64") => "windows-x64.exe",
        (os, arch) => {
            return Err(format!("no pre-built Cannon release for {os}/{arch}").into());
        }
    };

    let status = self_update::backends::github::Update::configure()
        .repo_owner("FelipeFelipeRenan")
        .repo_name("cannon-project")
        .bin_name("cannon")
        .target(target)
        .show_download_progress(true)
        .current_version(env!("CARGO_PKG_VERSION"))
        .build()?
        .update()?;

    if status.is_updated() {
        println!("✅ Successfully updated to version {}", status.version());
    } else {
        println!(
            "✨ You are already on the latest version: {}",
            status.version()
        );
    }

    Ok(())
}
