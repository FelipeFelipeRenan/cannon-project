use crate::payload;
use crate::report::ShotResult;
use reqwest::Method;
use std::sync::Arc;
use std::time::Instant;
use tokio::{
    sync::{mpsc, Semaphore},
    time::{interval, Duration},
};

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

    let mut ticker = if let Some(r) = rps {
        let mut int = interval(Duration::from_secs_f64(1.0 / r as f64));

        int.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        Some(int)
    } else {
        None
    };

    let start_engine = Instant::now();
    let target_rps = rps.unwrap_or(0) as f64;
    let body_arc = body.map(Arc::new);
    let expect_arc = expect.map(Arc::new);

    for _ in 0..count {
        if let Some(ref mut t) = ticker {
            t.tick().await;
        }
        let permit = Arc::clone(&semaphore).acquire_owned().await.unwrap();
        let client_clone = Arc::clone(&client);
        let url_clone = Arc::clone(&url);
        let tx_clone = tx.clone();
        let body_clone = body_arc.as_ref().map(Arc::clone);
        let method_clone = method.clone();
        let headers_clone = Arc::clone(&headers);
        let expect_clone = expect_arc.as_ref().map(Arc::clone);

        tokio::spawn(async move {
            if target_rps > 0.0 {
                let elapsed = start_engine.elapsed().as_secs_f64();
                let current_rps = if ramp_up_secs > 0 && elapsed < ramp_up_secs as f64 {
                    let progress = elapsed / ramp_up_secs as f64;
                    // Sobe de 1.0 até o target_rps linearmente
                    1.0 + (target_rps - 1.0) * progress
                } else {
                    target_rps
                };

                let delay = Duration::from_secs_f64(1.0 / current_rps);
                tokio::time::sleep(delay).await;
            }
            let _permit = permit;
            let start_request = Instant::now();

            let mut request_builder = client_clone.request(method_clone, url_clone.as_str());

            // 1. Aplicar headers e detectar Content-Type manual
            let mut has_content_type = false;
            for h in headers_clone.iter() {
                let parts: Vec<&str> = h.splitn(2, ':').collect();
                if parts.len() == 2 {
                    let key = parts[0].trim();
                    if key.to_lowercase() == "content-type" {
                        has_content_type = true;
                    }
                    request_builder = request_builder.header(key, parts[1].trim());
                }
            }

            // 2. Aplicar corpo dinâmico
            if let Some(b) = body_clone {
                let dynamic_body = payload::process_payload(&b);
                if !has_content_type {
                    request_builder = request_builder.header("Content-Type", "application/json");
                }
                request_builder = request_builder.body(dynamic_body);
            }

            // 3. Executar e Validar
            let response = request_builder.send().await;

            let (success, status_code, error, assertion_success) = match response {
                Ok(res) => {
                    let s = res.status();
                    let code = s.as_u16();
                    let mut is_success = s.is_success();
                    let mut err_msg = None;
                    let mut assert_ok = true;

                    // Se a requisição foi 2xx e o usuário quer validar o conteúdo
                    if is_success {
                        if let Some(expected) = expect_clone {
                            // Consome o corpo da resposta como texto
                            match res.text().await {
                                Ok(text) => {
                                    if !text.contains(expected.as_str()) {
                                        is_success = false;
                                        assert_ok = false;
                                        err_msg = Some("Assertion Failed".to_string());
                                    }
                                }
                                Err(_) => {
                                    is_success = false;
                                    assert_ok = false;
                                    err_msg = Some("Body Read Error".to_string());
                                }
                            }
                        }
                    }
                    (is_success, Some(code), err_msg, assert_ok)
                }
                Err(e) => {
                    let msg = if e.is_timeout() {
                        "Timeout".to_string()
                    } else if e.is_connect() {
                        "Connection Error".to_string()
                    } else {
                        "Network Error".to_string()
                    };
                    (false, None, Some(msg), true)
                }
            };

            let _ = tx_clone
                .send(ShotResult {
                    success,
                    duration: start_request.elapsed(),
                    status_code,
                    error,
                    assertion_success,
                })
                .await;
        });
    }
}
