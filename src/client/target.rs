use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt}; // NECESSÁRIO PARA O TCP FUNCIONAR
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

// Helpers para o TcpTarget
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

#[async_trait]
pub trait TargetClient: Send + Sync {
    async fn fire(&self, payload: &[u8]) -> TargetResult;
}

// --- ALVO HTTP ---
pub struct HttpTarget {
    pub client: reqwest::Client,
    pub url: String,
    pub method: reqwest::Method,
    pub headers: Arc<Vec<String>>,
    pub expected_body: Option<Arc<String>>,
}

#[async_trait]
impl TargetClient for HttpTarget {
    async fn fire(&self, payload: &[u8]) -> TargetResult {
        let start = std::time::Instant::now();
        let mut req = self.client.request(self.method.clone(), &self.url);

        if !payload.is_empty() {
            req = req.body(payload.to_vec());
        }

        for h in self.headers.iter() {
            if let Some((k, v)) = h.split_once(':') {
                req = req.header(k.trim(), v.trim());
            }
        }

        match req.send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let is_http_success = (200..300).contains(&status);
                let (error_msg, bytes_recv, assertion_success) = match resp.bytes().await {
                    Ok(bytes) => {
                        let mut err = None;
                        let mut assert_ok = true;
                        if let Some(expected) = &self.expected_body {
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

                TargetResult {
                    duration: start.elapsed(),
                    status_code: Some(status),
                    error: error_msg,
                    bytes_sent: payload.len() as u64,
                    bytes_received: bytes_recv,
                    success: is_http_success && assertion_success,
                    assertion_success,
                }
            }
            Err(e) => {
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

                TargetResult::fail(start.elapsed(), err_category)
            }
        }
    }
}

// --- ALVO TCP ---
pub struct TcpTarget {
    pub address: String,
}

#[async_trait]
impl TargetClient for TcpTarget {
    async fn fire(&self, payload: &[u8]) -> TargetResult {
        let start = std::time::Instant::now();
        match TcpStream::connect(&self.address).await {
            Ok(mut stream) => {
                // 1. Injetamos a quebra de linha real (0x0A) se não existir
                let mut final_payload = payload.to_vec();
                if !final_payload.ends_with(b"\n") {
                    final_payload.push(b'\n');
                }

                // 2. Dispara os bytes na rede
                if let Err(e) = stream.write_all(&final_payload).await {
                    return TargetResult::fail(start.elapsed(), e.to_string());
                }

                // 3. Força o envio do buffer do SO imediatamente
                let _ = stream.flush().await;

                // 4. Aguarda o ACK do Go
                let mut buffer = [0; 1];
                let _ = stream.read(&mut buffer).await;

                TargetResult::success(start.elapsed(), final_payload.len() as u64, 1)
            }
            Err(e) => TargetResult::fail(start.elapsed(), e.to_string()),
        }
    }
}
