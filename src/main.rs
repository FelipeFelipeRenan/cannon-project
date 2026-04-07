mod args;
mod engine;
mod payload;
mod report;

use crate::args::Args;
use crate::report::LatencyMetrics;
use crate::report::{to_ms, FinalReport};
use clap::Parser;
use colored::Colorize;
use hdrhistogram::Histogram;
use indicatif::{ProgressBar, ProgressStyle};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    if args.update {
        update()?;
        return Ok(());
    }

    let url_str = match args.url.as_ref() {
        Some(u) => u.clone(),
        None => {
            eprintln!(
                "{} {}",
                "error:".red().bold(),
                "O argumento '--url <URL>' é obrigatório para iniciar o teste."
            );
            std::process::exit(1);
        }
    };

    let client = Arc::new(
        reqwest::Client::builder()
            .user_agent(&args.user_agent)
            .timeout(std::time::Duration::from_millis(args.timeout))
            .build()?,
    );
    let url = Arc::new(url_str);
    let headers = Arc::new(args.headers.clone());

    let (tx, mut rx) = mpsc::channel(args.count as usize);

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

    let ramp_up_secs = args
        .ramp_up
        .as_ref()
        .map(|s| parse_duration(s))
        .unwrap_or(0);
    // Inicia o motor em background
    tokio::spawn(engine::run_producer(
        args.count,
        args.workers,
        Arc::clone(&url),
        client,
        tx,
        args.rps,
        args.body.clone(),
        args.method.clone(),
        headers,
        args.expect.clone(),
        ramp_up_secs,
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

    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Some(res) => {
                        pb.inc(1);

                        total_bytes_sent += res.bytes_sent;
                        total_bytes_received += res.bytes_received;
                        let elapsed = start_test.elapsed().as_secs_f64();
                        if elapsed > 0.1 {
                            let total_reqs = success_count + failure_count;
                            let rps = total_reqs as f64 / elapsed;
                            pb.set_message(format!("| ⚡ {:.1} RPS", rps));
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
    );

    // Exportação JSON
    if let Some(path) = &args.output {
        save_report(&path, &args, &url, success_count, failure_count, &hist)?;
    }

    Ok(())
}

fn print_summary(
    successes: u64,
    failures: u64,
    hist: &Histogram<u64>,
    total: std::time::Duration,
    target_rps: Option<u32>,
    status_counts: std::collections::HashMap<u16, u64>,
    error_counts: std::collections::HashMap<String, u64>,
    assertion_failures: u64,
    bytes_sent: u64,
    bytes_recv: u64,
) {
    println!("\n{}", "--- 🏁 RELATÓRIO DO CANNON ---".bold().underline());
    println!("Sucessos:     {}", successes);
    println!("Falhas:       {}", failures);

    let mut metrics = Vec::new();
    if successes > 0 {
        let to_ms_str = |v| format!("{:.2}ms", to_ms(v));
        metrics.push(LatencyMetrics {
            metric: "Mínimo".to_string(),
            value: to_ms_str(hist.min()),
        });
        metrics.push(LatencyMetrics {
            metric: "Média".to_string(),
            value: to_ms_str(hist.mean() as u64),
        });
        metrics.push(LatencyMetrics {
            metric: "p50 (Mediana)".to_string(),
            value: to_ms_str(hist.value_at_quantile(0.5)),
        });
        metrics.push(LatencyMetrics {
            metric: "p95".to_string(),
            value: to_ms_str(hist.value_at_quantile(0.95)),
        });
        metrics.push(LatencyMetrics {
            metric: "p99".to_string(),
            value: to_ms_str(hist.value_at_quantile(0.99)),
        });
        metrics.push(LatencyMetrics {
            metric: "Máximo".to_string(),
            value: to_ms_str(hist.max()),
        });

        report::render_ascii_histogram(hist);
        println!("\n{}", "-------------------------".bright_black());
    }

    let table = tabled::Table::new(metrics)
        .with(tabled::settings::Style::modern())
        .to_string();

    println!("{}", table);
    println!(
        "{} {} | {} {} | {} {:?}",
        "✅ Sucessos:".green().bold(),
        successes.to_string().bright_white(),
        "❌ Falhas:".red().bold(),
        failures.to_string().bright_white(),
        "⏱️ Tempo Total:".cyan().bold(),
        total
    );

    let total_secs = total.as_secs_f64();
    let actual_rps = successes as f64 / total_secs;

    println!("\n{}", "-------------------------".bright_black());

    println!(
        "\n{}",
        "📊 DISTRIBUIÇÃO DE STATUS CODES".bold().bright_white()
    );

    let mut codes: Vec<_> = status_counts.into_iter().collect();

    codes.sort_by_key(|a| a.0);

    for (code, count) in codes {
        let color_code = match code {
            200..=299 => code.to_string().green(),
            400..=499 => code.to_string().yellow(),
            _ => code.to_string().red(),
        };

        println!("  HTTP {}: {}", color_code, count);
    }

    if !error_counts.is_empty() {
        println!("\n{}", "❌ DETALHAMENTO DE FALHAS".bold().red());
        for (err, count) in error_counts {
            println!("  {}: {}", err.yellow(), count);
        }
    }

    if assertion_failures > 0 {
        println!(
            "❌ Falhas de Asserção: {}",
            assertion_failures.to_string().red()
        );
    }

    println!("\n{}", "-------------------------".bright_black());

    println!("\n{}", "📈 EFICIÊNCIA E REDE".bold().bright_white());

    // --- NOVA LÓGICA DE CÁLCULO DE MB/s ---
    let sent_mb = bytes_sent as f64 / 1_048_576.0; // Divide por 1024^2
    let recv_mb = bytes_recv as f64 / 1_048_576.0;

    let throughput_sent = sent_mb / total_secs;
    let throughput_recv = recv_mb / total_secs;

    let sent_mb_str = format!("{:.2}", sent_mb).magenta();
    let throughput_sent_str = format!("{:.2}", throughput_sent).yellow();
    let recv_mb_str = format!("{:.2}", recv_mb).cyan();
    let throughput_recv_str = format!("{:.2}", throughput_recv).yellow();

    println!(
        "📤 Transferência:   {} MB totais ({} MB/s)",
        sent_mb_str, throughput_sent_str
    );
    println!(
        "📥 Recebimento:     {} MB totais ({} MB/s)",
        recv_mb_str, throughput_recv_str
    );
    println!("\n{}", "-------------------------".bright_black());

    println!("\n{}", "📈 EFICIÊNCIA DO CANHÃO".bold().bright_white());

    if let Some(target) = target_rps {
        let efficiency = (actual_rps / target as f64) * 100.0;
        let rps_str = format!("{:.2}", actual_rps).yellow();
        println!("RPS Alvo:      {}", target.to_string().cyan());
        println!("RPS Real:      {} ({:.1}%)", rps_str, efficiency);
    } else {
        println!(
            "RPS Médio:     {} req/s",
            format!("{:.2}", actual_rps).yellow()
        );
    }

    println!("\n{}", "-------------------------".bright_black());
    println!("Teste finalizado em {}s", total.as_secs());
}

fn save_report(
    path: &str,
    args: &Args,
    url: &str,
    successes: u64,
    failures: u64,
    hist: &Histogram<u64>,
) -> Result<(), Box<dyn std::error::Error>> {
    let report = FinalReport {
        target: url.to_string(),
        total_requests: args.count,
        concurrency: args.workers,
        successes,
        failures,
        min_ms: to_ms(hist.min()),
        avg_ms: to_ms(hist.mean() as u64),
        p50_ms: to_ms(hist.value_at_quantile(0.5)),
        p95_ms: to_ms(hist.value_at_quantile(0.95)),
        p99_ms: to_ms(hist.value_at_quantile(0.99)),
        max_ms: to_ms(hist.max()),
    };
    let json = serde_json::to_string_pretty(&report)?;
    std::fs::write(path, json)?;
    println!("📂 Relatório salvo com sucesso!");
    Ok(())
}

fn parse_duration(s: &str) -> u64 {
    let s = s.to_lowercase();

    if s.ends_with('s') {
        s.trim_end_matches('s').parse().unwrap_or(0)
    } else if s.ends_with('m') {
        s.trim_end_matches('m').parse::<u64>().unwrap_or(0) * 60
    } else {
        s.parse().unwrap_or(0)
    }
}

fn print_banner() {
    let banner = r#"
      _____          _   _ _   _  ____  _   _ 
     / ____|   /\   | \ | | \ | |/ __ \| \ | |
    | |       /  \  |  \| |  \| | |  | |  \| |
    | |      / /\ \ | . ` | . ` | |  | | . ` |
    | |____ / ____ \| |\  | |\  | |__| | |\  |
     \_____/_/    \_\_| \_|_| \_|\____/|_| \_|
    "#;
    println!("{}", banner.bright_red().bold());
    println!(
        "{}",
        "--- The High-Velocity Load Tester ---"
            .bright_black()
            .italic()
    );
    println!();
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
