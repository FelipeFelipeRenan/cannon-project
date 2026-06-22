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
    let mut args = Args::parse();

    if args.update {
        update()?;
        return Ok(());
    }

    // Se o utilizador passou um ficheiro YAML, lemos e fazemos o merge
    if let Some(config_path) = &args.config {
        let yaml_str = std::fs::read_to_string(config_path)
            .unwrap_or_else(|_| panic!("❌ Erro: Não foi possível ler o arquivo {}", config_path));

        let conf: args::FileConfig = serde_yaml::from_str(&yaml_str)
            .unwrap_or_else(|e| panic!("❌ Erro no formato YAML: {}", e));

        // Regra de Ouro: O YAML sobrepõe os valores padrão da CLI
        if conf.url.is_some() {
            args.url = conf.url;
        }
        if let Some(w) = conf.workers {
            args.workers = w;
        }
        if let Some(c) = conf.count {
            args.count = c;
        }
        if let Some(rps) = conf.rps {
            args.rps = Some(rps);
        }
        if let Some(t) = conf.timeout {
            args.timeout = t;
        }
        if let Some(m) = conf.method {
            args.method = m;
        }
        if let Some(body) = conf.body {
            args.body = Some(body);
        }
        if let Some(exp) = conf.expect {
            args.expect = Some(exp);
        }
        if let Some(apdex) = conf.apdex_t {
            args.apdex_t = apdex;
        }
        if let Some(ins) = conf.insecure {
            args.insecure = ins;
        }
        if let Some(csv_path) = conf.csv {
            args.csv = Some(csv_path)
        }
        if let Some(h2) = conf.http2 {
            args.http2 = h2;
        }
        if let Some(ct) = conf.connect_timeout {
            args.connect_timeout = ct;
        }
        // Concatena headers do YAML com os headers passados na CLI (se houver)
        if let Some(mut yaml_headers) = conf.headers {
            yaml_headers.append(&mut args.headers);
            args.headers = yaml_headers;
        }
    }
    // Validação final de segurança: O URL tem que existir depois do merge
    if args.url.is_none() {
        eprintln!(
            "❌ Erro: É necessário fornecer uma URL via flag (-u) ou no ficheiro YAML (--config)"
        );
        std::process::exit(1);
    }

    let url_str = match args.url.as_ref() {
        Some(u) => u.clone(),
        None => {
            eprintln!(
                "{} O argumento '--url <URL>' é obrigatório para iniciar o teste.",
                "error:".red().bold()
            );
            std::process::exit(1);
        }
    };

    let parsed_percentiles: Vec<f64> = args
        .percentiles
        .split(',')
        .filter_map(|s| s.trim().parse::<f64>().ok())
        .map(|p| p / 100.0)
        .collect();

    let mut client_builder = reqwest::Client::builder()
        .tcp_nodelay(true)
        .pool_max_idle_per_host(args.workers as usize)
        .pool_idle_timeout(Some(std::time::Duration::from_secs(90)))
        .user_agent(&args.user_agent)
        .timeout(std::time::Duration::from_millis(args.timeout))
        .connect_timeout(std::time::Duration::from_millis(args.connect_timeout))
        .timeout(std::time::Duration::from_millis(args.timeout));

    if args.insecure {
        client_builder = client_builder.danger_accept_invalid_certs(true);
    }

    if args.http2 {
        client_builder = client_builder.http2_prior_knowledge();
    }

    let client = Arc::new(client_builder.build()?);

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
    tokio::spawn(engine::run_workers(
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
                            let status = res.status_code.map(|c| c.to_string()).unwrap_or_else(|| "N/A".to_string());
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
            report::generate_html_report(path, &json_data)?;
            println!(
                "🌐 Relatório HTML salvo com sucesso em {}!",
                path.bright_cyan()
            );
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
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
    percentiles: &[f64],
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

        // O MOTOR DINÂMICO DE PERCENTIS
        for &p in percentiles {
            let p_val = p * 100.0;
            
            // Se for cravado (ex: 50.0), imprime "p50". Se for fracionado (ex: 99.9), imprime "p99.9"
            let p_label = if p_val.fract() == 0.0 {
                if p_val == 50.0 {
                    "p50 (Mediana)".to_string()
                } else {
                    format!("p{:.0}", p_val)
                }
            } else {
                format!("p{}", p_val)
            };

            metrics.push(LatencyMetrics {
                metric: p_label,
                value: to_ms_str(hist.value_at_quantile(p)),
            });
        }

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

        // Converte para vetor e ordena (maior quantidade de erros no topo)
        let mut sorted_errors: Vec<_> = error_counts.into_iter().collect();
        sorted_errors.sort_by(|a, b| b.1.cmp(&a.1));

        for (err, count) in sorted_errors {
            // Calcula a porcentagem em relação ao total de falhas
            let perc = (count as f64 / failures as f64) * 100.0;
            println!(
                "  {:<30} {:>6} ({:>4.1}%)",
                err.yellow(),
                count.to_string().bright_white(),
                perc
            );
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
