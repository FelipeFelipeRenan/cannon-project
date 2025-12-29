use crate::payload;
use crate::{report::ShotResult};
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
) {
    let semaphore = Arc::new(Semaphore::new(workers as usize));

    let method = Method::from_bytes(method_str.to_uppercase().as_bytes())
        .unwrap_or(Method::GET);

    let mut ticker = if let Some(r) = rps {
        let mut int = interval(Duration::from_secs_f64(1.0 / r as f64));

        int.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        Some(int)
    } else {
        None
    };

    let body_arc = body.map(Arc::new);

    for _ in 0..count {
        if let Some(ref mut t) = ticker {
            t.tick().await;
        }
        let permit = Arc::clone(&semaphore).acquire_owned().await.unwrap();
        let client_clone = Arc::clone(&client);
        let url_clone = Arc::clone(&url);
        let tx_clone = tx.clone();
        let body_clone = body_arc.as_ref().map(Arc::clone);
        let method_clone= method.clone();
        let headers_clone = Arc::clone(&headers);

        tokio::spawn(async move {
            let _permit = permit;
            let start_request = Instant::now();

            let mut request_builder = client_clone.request(method_clone, url_clone.as_str());

            for h in headers_clone.iter(){
                let parts: Vec<&str> = h.splitn(2, ':').collect();
                if parts.len() == 2{
                    request_builder = request_builder.header(parts[0].trim(), parts[1].trim())
                }
            }
            if let Some(b) = body_clone {
                let dynamic_body = payload::process_payload(&b);
                request_builder = request_builder
                    .header("Content-Type", "application/json")
                    .body(dynamic_body);
            } 

            let response = request_builder.send().await;
            let (success, status_code) = match response{
                Ok(res) => {
                    let s = res.status();
                    (s.is_success(), Some(s.as_u16()))
                },
                Err(_) => (false, None),
            };

            let _ = tx_clone
                .send(ShotResult {
                    success,
                    duration: start_request.elapsed(),
                    status_code,
                })
                .await;
        });
    }
}
