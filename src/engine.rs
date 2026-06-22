use crate::payload::process_payload;
use reqwest::Client;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::Instant;

pub struct RequestResult {
    pub duration: std::time::Duration,
    pub status_code: Option<u16>,
    pub error: Option<String>,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub success: bool,
    pub assertion_success: bool,
}

#[allow(clippy::too_many_arguments)]
pub async fn run_workers(
    count: u32,
    workers: u32,
    url: Arc<String>,
    method: reqwest::Method,
    body: Option<Arc<String>>,
    headers: Arc<Vec<String>>,
    client: Arc<Client>,
    expected_body: Option<Arc<String>>,
    tx: mpsc::Sender<RequestResult>,
    rps: Option<u32>,
) {
    // 1. Canal interno para distribuir as "balas" para os workers
    // Tamanho do canal é o número de workers para haver backpressure perfeito
    let (job_tx, async_job_rx) = async_channel::bounded::<()>(workers as usize);

    // 2. Criar o Pool Fixo de Workers
    let mut handles = Vec::new();

    for _ in 0..workers {
        let url = url.clone();
        let method = method.clone();
        let body = body.clone();
        let headers = headers.clone();
        let client = client.clone();
        let expected_body = expected_body.clone();
        let tx = tx.clone();
        let rx = async_job_rx.clone();

        let handle = tokio::spawn(async move {
            // O worker fica vivo num loop enquanto houver trabalho na fila
            while rx.recv().await.is_ok() {
                let start = Instant::now();
                let mut req = client.request(method.clone(), url.as_ref());

                // 1. Calcula o tamanho do payload aqui de forma barata
                let mut bytes_sent = 0;
                if let Some(b) = &body {
                    let processed = process_payload(b);
                    bytes_sent = processed.len() as u64;
                    req = req.body(processed);
                }

                for h in headers.iter() {
                    if let Some((k, v)) = h.split_once(':') {
                        req = req.header(k.trim(), v.trim());
                    }
                }

                let res = match req.send().await {
                    Ok(resp) => {
                        let status = resp.status().as_u16();
                        let is_http_success = (200..300).contains(&status);

                        let (error_msg, bytes_recv, assertion_success) = match resp.bytes().await {
                            Ok(bytes) => {
                                let mut err = None;
                                let mut assert_ok = true;

                                if let Some(expected) = &expected_body {
                                    if let Ok(text) = String::from_utf8(bytes.to_vec()) {
                                        if !text.contains(expected.as_ref()) {
                                            err = Some(format!("Mismatch: missing '{}'", expected));
                                            assert_ok = false;
                                        }
                                    }
                                }
                                (err, bytes.len() as u64, assert_ok)
                            }
                            Err(e) => (Some(format!("Read Error: {}", e)), 0, false),
                        };

                        RequestResult {
                            duration: start.elapsed(),
                            status_code: Some(status),
                            error: error_msg,
                            bytes_sent,
                            bytes_received: bytes_recv,
                            success: is_http_success && assertion_success,
                            assertion_success,
                        }
                    }
                    Err(e) => {
                        // O microscópio de infraestrutura
                        let err_category = if e.is_timeout() {
                            "Timeout Exceeded".to_string()
                        } else if e.is_connect() {
                            "Connection Failed (TCP/DNS)".to_string()
                        } else if e.is_decode() {
                            "Body Decode Error".to_string()
                        } else if e.is_redirect() {
                            "Too Many Redirects".to_string()
                        } else {
                            format!("Network Error: {}", e)
                        };

                        RequestResult {
                            duration: start.elapsed(),
                            status_code: e.status().map(|s| s.as_u16()),
                            error: Some(err_category),
                            bytes_sent,
                            bytes_received: 0,
                            success: false,
                            assertion_success: false,
                        }
                    }
                };

                let _ = tx.send(res).await;
            }
        });
        handles.push(handle);
    }

    // 3. O Metrônomo (Producer) agora usa tempo ABSOLUTO para evitar o "Drift"
    if let Some(r) = rps {
        let mut interval =
            tokio::time::interval(std::time::Duration::from_secs_f64(1.0 / r as f64));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Burst);

        for _ in 0..count {
            interval.tick().await; // Espera o próximo pulso exato do relógio
            let _ = job_tx.send(()).await;
        }
    } else {
        // Modo "Fogo Livre": dispara o mais rápido possível se não houver limite de RPS
        for _ in 0..count {
            let _ = job_tx.send(()).await;
        }
    }

    // Fecha a fábrica (os workers saem do loop e morrem limpos)
    drop(job_tx);

    // Espera os últimos disparos terminarem
    for handle in handles {
        let _ = handle.await;
    }
}
