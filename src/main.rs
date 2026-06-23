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
    } // Extrai a URL ou mata o processo de forma graciosa
    let url_str = cannon::security::url_validator::validate_and_extract(&args.url);
    // Transforma os percentis da CLI para a escala matemática do HdrHistogram
    let parsed_percentiles: Vec<f64> = args
        .percentiles
        .split(',')
        .filter_map(|s| s.trim().parse::<f64>().ok())
        .map(|p| p / 100.0)
        .collect();

    let client = Arc::new(cannon::client::http::build_optimized_client(&args)?);

    let url = Arc::new(url_str);
    let headers = Arc::new(args.headers.clone());

    let buffer_size = std::cmp::min(args.workers as usize, 10_000).max(1);

    // change to a fix size like 10_000
    let (tx, mut rx) = mpsc::channel(buffer_size);

    print_banner();

    println!("🎯 Alvo: {}", url.bright_cyan().bold());
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

    let body_arc = args.body.clone().map(Arc::new);
    let expect_arc = args.expect.clone().map(Arc::new);

    // Inicia o motor em background
    tokio::spawn(cannon::engine::worker::run_workers(
        args.count,
        args.workers,
        url.clone(),
        reqwest::Method::from_bytes(args.method.as_bytes()).unwrap(),
        body_arc,
        headers,
        client,
        expect_arc,
        tx,
        args.rps,
    ));

    // Configura UI e Métricas
    let mut success_count = 0;
    let mut failure_count = 0;
    let mut hist = Histogram::<u64>::new_with_bounds(1, 60_000_000, 3)?;

    let mut status_counts = std::collections::HashMap::<u16, u64>::new();
    let mut error_counts = std::collections::HashMap::<String, u64>::new();

    let mut assertion_failures = 0;

    let mut total_bytes_sent: u64 = 0;
    let mut total_bytes_received: u64 = 0;

    let pb = ProgressBar::new(args.count as u64);
    pb.set_style(
    ProgressStyle::default_bar()
        .template("{spinner:.bold.green} [{elapsed_precise}] {bar:40.magenta/blue} {pos:>7}/{len:7} {msg}")
        .unwrap()
        .progress_chars("━╾─"), // Gradiente de blocos Unicode
);

    println!(
        "{}",
        "Pressione Ctrl+C para interromper e ver o relatório parcial".bright_black()
    );

    // Prepara o escritor CSV se o argumento for passado
    let mut csv_writer = match &args.csv {
        Some(path) => {
            let mut w = csv::Writer::from_path(path).expect("❌ Erro ao criar arquivo CSV");
            w.write_record(["tempo_relativo_ms", "status", "latencia_ms", "erro"])
                .unwrap();
            Some(w)
        }
        None => None,
    };

    loop {
        tokio::select! {
                result = rx.recv() => {
                    match result {
                        Some(res) => {
                            if let Some(w) = &mut csv_writer {
                            let ts = start_test.elapsed().as_millis().to_string();
                            let status = res.status_code.map(|c: u16| c.to_string()).unwrap_or_else(|| "N/A".to_string());
                            let lat = res.duration.as_millis().to_string();
                            let err = res.error.clone().unwrap_or_default();
                            let _ = w.write_record(&[ts, status, lat, err]);
                        }
                            pb.inc(1);

                            total_bytes_sent += res.bytes_sent;
                            total_bytes_received += res.bytes_received;

                            let elapsed = start_test.elapsed().as_secs_f64();
                            if elapsed > 0.1 {
                                let total_reqs = success_count + failure_count;
                                if total_reqs % 25 == 0 && elapsed > 0.1 {
                                    let rps = total_reqs as f64 / elapsed;
                                     pb.set_message(format!("| ⚡ {:.1} RPS", rps));
        }

                            }


                            if let Some(code) = res.status_code {
                                *status_counts.entry(code).or_insert(0) += 1;
                            }

                            if res.success {
                                success_count += 1;
                                let _ = hist.record(res.duration.as_micros() as u64);
                            } else {
                                failure_count += 1;
                            }
                            if let Some(err_msg) = res.error{
                                *error_counts.entry(err_msg).or_insert(0) += 1;
                            }
                            if !res.assertion_success {
                                assertion_failures += 1;
                            }
                        },
                        None => break,
                    }
                }
                _ = tokio::signal::ctrl_c() => {
                    println!("\n\n{}", "⚠️ Interrupção detectada! Preparando relatório parcial...".yellow().bold());
                    break;
                }
            }
    }

    pb.finish_with_message("Concluído");

    if let Some(mut w) = csv_writer {
        let _ = w.flush();
        println!(
            "📊 Dados brutos exportados para {}!",
            args.csv.as_ref().unwrap().bright_cyan()
        );
    }

    let status_for_report = status_counts.clone();
    let errors_for_report = error_counts.clone();
    let total_secs = start_test.elapsed().as_secs_f64();
    let actual_rps = success_count as f64 / total_secs;

    // Satisfatórias (<= 50ms) e Toleráveis (51ms a 200ms)
    let t_us = args.apdex_t * 1000; // 50ms em microssegundos
    let satisfied = hist.count_between(0, t_us);
    let tolerating = hist.count_between(t_us + 1, t_us * 4);
    let apdex = if !hist.is_empty() {
        (satisfied as f64 + (tolerating as f64 / 2.0)) / hist.len() as f64
    } else {
        0.0
    };

    // Impressão do Relatório
    print_summary(
        success_count,
        failure_count,
        &hist,
        start_test.elapsed(),
        args.rps,
        status_counts,
        error_counts,
        assertion_failures,
        total_bytes_sent,
        total_bytes_received,
        &parsed_percentiles,
    );

    // Exportação de Dados (JSON / HTML)
    if args.output.is_some() || args.html.is_some() {
        let report = FinalReport {
            target: url.to_string(),
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
        .repo_owner("FelipeFelipeRenan") //
        .repo_name("cannon-project") //
        .bin_name("cannon") // Nome do binário local
        .target(target) // Força a busca pelo asset que termina com este target
        .show_download_progress(true) //
        .current_version(env!("CARGO_PKG_VERSION")) //
        .build()?
        .update()?;

    if status.updated() {
        println!(
            "✅ Atualizado com sucesso para a versão {}",
            status.version()
        ); //
    } else {
        println!(
            "✨ Você já está na versão mais recente: {}",
            status.version()
        ); //
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
