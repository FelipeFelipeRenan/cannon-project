// src/engine/worker.rs

use crate::client::target::Target;
use crate::payload::generator::PayloadTemplate;
use hdrhistogram::Histogram;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;

// Painel Global Lock-Free para atualizar a UI em tempo real
#[derive(Default)]
pub struct SharedMetrics {
    pub successes: AtomicU64,
    pub failures: AtomicU64,
    pub bytes_sent: AtomicU64,
    pub bytes_received: AtomicU64,
}

// O que cada worker devolve no fim da sua vida
pub struct WorkerResult {
    pub histogram: Histogram<u64>,
    pub status_counts: HashMap<u16, u64>,
    pub error_counts: HashMap<String, u64>,
    pub assertion_failures: u64,
}

pub struct CsvRecord {
    pub relative_ms: String,
    pub status: String,
    pub latency_ms: String,
    pub error: String,
}

#[allow(clippy::too_many_arguments)]
pub async fn run_workers(
    count: u32,
    workers: u32,
    template: Option<Arc<PayloadTemplate>>,
    rps: Option<u32>,
    target: Arc<Target>,
    shared_metrics: Arc<SharedMetrics>,
    csv_tx: Option<mpsc::Sender<CsvRecord>>,
    start_time: Instant,
) -> Vec<WorkerResult> {
    let (job_tx, async_job_rx) = async_channel::bounded::<()>(workers as usize);
    let mut handles = Vec::new();

    for _ in 0..workers {
        let template = template.clone();
        let rx = async_job_rx.clone();
        let target = target.clone();
        let shared = shared_metrics.clone();
        let csv_tx = csv_tx.clone();

        let handle = tokio::spawn(async move {
            let mut payload_buffer = Vec::with_capacity(1024);

            // Estado LOCAL do worker (Sem Lock!)
            let mut local_hist = Histogram::<u64>::new_with_bounds(1, 60_000_000, 3).unwrap();
            let mut local_status = HashMap::new();
            let mut local_errors = HashMap::new();
            let mut local_assert_failures = 0;

            while rx.recv().await.is_ok() {
                if let Some(tpl) = &template {
                    tpl.render(&mut payload_buffer);
                }

                let payload_ref: &[u8] = if template.is_some() {
                    payload_buffer.as_slice()
                } else {
                    &[]
                };
                let res = target.fire(payload_ref).await;

                // 1. Atualiza Atomics Globais (Rápido, vai direto pra L1 Cache)
                if res.success {
                    shared.successes.fetch_add(1, Ordering::Relaxed);
                    let _ = local_hist.record(res.duration.as_micros() as u64);
                } else {
                    shared.failures.fetch_add(1, Ordering::Relaxed);
                }
                shared
                    .bytes_sent
                    .fetch_add(res.bytes_sent, Ordering::Relaxed);
                shared
                    .bytes_received
                    .fetch_add(res.bytes_received, Ordering::Relaxed);

                // 2. Atualiza HashMaps Locais
                if let Some(code) = res.status_code {
                    *local_status.entry(code).or_insert(0) += 1;
                }
                if let Some(err) = &res.error {
                    *local_errors.entry(err.clone()).or_insert(0) += 1;
                }
                if !res.assertion_success {
                    local_assert_failures += 1;
                }

                // 3. I/O Assíncrono de CSV (Se ativado)
                if let Some(tx) = &csv_tx {
                    let rec = CsvRecord {
                        relative_ms: start_time.elapsed().as_millis().to_string(),
                        status: res
                            .status_code
                            .map(|c| c.to_string())
                            .unwrap_or_else(|| "N/A".to_string()),
                        latency_ms: res.duration.as_millis().to_string(),
                        error: res.error.unwrap_or_default(),
                    };
                    let _ = tx.send(rec).await;
                }
            }

            // Devolve o balanço do worker quando o teste acabar
            WorkerResult {
                histogram: local_hist,
                status_counts: local_status,
                error_counts: local_errors,
                assertion_failures: local_assert_failures,
            }
        });
        handles.push(handle);
    }

    // Cronômetro do RPS constante
    if let Some(r) = rps {
        let mut interval =
            tokio::time::interval(std::time::Duration::from_secs_f64(1.0 / r as f64));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Burst);
        for _ in 0..count {
            interval.tick().await;
            let _ = job_tx.send(()).await;
        }
    } else {
        for _ in 0..count {
            let _ = job_tx.send(()).await;
        }
    }

    drop(job_tx); // Fecha o canal de jobs para avisar os workers que acabou

    // Coleta todos os resultados locais
    let mut results = Vec::new();
    for handle in handles {
        if let Ok(res) = handle.await {
            results.push(res);
        }
    }
    results
}
