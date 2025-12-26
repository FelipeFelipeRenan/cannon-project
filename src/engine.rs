use crate::report::ShotResult;
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
) {
    let semaphore = Arc::new(Semaphore::new(workers as usize));

    let mut ticker = if let Some(r) = rps {
        let mut int = interval(Duration::from_secs_f64(1.0 / r as f64));

        int.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        Some(int)
    } else {
        None
    };

    for _ in 0..count {
        if let Some(ref mut t) = ticker {
            t.tick().await;
        }
        let permit = Arc::clone(&semaphore).acquire_owned().await.unwrap();
        let client_clone = Arc::clone(&client);
        let url_clone = Arc::clone(&url);
        let tx_clone = tx.clone();

        tokio::spawn(async move {
            let _permit = permit;
            let start_request = Instant::now();

            let response = client_clone.get(url_clone.as_str()).send().await;
            let success = response.is_ok() && response.unwrap().status().is_success();

            let _ = tx_clone
                .send(ShotResult {
                    success,
                    duration: start_request.elapsed(),
                })
                .await;
        });
    }
}
