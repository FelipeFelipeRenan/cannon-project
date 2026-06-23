use std::collections::HashMap;

use colored::Colorize;
use hdrhistogram::Histogram;
use serde::Serialize;
use tabled::Tabled;

#[derive(Serialize, Tabled)]
pub struct FinalReport {
    #[tabled(rename = "Target URL")]
    pub target: String,
    #[tabled(rename = "Total")]
    pub total_requests: u32,
    #[tabled(rename = "Workers")]
    pub concurrency: u32,
    #[tabled(rename = "Success")]
    pub successes: u64,
    #[tabled(rename = "Failures")]
    pub failures: u64,
    pub min_ms: f64,
    pub avg_ms: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub max_ms: f64,

    pub actual_rps: f64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    #[tabled(skip)]
    pub status_codes: HashMap<u16, u64>,
    #[tabled(skip)]
    pub errors: HashMap<String, u64>,
    pub duration_secs: f64,

    pub apdex_score: f64,
}

#[derive(Tabled)]
pub struct LatencyMetrics {
    pub metric: String,
    pub value: String,
}

pub fn to_ms(us: u64) -> f64 {
    us as f64 / 1000.0
}

pub fn render_ascii_histogram(hist: &hdrhistogram::Histogram<u64>) {
    println!("\n{}", "📊 DISTRIBUIÇÃO DE LATÊNCIA".bold().bright_white());

    let min = hist.min();
    let max = hist.max();

    let step = (max - min) / 10;

    let step = if step == 0 { 1 } else { step };

    let mut max_count = 0;

    for bucket in hist.iter_linear(step) {
        if bucket.count_since_last_iteration() > max_count {
            max_count = bucket.count_since_last_iteration();
        }
    }

    if max_count == 0 {
        return;
    }

    for bucket in hist.iter_linear(step) {
        let count = bucket.count_since_last_iteration();
        let percent = (count as f64 / hist.len() as f64) * 100.0;

        let bar_width = (count as f64 / max_count as f64 * 30.0) as usize;
        let bar = "█".repeat(bar_width);

        println!(
            "{:>8.2}ms [{:<30}] {:>6} ({:.1}%)",
            to_ms(bucket.value_iterated_to()),
            bar.cyan(),
            count,
            percent
        );

        if bucket.value_iterated_to() >= max {
            break;
        }
    }
}
pub fn generate_html_report(path: &str, report_json: &str) -> std::io::Result<()> {
    let template = include_str!("../../templates/dashboard.html");
    let final_html = template.replace("/*JSON_PAYLOAD*/", report_json);
    std::fs::write(path, final_html)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn print_summary(
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

        render_ascii_histogram(hist);
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
        sorted_errors.sort_by_key(|item| std::cmp::Reverse(item.1));
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

pub fn print_banner() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_ms_conversion() {
        // to_ms converte microssegundos (us) para milissegundos (ms)
        assert_eq!(to_ms(1_000), 1.0);
        assert_eq!(to_ms(500), 0.5);
        assert_eq!(to_ms(1_500_000), 1500.0);
        assert_eq!(to_ms(0), 0.0);
    }
}
