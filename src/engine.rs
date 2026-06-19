use crate::payload;
use crate::report::ShotResult;
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Method,
};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Instant;
use tokio::{
    sync::{mpsc, Semaphore},
    time::Duration,
};

#[allow(clippy::too_many_arguments)]
pub async fn run_producer(
    count: u32,
    workers: u32,
    url: Arc<String>,
    client: Arc<reqwest::Client>,
    tx: mpsc::Sender<ShotResult>,
    rps: Option<u32>,
    body: Option<String>,
    method_str: String,
    headers: Arc<Vec<String>>,
    expect: Option<String>,
    ramp_up_secs: u64,
) {
    let semaphore = Arc::new(Semaphore::new(workers as usize));
    let method = Method::from_bytes(method_str.to_uppercase().as_bytes()).unwrap_or(Method::GET);

    // Compilação de Headers (Zero-cost por requisição)
    let mut header_map = HeaderMap::new();
    let mut has_content_type = false;
    for h in headers.iter() {
        let parts: Vec<&str> = h.splitn(2, ':').collect();
        if parts.len() == 2 {
            let key = parts[0].trim();
            let val = parts[1].trim();
            if key.eq_ignore_ascii_case("content-type") {
                has_content_type = true;
            }
            if let (Ok(k), Ok(v)) = (HeaderName::from_str(key), HeaderValue::from_str(val)) {
                header_map.insert(k, v);
            }
        }
    }

    if body.is_some() && !has_content_type {
        header_map.insert("content-type", HeaderValue::from_static("application/json"));
    }

    let start_engine = Instant::now();
    let target_rps = rps.unwrap_or(0) as f64;
    let body_arc = body.map(Arc::new);
    let expect_arc = expect.map(Arc::new);
    let mut next_shot_time = tokio::time::Instant::now();

    for _ in 0..count {
        // Lógica única e centralizada de Ramp-up e RPS Constante
        if target_rps > 0.0 {
            let elapsed = start_engine.elapsed().as_secs_f64();
            let current_rps = if ramp_up_secs > 0 && elapsed < ramp_up_secs as f64 {
                let progress = elapsed / ramp_up_secs as f64;
                1.0 + (target_rps - 1.0) * progress
            } else {
                target_rps
            };

            let delay = Duration::from_secs_f64(1.0 / current_rps);
            next_shot_time += delay;
            tokio::time::sleep_until(next_shot_time).await;
        }

        // Adquire permissão de forma segura
        let permit = match Arc::clone(&semaphore).acquire_owned().await {
            Ok(p) => p,
            Err(_) => break, // Se o semáforo fechar, encerra graciosamente
        };

        let client_clone = Arc::clone(&client);
        let url_clone = Arc::clone(&url);
        let tx_clone = tx.clone();
        let body_clone = body_arc.as_ref().map(Arc::clone);
        let method_clone = method.clone();
        let expect_clone = expect_arc.as_ref().map(Arc::clone);
        let header_map_clone = header_map.clone();

        tokio::spawn(async move {
            let _permit = permit; // A concorrência é garantida aqui
            let start_request = Instant::now();

            let mut request_builder = client_clone
                .request(method_clone, url_clone.as_str())
                .headers(header_map_clone);

            let mut bytes_sent = 0;

            if let Some(b) = body_clone {
                let dynamic_body = payload::process_payload(&b);
                bytes_sent = dynamic_body.len() as u64;
                request_builder = request_builder.body(dynamic_body);
            }

            let response = request_builder.send().await;

            let (success, status_code, error, assertion_success, bytes_received) = match response {
                Ok(res) => {
                    let s = res.status();
                    let code = s.as_u16();
                    let mut is_success = s.is_success();
                    let mut err_msg = None;
                    let mut assert_ok = true;

                    let rx_bytes = match res.bytes().await {
                        Ok(b) => {
                            if is_success {
                                if let Some(expected) = expect_clone {
                                    let text = String::from_utf8_lossy(&b);
                                    if !text.contains(expected.as_str()) {
                                        is_success = false;
                                        assert_ok = false;
                                        err_msg = Some("Assertion Failed".to_string());
                                    }
                                }
                            }
                            b.len() as u64
                        }
                        Err(_) => {
                            is_success = false;
                            err_msg = Some("Body Read Error".to_string());
                            0
                        }
                    };
                    (is_success, Some(code), err_msg, assert_ok, rx_bytes)
                }
                Err(e) => {
                    let msg = if e.is_timeout() {
                        "Timeout".to_string()
                    } else if e.is_connect() {
                        "Connection Error".to_string()
                    } else {
                        "Network Error".to_string()
                    };
                    (false, None, Some(msg), true, 0)
                }
            };

            let _ = tx_clone
                .send(ShotResult {
                    success,
                    duration: start_request.elapsed(),
                    status_code,
                    error,
                    assertion_success,
                    bytes_sent,
                    bytes_received,
                })
                .await;
        });
    }
}
