use async_channel::{Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Result of a single request executed against a [`Target`].
///
/// `TargetResult` is the boundary between the client layer and the load
/// testing engine. Each request produces exactly one result, which the
/// engine can then use to update aggregate metrics.
///
/// The fields are protocol-agnostic where possible, allowing the same
/// structure to represent both HTTP and raw TCP operations.
pub struct TargetResult {
    /// Whether the request completed successfully.
    ///
    /// A request may be unsuccessful because of a transport or protocol
    /// error, or because the target returned an unexpected result.
    pub success: bool,
    /// Time spent executing the request.
    pub duration: Duration,
    /// HTTP response status code, when the target is HTTP.
    ///
    /// This is `None` for raw TCP targets or when no HTTP response was
    /// received.
    pub status_code: Option<u16>,
    /// Error description, when the request failed.
    pub error: Option<String>,
    /// Number of bytes sent to the target.
    pub bytes_sent: u64,
    /// Number of bytes received from the target.
    pub bytes_received: u64,
    /// Whether the configured response assertion succeeded.
    ///
    /// When no assertion is configured, this is considered successful.
    pub assertion_success: bool,
}

impl TargetResult {
    /// Creates a successful request result.
    ///
    /// The result starts with no HTTP status code or error and marks the
    /// response assertion as successful.
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

    /// Creates a failed request result.
    ///
    /// The supplied error message is stored in the result and all byte
    /// counters are initialized to zero.
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

/// A load-testing target backed by either HTTP or raw TCP.
///
/// `Target` provides a protocol-independent interface to the engine.
/// Workers can execute requests through [`Target::fire`] without needing
/// to know which transport is being used.
pub enum Target {
    /// HTTP target backed by a reusable `reqwest` client.
    Http {
        /// HTTP client used to execute requests.
        client: reqwest::Client,

        /// Target URL.
        url: String,

        /// HTTP method used for requests.
        method: reqwest::Method,

        /// Additional HTTP headers applied to requests.
        headers: Arc<Vec<String>>,

        /// Optional response body content that must be present for an
        /// assertion to succeed.
        expected_body: Option<Arc<String>>,
    },

    /// Raw TCP target backed by a pool of reusable connections.
    Tcp {
        /// Channel used to return connections to the TCP pool.
        pool_tx: Sender<TcpStream>,

        /// Channel used to acquire connections from the TCP pool.
        pool_rx: Receiver<TcpStream>,

        /// TCP address of the target.
        address: String,
    },
}

impl Target {
    /// Creates an HTTP target.
    ///
    /// The supplied client is reused across requests, allowing connection
    /// pooling and avoiding the overhead of creating a new HTTP client for
    /// every request.
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

    /// Creates a raw TCP target.
    ///
    /// The target establishes a pool of TCP connections that can be reused by
    /// workers during the load test. This avoids creating a new connection for
    /// every request and allows multiple workers to generate concurrent traffic.
    ///
    /// # Errors
    ///
    /// Returns an error if the initial connection pool cannot be established.
    pub async fn new_tcp(address: &str, workers: u32) -> Result<Self, String> {
        let (tx, rx) = async_channel::bounded(workers as usize);
        println!("🔌 Establishing a pool of {} TCP connections...", workers);
        for _ in 0..workers {
            match TcpStream::connect(address).await {
                Ok(stream) => {
                    let _ = tx.send(stream).await;
                }
                Err(e) => return Err(format!("Failed to connect: {}", e)),
            }
        }
        Ok(Self::Tcp {
            pool_tx: tx,
            pool_rx: rx,
            address: address.to_string(),
        })
    }

    async fn reconnect(
        pool_tx: &async_channel::Sender<TcpStream>,
        address: &str,
    ) -> Result<(), String> {
        let stream = TcpStream::connect(address)
            .await
            .map_err(|error| format!("Reconnect Error: {error}"))?;

        pool_tx
            .send(stream)
            .await
            .map_err(|error| format!("TCP Pool Error: {error}"))?;

        Ok(())
    }

    /// Executes a single request against the target.
    ///
    /// For HTTP targets, this sends one HTTP request using the configured
    /// method, headers, payload, and response assertion. For TCP targets, the
    /// payload is written to a pooled connection and the target response is
    /// read according to the TCP transport implementation.
    ///
    /// The returned [`TargetResult`] contains the outcome and measurements
    /// produced by this request.
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
                    if let Err(error) = stream.write_all(payload).await {
                        let reconnect_error = Self::reconnect(pool_tx, address).await.err();

                        let message = match reconnect_error {
                            Some(reconnect_error) => {
                                format!("Broken Pipe: {error}; {reconnect_error}")
                            }
                            None => format!("Broken Pipe: {error}"),
                        };

                        return TargetResult::fail(start.elapsed(), message);
                    }

                    let mut buffer = [0; 1];
                    if let Err(error) = stream.read_exact(&mut buffer).await {
                        let reconnect_error = Self::reconnect(pool_tx, address).await.err();

                        let message = match reconnect_error {
                            Some(reconnect_error) => {
                                format!("Connection Reset: {error}; {reconnect_error}")
                            }
                            None => format!("Connection Reset: {error}"),
                        };

                        return TargetResult::fail(start.elapsed(), message);
                    }

                    let _ = pool_tx.send(stream).await;

                    TargetResult::success(start.elapsed(), payload.len() as u64, 1)
                } else {
                    TargetResult::fail(start.elapsed(), "TCP Pool Exhausted".to_string())
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;

    async fn spawn_http_server(response: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();

            let mut request = vec![0u8; 4096];
            let _ = stream.read(&mut request).await;

            stream.write_all(response.as_bytes()).await.unwrap();
        });

        format!("http://{address}")
    }

    fn http_client() -> reqwest::Client {
        reqwest::Client::builder().build().unwrap()
    }

    #[tokio::test]
    async fn http_target_succeeds_on_2xx_response() {
        let url = spawn_http_server(
            "HTTP/1.1 200 OK\r\n\
             Content-Length: 5\r\n\
             \r\n\
             hello",
        )
        .await;

        let target = Target::new_http(
            http_client(),
            url,
            reqwest::Method::GET,
            Arc::new(Vec::new()),
            None,
        );

        let result = target.fire(b"").await;

        assert!(result.success);
        assert_eq!(result.status_code, Some(200));
        assert!(result.assertion_success);
        assert_eq!(result.bytes_sent, 0);
        assert_eq!(result.bytes_received, 5);
        assert!(result.error.is_none());
    }

    #[tokio::test]
    async fn http_target_rejects_non_2xx_response() {
        let url = spawn_http_server(
            "HTTP/1.1 500 Internal Server Error\r\n\
             Content-Length: 5\r\n\
             \r\n\
             error",
        )
        .await;

        let target = Target::new_http(
            http_client(),
            url,
            reqwest::Method::GET,
            Arc::new(Vec::new()),
            None,
        );

        let result = target.fire(b"").await;

        assert!(!result.success);
        assert_eq!(result.status_code, Some(500));
        assert!(result.assertion_success);
        assert_eq!(result.bytes_received, 5);
        assert!(result.error.is_none());
    }

    #[tokio::test]
    async fn http_target_accepts_matching_expected_body() {
        let url = spawn_http_server(
            "HTTP/1.1 200 OK\r\n\
             Content-Length: 11\r\n\
             \r\n\
             hello world",
        )
        .await;

        let target = Target::new_http(
            http_client(),
            url,
            reqwest::Method::GET,
            Arc::new(Vec::new()),
            Some(Arc::new("world".to_string())),
        );

        let result = target.fire(b"").await;

        assert!(result.success);
        assert_eq!(result.status_code, Some(200));
        assert!(result.assertion_success);
        assert!(result.error.is_none());
    }

    #[tokio::test]
    async fn http_target_rejects_mismatching_expected_body() {
        let url = spawn_http_server(
            "HTTP/1.1 200 OK\r\n\
             Content-Length: 5\r\n\
             \r\n\
             hello",
        )
        .await;

        let target = Target::new_http(
            http_client(),
            url,
            reqwest::Method::GET,
            Arc::new(Vec::new()),
            Some(Arc::new("world".to_string())),
        );

        let result = target.fire(b"").await;

        assert!(!result.success);
        assert_eq!(result.status_code, Some(200));
        assert!(!result.assertion_success);
        assert_eq!(result.error.as_deref(), Some("Mismatch: missing 'world'"));
    }

    #[tokio::test]
    async fn http_target_reports_network_errors() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();

        drop(listener);

        let target = Target::new_http(
            http_client(),
            format!("http://{address}"),
            reqwest::Method::GET,
            Arc::new(Vec::new()),
            None,
        );

        let result = target.fire(b"").await;

        assert!(!result.success);
        assert!(result.status_code.is_none());
        assert!(!result.assertion_success);
        assert!(result.error.is_some());
        assert!(result
            .error
            .as_deref()
            .unwrap()
            .starts_with("Network Error:"));
    }

    #[tokio::test]
    async fn tcp_target_succeeds_when_server_responds() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();

            let mut payload = [0u8; 4];
            stream.read_exact(&mut payload).await.unwrap();

            assert_eq!(&payload, b"ping");

            stream.write_all(&[1]).await.unwrap();
        });

        let target = Target::new_tcp(&address, 1).await.unwrap();

        let result = target.fire(b"ping").await;

        assert!(result.success);
        assert!(result.assertion_success);
        assert_eq!(result.status_code, None);
        assert_eq!(result.bytes_sent, 4);
        assert_eq!(result.bytes_received, 1);
        assert!(result.error.is_none());

        server.await.unwrap();
    }

    #[tokio::test]
    async fn tcp_target_rejects_initial_connection_failure() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();

        drop(listener);

        let result = Target::new_tcp(&address, 1).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn tcp_target_reconnects_after_read_failure() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();

        let server = tokio::spawn(async move {
            // First connection: receive the request and close the connection
            // without sending a response.
            let (mut first_stream, _) = listener.accept().await.unwrap();

            let mut payload = [0u8; 4];
            first_stream.read_exact(&mut payload).await.unwrap();

            assert_eq!(&payload, b"ping");

            drop(first_stream);

            // Second connection: this should be created by reconnect().
            let (mut second_stream, _) = listener.accept().await.unwrap();

            let mut payload = [0u8; 4];
            second_stream.read_exact(&mut payload).await.unwrap();

            assert_eq!(&payload, b"ping");

            second_stream.write_all(&[1]).await.unwrap();
        });

        let target = Target::new_tcp(&address, 1).await.unwrap();

        let first_result = target.fire(b"ping").await;

        assert!(!first_result.success);
        assert!(!first_result.assertion_success);
        assert!(first_result.error.is_some());

        let second_result = target.fire(b"ping").await;

        assert!(second_result.success);
        assert!(second_result.assertion_success);
        assert_eq!(second_result.bytes_sent, 4);
        assert_eq!(second_result.bytes_received, 1);
        assert!(second_result.error.is_none());

        server.await.unwrap();
    }
}
