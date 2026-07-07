// src/engine/worker.rs

use crate::client::target::{Target, TargetResult};
use crate::payload::generator::PayloadTemplate;
use std::sync::Arc;
use tokio::sync::mpsc;

#[allow(clippy::too_many_arguments)]
pub async fn run_workers(
    count: u32,
    workers: u32,
    template: Option<Arc<PayloadTemplate>>, // 🚀 Novo motor de templates (Texto/Binário)
    tx: mpsc::Sender<TargetResult>,
    rps: Option<u32>,
    target: Arc<Target>,
) {
    let (job_tx, async_job_rx) = async_channel::bounded::<()>(workers as usize);
    let mut handles = Vec::new();

    for _ in 0..workers {
        let template = template.clone();
        let tx = tx.clone();
        let rx = async_job_rx.clone();
        let target = target.clone();

        let handle = tokio::spawn(async move {
            // 🚀 O SEGREDO ESTÁ AQUI: Aloca memória UMA ÚNICA VEZ antes do loop começar
            let mut payload_buffer = Vec::with_capacity(1024);

            // src/engine/worker.rs (trecho do loop)

            while rx.recv().await.is_ok() {
                // Renderiza o template reciclando o buffer
                if let Some(tpl) = &template {
                    tpl.render(&mut payload_buffer);
                }

                // 🎯 CORREÇÃO: Força o tipo explícito &[u8] usando as_slice()
                let payload_ref: &[u8] = if template.is_some() {
                    payload_buffer.as_slice()
                } else {
                    &[]
                };

                // Dispara contra o alvo repassando a referência
                let res = target.fire(payload_ref).await;

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
