use crate::client::target::{TargetClient, TargetResult};
use crate::payload::generator::process_payload;
use std::sync::Arc;
use tokio::sync::mpsc;

#[allow(clippy::too_many_arguments)]
pub async fn run_workers(
    count: u32,
    workers: u32,
    body: Option<Arc<String>>,
    tx: mpsc::Sender<TargetResult>, // Agora envia o TargetResult diretamente
    rps: Option<u32>,
    target: Arc<dyn TargetClient>, // A MÁGICA DA ABSTRAÇÃO
) {
    let (job_tx, async_job_rx) = async_channel::bounded::<()>(workers as usize);
    let mut handles = Vec::new();

    for _ in 0..workers {
        let body = body.clone();
        let tx = tx.clone();
        let rx = async_job_rx.clone();
        let target = target.clone();

        let handle = tokio::spawn(async move {
            while rx.recv().await.is_ok() {
                // 1. Gera os bytes do payload
                let mut payload_bytes = Vec::new();
                if let Some(b) = &body {
                    let processed = process_payload(b);
                    payload_bytes = processed.into_bytes();
                }

                // 2. Dispara contra o alvo (Não importa se é HTTP ou TCP)
                let res = target.fire(&payload_bytes).await;

                // 3. Envia o resultado para o main
                let _ = tx.send(res).await;
            }
        });
        handles.push(handle);
    }

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

    drop(job_tx);
    for handle in handles {
        let _ = handle.await;
    }
}
