use std::collections::HashMap;

use colored::Colorize;
use serde::Serialize;
use tabled::Tabled;

pub struct ShotResult {
    pub success: bool,
    pub duration: std::time::Duration,
    pub status_code: Option<u16>,
    pub error: Option<String>,
    pub assertion_success: bool,
    pub bytes_sent: u64,
    pub bytes_received: u64,
}

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
    let template = r#"
    <!DOCTYPE html>
    <html lang="pt-PT">
    <head>
        <meta charset="UTF-8">
        <meta name="viewport" content="width=device-width, initial-scale=1.0">
        <title>Cannon - Painel de Telemetria</title>
        <script src="https://cdn.jsdelivr.net/npm/chart.js"></script>
        <style>
            :root { --bg: #0d1117; --card-bg: #161b22; --border: #30363d; --text: #c9d1d9; --blue: #58a6ff; --green: #3fb950; --red: #f85149; --yellow: #d29922; }
            body { font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif; background-color: var(--bg); color: var(--text); margin: 0; padding: 20px; }
            .container { max-width: 1200px; margin: auto; background: var(--card-bg); padding: 30px; border-radius: 8px; box-shadow: 0 4px 15px rgba(0,0,0,0.5); }
            
            /* ESTILOS DO BOTÃO DE IMPRESSÃO */
            .header-bar { display: flex; justify-content: space-between; align-items: center; border-bottom: 1px solid var(--border); padding-bottom: 10px; margin-bottom: 20px; }
            .header-bar h1 { margin: 0; color: var(--blue); font-size: 2em; }
            .print-btn { background: var(--blue); color: #fff; border: none; padding: 8px 16px; border-radius: 5px; cursor: pointer; font-weight: bold; font-size: 14px; transition: 0.2s; }
            .print-btn:hover { background: #388bfd; }
            
            .subtitle { text-align: center; color: #8b949e; margin-top: 0; margin-bottom: 30px; font-weight: 500; }
            .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 15px; margin-bottom: 30px; }
            .card { background: #21262d; padding: 20px; border-radius: 8px; text-align: center; border: 1px solid var(--border); }
            .card h3 { margin: 0 0 10px 0; font-size: 13px; color: #8b949e; text-transform: uppercase; letter-spacing: 1px; }
            .card p { margin: 0; font-size: 26px; font-weight: bold; color: #fff; }
            .target-text { font-size: 14px !important; font-weight: normal !important; word-break: break-all; }
            .success-text { color: var(--green) !important; }
            .fail-text { color: var(--red) !important; }
            .warn-text { color: var(--yellow) !important; }
            .charts-grid { display: grid; grid-template-columns: 2fr 1fr; gap: 20px; margin-top: 20px; }
            .chart-container { background: #21262d; padding: 20px; border-radius: 8px; border: 1px solid var(--border); position: relative; height: 350px; }
            .errors-section { margin-top: 30px; background: #21262d; padding: 20px; border-radius: 8px; border: 1px solid var(--border); display: none; }
            table { width: 100%; border-collapse: collapse; margin-top: 10px; }
            th, td { text-align: left; padding: 12px; border-bottom: 1px solid var(--border); }
            th { color: #8b949e; text-transform: uppercase; font-size: 13px; }
            
            /* OTIMIZAÇÃO PARA PDF */
            @media print {
                body { background: #fff; color: #000; }
                .container, .card, .chart-container, .errors-section { background: #fff; border: 1px solid #ccc; box-shadow: none; }
                .print-btn { display: none; } /* Esconde o botão ao imprimir */
                .subtitle, .card h3, th { color: #555; }
                * { -webkit-print-color-adjust: exact; color-adjust: exact; }
            }
            @media (max-width: 768px) { .charts-grid { grid-template-columns: 1fr; } }
        </style>
    </head>
    <body>
        <div class="container">
            <div class="header-bar">
                <h1>🚀 Telemetria Cannon</h1>
                <button class="print-btn" onclick="window.print()">🖨️ Guardar PDF</button>
            </div>
            <p class="subtitle">Análise Detalhada de Performance HTTP</p>
            
            <div id="summary" class="grid"></div>
            
            <div class="charts-grid">
                <div class="chart-container">
                    <canvas id="latencyChart"></canvas>
                </div>
                <div class="chart-container">
                    <canvas id="statusChart"></canvas>
                </div>
            </div>

            <div id="errorsSection" class="errors-section">
                <h3 style="color: var(--red); margin-top: 0;">⚠️ Registo de Falhas</h3>
                <table id="errorsTable">
                    <thead><tr><th>Tipo de Erro</th><th>Ocorrências</th></tr></thead>
                    <tbody></tbody>
                </table>
            </div>
        </div>

        <script>
            const data = /*JSON_PAYLOAD*/;

            const mbSent = data.bytes_sent / 1048576;
            const mbRecv = data.bytes_received / 1048576;
            const tpRecv = (mbRecv / data.duration_secs).toFixed(2);

            // Calcula a cor e o rótulo do Apdex
            let apdexColor = 'fail-text';
            let apdexLabel = 'Inaceitável';
            if (data.apdex_score >= 0.94) { apdexColor = 'success-text'; apdexLabel = 'Excelente'; }
            else if (data.apdex_score >= 0.85) { apdexColor = 'success-text'; apdexLabel = 'Bom'; }
            else if (data.apdex_score >= 0.70) { apdexColor = 'warn-text'; apdexLabel = 'Razoável'; }
            else if (data.apdex_score >= 0.50) { apdexColor = 'warn-text'; apdexLabel = 'Pobre'; }

            const summaryDiv = document.getElementById('summary');
            const metrics = [
                { label: 'Alvo', value: data.target, cls: 'target-text' },
                { label: 'Índice Apdex', value: data.apdex_score.toFixed(2) + ' (' + apdexLabel + ')', cls: apdexColor }, // --- NOVO CARD AQUI ---
                { label: 'RPS Real', value: data.actual_rps.toFixed(2) + ' req/s' },
                { label: 'Download Máx', value: tpRecv + ' MB/s' },
                { label: 'Sucessos', value: data.successes, cls: 'success-text' },
                { label: 'Falhas', value: data.failures, cls: data.failures > 0 ? 'fail-text' : '' }
            ];
            
            metrics.forEach(m => {
                summaryDiv.innerHTML += `<div class="card"><h3>${m.label}</h3><p class="${m.cls || ''}">${m.value}</p></div>`;
            });

            Chart.defaults.color = '#8b949e';
            Chart.defaults.font.family = "'Segoe UI', sans-serif";

            // Gráfico de Latência
            new Chart(document.getElementById('latencyChart').getContext('2d'), {
                type: 'bar',
                data: {
                    labels: ['Mínimo', 'Média', 'p50', 'p95', 'p99', 'Máximo'],
                    datasets: [{
                        label: 'Latência (ms)',
                        data: [data.min_ms, data.avg_ms, data.p50_ms, data.p95_ms, data.p99_ms, data.max_ms],
                        backgroundColor: 'rgba(88, 166, 255, 0.7)',
                        borderColor: '#58a6ff',
                        borderWidth: 1,
                        borderRadius: 4
                    }]
                },
                options: { responsive: true, maintainAspectRatio: false, plugins: { title: { display: true, text: 'Percentis de Latência' } }, scales: { y: { beginAtZero: true, grid: { color: '#30363d' } }, x: { grid: { display: false } } } }
            });

            // Gráfico de Status Codes
            const statusLabels = Object.keys(data.status_codes).map(code => `HTTP ${code}`);
            const statusValues = Object.values(data.status_codes);
            const statusColors = Object.keys(data.status_codes).map(code => {
                if(code.startsWith('2')) return '#3fb950';
                if(code.startsWith('4')) return '#d29922';
                if(code.startsWith('5')) return '#f85149';
                return '#8b949e';
            });

            new Chart(document.getElementById('statusChart').getContext('2d'), {
                type: 'doughnut',
                data: {
                    labels: statusLabels,
                    datasets: [{
                        data: statusValues,
                        backgroundColor: statusColors,
                        borderWidth: 0
                    }]
                },
                options: { responsive: true, maintainAspectRatio: false, plugins: { title: { display: true, text: 'Distribuição de Status' }, legend: { position: 'bottom' } }, cutout: '65%' }
            });

            // Lógica de Tabela de Erros
            const errorKeys = Object.keys(data.errors);
            if (errorKeys.length > 0) {
                document.getElementById('errorsSection').style.display = 'block';
                const tbody = document.querySelector('#errorsTable tbody');
                errorKeys.forEach(err => {
                    tbody.innerHTML += `<tr><td style="color: var(--yellow)">${err}</td><td>${data.errors[err]}</td></tr>`;
                });
            }
        </script>
    </body>
    </html>
    "#;

    let final_html = template.replace("/*JSON_PAYLOAD*/", report_json);
    std::fs::write(path, final_html)?;
    Ok(())
}