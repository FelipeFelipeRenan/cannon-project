use async_channel::{Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub struct TargetResult {
    pub success: bool,
    pub duration: Duration,
    pub status_code: Option<u16>,
    pub error: Option<String>,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub assertion_success: bool,
}

impl TargetResult {
    pub fn success(duration: Duration, bytes_sent: u64, bytes_received: u64) -> Self {
        Self {
            success: true,
            duration,
            status_code: None,
            error: None,
            bytes_sent,
            bytes_received,
            assertion_success: true,
        }
    }
    pub fn fail(duration: Duration, error: String) -> Self {
        Self {
            success: false,
            duration,
            status_code: None,
            error: Some(error),
            bytes_sent: 0,
            bytes_received: 0,
            assertion_success: false,
        }
    }
}

// --- ENUM POLIMÓRFICO (STATIC DISPATCH - ZERO VTABLE OVERHEAD) ---
pub enum Target {
    Http {
        client: reqwest::Client,
        url: String,
        method: reqwest::Method,
        headers: Arc<Vec<String>>,
        expected_body: Option<Arc<String>>,
    },
    Tcp {
        pool_tx: Sender<TcpStream>,
        pool_rx: Receiver<TcpStream>,
        address: String,
    },
}

impl Target {
    // Factory method para HTTP (Aceita os Arcs diretamente)
    pub fn new_http(
        client: reqwest::Client,
        url: String,
        method: reqwest::Method,
        headers: Arc<Vec<String>>,
        expected_body: Option<Arc<String>>,
    ) -> Self {
        Self::Http {
            client,
            url,
            method,
            headers,
            expected_body,
        }
    }

    // Factory method para TCP (Assíncrono, constrói o Pool e devolve Result)
    pub async fn new_tcp(address: &str, workers: u32) -> Result<Self, String> {
        let (tx, rx) = async_channel::bounded(workers as usize);
        println!("🔌 Estabelecendo pool de {} conexões TCP...", workers);
        for _ in 0..workers {
            match TcpStream::connect(address).await {
                Ok(stream) => {
                    let _ = tx.send(stream).await;
                }
                Err(e) => return Err(format!("Falha ao conectar: {}", e)),
            }
        }
        Ok(Self::Tcp {
            pool_tx: tx,
            pool_rx: rx,
            address: address.to_string(),
        })
    }

    #[inline(always)]
    fn trigger_reconnect(pool_tx: async_channel::Sender<TcpStream>, address: String) {
        tokio::spawn(async move {
            if let Ok(new_stream) = TcpStream::connect(&address).await {
                let _ = pool_tx.send(new_stream).await;
            }
        });
    }

    // O compilador injeta esse match direto no loop do Worker!
    #[inline(always)]
    pub async fn fire(&self, payload: &[u8]) -> TargetResult {
        let start = std::time::Instant::now();

        match self {
            Target::Http {
                client,
                url,
                method,
                headers,
                expected_body,
            } => {
                let mut req = client.request(method.clone(), url);
                if !payload.is_empty() {
                    req = req.body(payload.to_vec());
                }
                for h in headers.iter() {
                    if let Some((k, v)) = h.split_once(':') {
                        req = req.header(k.trim(), v.trim());
                    }
                }

                match req.send().await {
                    Ok(resp) => {
                        let status = resp.status().as_u16();
                        let (error_msg, bytes_recv, assert_ok) = match resp.bytes().await {
                            Ok(bytes) => {
                                let mut err = None;
                                let mut ok = true;
                                if let Some(expected) = expected_body {
                                    if let Ok(text) = std::str::from_utf8(&bytes) {
                                        if !text.contains(expected.as_ref().as_str()) {
                                            err = Some(format!("Mismatch: missing '{}'", expected));
                                            ok = false;
                                        }
                                    }
                                }
                                (err, bytes.len() as u64, ok)
                            }
                            Err(e) => (Some(format!("Read Error: {}", e)), 0, false),
                        };
                        TargetResult {
                            duration: start.elapsed(),
                            status_code: Some(status),
                            error: error_msg,
                            bytes_sent: payload.len() as u64,
                            bytes_received: bytes_recv,
                            success: (200..300).contains(&status) && assert_ok,
                            assertion_success: assert_ok,
                        }
                    }
                    Err(e) => TargetResult::fail(start.elapsed(), format!("Network Error: {}", e)),
                }
            }

            Target::Tcp {
                pool_tx,
                pool_rx,
                address,
            } => {
                if let Ok(mut stream) = pool_rx.recv().await {
                    // 1. Escreve APENAS os bytes puros do template. Sem \n, sem magia.
                    // 1. Tenta escrever. Se falhar, a conexão caiu. Dispara a cura!
                    if let Err(e) = stream.write_all(payload).await {
                        Self::trigger_reconnect(pool_tx.clone(), address.clone());
                        return TargetResult::fail(start.elapsed(), format!("Broken Pipe: {}", e));
                    }
                    let _ = stream.flush().await;

                    // 2. Tenta ler o ACK. Se falhar, o alvo fechou a porta na nossa cara. Cura!
                    let mut buffer = [0; 1];
                    if let Err(e) = stream.read_exact(&mut buffer).await {
                        Self::trigger_reconnect(pool_tx.clone(), address.clone());
                        return TargetResult::fail(
                            start.elapsed(),
                            format!("Connection Reset: {}", e),
                        );
                    }

                    // 3. Tudo perfeito. Devolve o socket intacto para o Pool.
                    let _ = pool_tx.send(stream).await;

                    TargetResult::success(start.elapsed(), payload.len() as u64, 1)
                } else {
                    TargetResult::fail(start.elapsed(), "TCP Pool Exhausted".to_string())
                }
            }
        }
    }
}
