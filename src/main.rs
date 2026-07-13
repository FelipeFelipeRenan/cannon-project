// src/main.rs

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use cannon::args::parser::Args;
use cannon::report::cli::{generate_html_report, print_banner, print_summary, to_ms, FinalReport};
use clap::Parser;
use colored::Colorize;
use hdrhistogram::Histogram;
use indicatif::{ProgressBar, ProgressStyle};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = Args::parse();

    if args.update {
        update()?;
        return Ok(());
    }

    // Se o utilizador passou um ficheiro YAML, fazemos o merge com tratamento de erro
    if let Err(e) = cannon::args::config::merge_with_yaml(&mut args) {
        eprintln!(
            "{} Falha ao carregar configuração YAML: {}",
            "❌ Erro:".red().bold(),
            e
        );
        std::process::exit(1);
    }

    // Validação de segurança condicional: ignora a exigência de HTTP se for modo TCP
    let url_str = if args.mode.to_lowercase() == "tcp" {
        args.url
            .clone()
            .expect("❌ Erro: The address (IP:Port) from the target is required!")
    } else {
        cannon::security::url_validator::validate_and_extract(&args.url)
    };

    // Transforma os percentis da CLI para a escala matemática do HdrHistogram
    let parsed_percentiles: Vec<f64> = args
        .percentiles
        .split(',')
        .filter_map(|s| s.trim().parse::<f64>().ok())
        .map(|p| p / 100.0)
        .collect();

    // Build HTTP client uma única vez (reutilizado para recriar Arcs)
    let http_client = cannon::client::http::build_optimized_client(&args)?;

    let buffer_size = std::cmp::min(args.workers as usize, 10_000).max(1);

    print_banner();

    // CORREÇÃO 1: Usar url_str aqui
    println!("🎯 Alvo: {}", url_str.bright_cyan().bold());
    println!(
        "🚀 {}",
        format!(
            "Preparando o canhão para {} disparo(s) com {} workers...",
            args.count.to_string().cyan(),
            args.workers.to_string().magenta()
        )
        .bold()
    );

    println!("⏱️ Timeout: {}ms", args.timeout.to_string().yellow());

    let start_test = Instant::now();

    let warmup_duration = std::time::Duration::from_secs(args.warmup);
    let warmup_end = start_test + warmup_duration;

    if args.warmup > 0 {
        println!(
            "🔥 Modo Warm-up ativado: Desprezando os primeiros {}s de métricas...",
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
        "Pressione Ctrl+C para interromper e ver o relatório parcial".bright_black()
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
                let _ = w.write_record(["tempo_relativo_ms", "status", "latencia_ms", "erro"]);
                while let Some(rec) = rx.recv().await {
                    let _ =
                        w.write_record(&[rec.relative_ms, rec.status, rec.latency_ms, rec.error]);
                }
                let _ = w.flush();
            }
        });
    }

    // Inicia o motor
    let engine_handle = tokio::spawn(cannon::engine::worker::run_workers(
        args.count,
        args.workers,
        template_arc,
        args.rps,
        target,
        shared_metrics.clone(),
        csv_tx,
        start_test,
        warmup_end,
    ));

    // UI Loop (Puxa os dados dos Atomics a cada 500ms)
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
                println!("\n\n{}", "⚠️ Interrupção detectada! Aguardando workers...".yellow().bold());
                break;
            }
        }
    }

    let worker_results = engine_handle.await.unwrap_or_default();
    pb.finish_with_message("Concluído");

    if let Some(path) = &args.csv {
        println!("📊 Dados brutos exportados para {}!", path.bright_cyan());
    }

    // Fusão dos relatórios locais (O Merge final)
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
    let actual_duration = start_test.elapsed();
    let stable_duration = actual_duration
        .checked_sub(warmup_duration)
        .unwrap_or(actual_duration);
    let total_secs = stable_duration.as_secs_f64();
    let actual_rps = success_count as f64 / total_secs;

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
        status_counts.clone(),
        error_counts.clone(),
        assertion_failures,
        total_bytes_sent,
        total_bytes_received,
        &parsed_percentiles,
    );

    let status_for_report = status_counts.clone();
    let errors_for_report = error_counts.clone();

    // Exportação de Dados (JSON / HTML)
    if args.output.is_some() || args.html.is_some() {
        let report = FinalReport {
            target: url_str.clone(), // CORREÇÃO 3: Usar o url_str aqui pro JSON/HTML
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
                "📂 Relatório JSON salvo com sucesso em {}!",
                path.bright_cyan()
            );
        }

        if let Some(path) = &args.html {
            generate_html_report(path, &json_data)?;
            println!(
                "🌐 Relatório HTML salvo com sucesso em {}!",
                path.bright_cyan()
            );
        }
    }

    Ok(())
}

fn update() -> Result<(), Box<dyn std::error::Error>> {
    // Definimos o identificador de destino que corresponde ao nome do asset no GitHub
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
        println!(
            "✅ Atualizado com sucesso para a versão {}",
            status.version()
        );
    } else {
        println!(
            "✨ Você já está na versão mais recente: {}",
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
