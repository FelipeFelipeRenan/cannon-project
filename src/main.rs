// src/main.rs

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use cannon::args::parser::Args;
use cannon::report::cli::{generate_html_report, print_banner, print_summary, to_ms, FinalReport};
use clap::Parser;
use colored::Colorize;
use hdrhistogram::Histogram;
use indicatif::{ProgressBar, ProgressStyle};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    if args.pin_threads {
        let core_ids = core_affinity::get_core_ids().expect("❌ Error reading CPU topology");
        let core_count = core_ids.len();
        let core_idx = Arc::new(AtomicUsize::new(0));

        println!(
            "CPU Pinning active: Applying Strict CPU Affinity ({} cores)...",
            core_count
        );

        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(core_count)
            .on_thread_start(move || {
                let idx = core_idx.fetch_add(1, Ordering::SeqCst) % core_count;
                let target_core = core_ids[idx];
                let _ = core_affinity::set_for_current(target_core);
            })
            .build()?;

        rt.block_on(async { run_app(args).await })
    } else {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async { run_app(args).await })
    }
}

async fn run_app(_args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let mut args = Args::parse();

    if args.update {
        update()?;
        return Ok(());
    }

    if let Err(e) = cannon::args::config::merge_with_yaml(&mut args) {
        eprintln!(
            "{} Failed to load YAML configuration: {}",
            "❌ Error:".red().bold(),
            e
        );
        std::process::exit(1);
    }

    let url_str = if args.mode.to_lowercase() == "tcp" {
        args.url
            .clone()
            .expect("❌ Error: The address (IP:Port) from the target is required!")
    } else {
        cannon::security::url_validator::validate_and_extract(&args.url)
    };

    let parsed_percentiles: Vec<f64> = args
        .percentiles
        .split(',')
        .filter_map(|s| s.trim().parse::<f64>().ok())
        .map(|p| p / 100.0)
        .collect();

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

    let start_test = Instant::now();

    let warmup_duration = std::time::Duration::from_secs(args.warmup);

    if args.warmup > 0 {
        println!(
            "🔥 Warm-up Mode active: Disregarding the first {}s of metrics...",
            args.warmup.to_string().yellow()
        );
    }

    let template_arc = args
        .body
        .clone()
        .map(|b| cannon::payload::generator::PayloadTemplate::parse(&b));
    let expect_arc = args.expect.clone().map(Arc::new);

    let target: Arc<cannon::client::target::Target> = if args.mode.to_lowercase() == "tcp" {
        let clean_addr = url_str.replace("http://", "").replace("https://", "");
        let tcp_target = cannon::client::target::Target::new_tcp(&clean_addr, args.workers)
            .await
            .expect("❌ Failed to create TCP target");
        Arc::new(tcp_target)
    } else {
        let http_target = cannon::client::target::Target::new_http(
            http_client.clone(),
            url_str.clone(),
            reqwest::Method::from_bytes(args.method.as_bytes()).unwrap_or(reqwest::Method::GET),
            Arc::new(args.headers.clone()),
            expect_arc,
        );
        Arc::new(http_target)
    };

    let pb = ProgressBar::new(args.count as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.bold.green} [{elapsed_precise}] {bar:40.magenta/blue} {pos:>7}/{len:7} {msg}")
            .unwrap()
            .progress_chars("━╾─"),
    );
    println!(
        "{}",
        "Press Ctrl+C to interrupt and view partial report".bright_black()
    );

    // Instancia os Atomics
    use std::sync::atomic::Ordering;
    let shared_metrics = Arc::new(cannon::engine::worker::SharedMetrics::default());

    // Configuração do CSV Assíncrono
    let mut csv_tx = None;
    if let Some(path) = &args.csv {
        let (tx, mut rx) = mpsc::channel::<cannon::engine::worker::CsvRecord>(buffer_size);
        csv_tx = Some(tx);
        let path_clone = path.clone();

        // Spawn Background Worker pro I/O de disco
        tokio::spawn(async move {
            if let Ok(mut w) = csv::Writer::from_path(&path_clone) {
                let _ = w.write_record(["relative_time_ms", "status", "latency_ms", "error"]);
                while let Some(rec) = rx.recv().await {
                    let _ =
                        w.write_record(&[rec.relative_ms, rec.status, rec.latency_ms, rec.error]);
                }
                let _ = w.flush();
            }
        });
    }

    let engine_handle = tokio::spawn(cannon::engine::worker::run_workers(
        args.count,
        args.workers,
        template_arc,
        args.rps,
        target,
        shared_metrics.clone(),
        csv_tx,
        start_test,
        warmup_duration,
    ));

    let mut last_total = 0;
    let mut last_time = Instant::now();

    while !engine_handle.is_finished() {
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_millis(500)) => {
                let succ = shared_metrics.successes.load(Ordering::Relaxed);
                let fail = shared_metrics.failures.load(Ordering::Relaxed);
                let total = succ + fail;

                pb.set_position(total);

                let elapsed = last_time.elapsed().as_secs_f64();
                if elapsed >= 0.1 && total > last_total {
                    let rps = (total - last_total) as f64 / elapsed;
                    pb.set_message(format!("| ⚡ {:.1} RPS", rps));
                    last_total = total;
                    last_time = Instant::now();
                }
            }
            _ = tokio::signal::ctrl_c() => {
                println!("\n\n{}", "⚠️ Interruption detected! Waiting workers...".yellow().bold());
                break;
            }
        }
    }

    let (worker_results, measurement_duration) = engine_handle.await.unwrap_or_default();
    pb.finish_with_message("Finished");

    if let Some(path) = &args.csv {
        println!("📊 Raw data exported to {}!", path.bright_cyan());
    }

    // Merging local reports (The final merge)
    let mut hist = Histogram::<u64>::new_with_bounds(1, 60_000_000, 3)?;
    let mut status_counts = std::collections::HashMap::new();
    let mut error_counts = std::collections::HashMap::new();
    let mut assertion_failures = 0;

    for w in worker_results {
        let _ = hist.add(w.histogram);
        for (k, v) in w.status_counts {
            *status_counts.entry(k).or_insert(0) += v;
        }
        for (k, v) in w.error_counts {
            *error_counts.entry(k).or_insert(0) += v;
        }
        assertion_failures += w.assertion_failures;
    }

    let success_count = shared_metrics.successes.load(Ordering::Relaxed);
    let failure_count = shared_metrics.failures.load(Ordering::Relaxed);
    let total_bytes_sent = shared_metrics.bytes_sent.load(Ordering::Relaxed);
    let total_bytes_received = shared_metrics.bytes_received.load(Ordering::Relaxed);
    let measured_requests = shared_metrics.measured_requests.load(Ordering::Relaxed);
    let total_secs = measurement_duration.as_secs_f64();
    let actual_rps = if total_secs > 0.0 {
        measured_requests as f64 / total_secs
    } else {
        0.0
    };

    let t_us = args.apdex_t * 1000;
    let satisfied = hist.count_between(0, t_us);
    let tolerating = hist.count_between(t_us + 1, t_us * 4);
    let apdex = if !hist.is_empty() {
        (satisfied as f64 + (tolerating as f64 / 2.0)) / hist.len() as f64
    } else {
        0.0
    };

    print_summary(
        success_count,
        failure_count,
        &hist,
        start_test.elapsed(),
        args.rps,
        actual_rps,
        status_counts.clone(),
        error_counts.clone(),
        assertion_failures,
        total_bytes_sent,
        total_bytes_received,
        &parsed_percentiles,
    );

    let current_p99_ms = hist.value_at_percentile(99.0) as f64 / 1000.0;

    if let Some(path) = &args.save_baseline {
        let baseline_data = serde_json::json!({
            "p99_ms": current_p99_ms,
            "rps": actual_rps
        });

        if let Ok(json_str) = serde_json::to_string_pretty(&baseline_data) {
            let _ = std::fs::write(path, json_str);
            println!(
                "\n💾 Performance baseline saved successfully to: {}",
                path.bright_green()
            );
        }
    }

    if let Some(path) = &args.compare_baseline {
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Ok(baseline) = serde_json::from_str::<serde_json::Value>(&content) {
                let base_p99 = baseline["p99_ms"].as_f64().unwrap_or(0.0);

                println!("\n⚖️  {}", "BASELINE ANALISYS (CI/CD)".bright_blue().bold());
                println!("   Historic P99: {:.2}ms", base_p99);
                println!("   Current P99:     {:.2}ms", current_p99_ms);

                if current_p99_ms > base_p99 {
                    let degradation = ((current_p99_ms - base_p99) / base_p99) * 100.0;
                    println!(
                        "   Variation:      +{} worst",
                        format!("{:.2}%", degradation).yellow()
                    );

                    if degradation > args.tolerance {
                        println!(
                            "\n❌ {} Tolerance of {}% exceeded. Aborting with error...",
                            "PERFORMANCE REGRESSION DETECTED!".red().bold(),
                            args.tolerance
                        );
                        std::process::exit(1);
                    } else {
                        println!(
                            "\n✅ Accepted regression. Whintin tolerance of {}%.",
                            args.tolerance
                        );
                    }
                } else {
                    let improvement = ((base_p99 - current_p99_ms) / base_p99) * 100.0;
                    println!(
                        "   Variation:      -{} better",
                        format!("{:.2}%", improvement).green()
                    );
                    println!("\n✅ Performance improved or remained constant!");
                }
            }
        } else {
            println!(
                "\n⚠️ Warning: Baseline file '{}' not found. Comparison skipped.",
                path.yellow()
            );
        }
    }

    let status_for_report = status_counts.clone();
    let errors_for_report = error_counts.clone();

    if args.output.is_some() || args.html.is_some() {
        let report = FinalReport {
            target: url_str.clone(),
            total_requests: args.count,
            concurrency: args.workers,
            successes: success_count,
            failures: failure_count,
            min_ms: to_ms(hist.min()),
            avg_ms: to_ms(hist.mean() as u64),
            p50_ms: to_ms(hist.value_at_quantile(0.5)),
            p95_ms: to_ms(hist.value_at_quantile(0.95)),
            p99_ms: to_ms(hist.value_at_quantile(0.99)),
            max_ms: to_ms(hist.max()),
            actual_rps,
            bytes_sent: total_bytes_sent,
            bytes_received: total_bytes_received,
            status_codes: status_for_report,
            errors: errors_for_report,
            duration_secs: total_secs,
            apdex_score: apdex,
        };

        let json_data = serde_json::to_string_pretty(&report)?;

        if let Some(path) = &args.output {
            std::fs::write(path, &json_data)?;
            println!(
                "📂 JSON report saved successfully to {}!",
                path.bright_cyan()
            );
        }

        if let Some(path) = &args.html {
            generate_html_report(path, &json_data)?;
            println!(
                "🌐 HTML report saved successfully to {}!",
                path.bright_cyan()
            );
        }
    }

    Ok(())
}

fn update() -> Result<(), Box<dyn std::error::Error>> {
    let target = if cfg!(target_os = "linux") {
        "linux-x64"
    } else if cfg!(target_os = "windows") {
        "windows-x64.exe"
    } else if cfg!(target_os = "macos") {
        "macos-x64"
    } else {
        ""
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

    if status.updated() {
        println!("✅ Successfully updated to version {}", status.version());
    } else {
        println!(
            "✨ You are already on the latest version: {}",
            status.version()
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use hdrhistogram::Histogram;

    #[test]
    fn test_histogram_percentile_math() {
        // Initialize the histogram exactly as we do in the engine
        let mut hist = Histogram::<u64>::new(3).expect("Failed to create histogram");

        // Simulate 100 requests with latencies from 1ms to 100ms
        for i in 1..=100 {
            hist.record(i).unwrap();
        }

        // Validate if the percentile math (that Cannon exports) is accurate
        assert_eq!(
            hist.value_at_quantile(0.50),
            50,
            "The median (p50) should be 50"
        );
        assert_eq!(hist.value_at_quantile(0.95), 95, "The p95 should be 95");
        assert_eq!(hist.value_at_quantile(0.99), 99, "The p99 should be 99");
        assert_eq!(hist.max(), 100, "The maximum latency should be 100");
        assert_eq!(hist.min(), 1, "The minimum latency should be 1");
    }

    #[test]
    fn test_apdex_calculation_logic() {
        let mut hist = Histogram::<u64>::new(3).unwrap();

        // Simulate requests:
        // 60 satisfied requests (<= 50ms)
        // 30 tolerating requests (<= 200ms)
        // 10 frustrated requests (> 200ms)
        for _ in 0..60 {
            hist.record(40).unwrap();
        }
        for _ in 0..30 {
            hist.record(150).unwrap();
        }
        for _ in 0..10 {
            hist.record(300).unwrap();
        }

        let apdex_t = 50;
        let satisfied = hist.count_between(0, apdex_t);
        let tolerating = hist.count_between(apdex_t + 1, apdex_t * 4);

        // Apdex Formula: (Satisfied + (Tolerating / 2)) / Total
        let apdex_score = (satisfied as f64 + (tolerating as f64 / 2.0)) / 100.0;

        assert_eq!(satisfied, 60);
        assert_eq!(tolerating, 30);
        assert_eq!(
            apdex_score, 0.75,
            "The calculated Apdex Score should be 0.75"
        );
    }
}
