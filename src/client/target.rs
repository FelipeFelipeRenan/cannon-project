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
    /// counters are initialized to zero. `assertion_success` stays `true`
    /// because no response was available to evaluate an assertion against.
    pub fn fail(duration: Duration, error: String) -> Self {
        Self {
            success: false,
            duration,
            status_code: None,
            error: Some(error),
            bytes_sent: 0,
            bytes_received: 0,
            // A transport failure says nothing about the response assertion,
            // so it must not be counted as an assertion failure.
            assertion_success: true,
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

        /// Additional HTTP headers, parsed once when the target is created.
        header_map: reqwest::header::HeaderMap,

        /// Optional response body content that must be present for an
        /// assertion to succeed.
        expected_body: Option<Arc<String>>,

        /// Status codes that count as success (`None`: any 2xx).
        accepted_status: Option<Arc<StatusFilter>>,
    },

    /// Raw TCP target backed by a pool of reusable connection slots.
    Tcp {
        /// Channel used to return connection slots to the pool.
        pool_tx: Sender<Option<TcpConn>>,

        /// Channel used to acquire connection slots from the pool.
        ///
        /// A slot is `None` when its connection broke; the next request that
        /// takes it opens a fresh one. Slots are always returned, so the pool
        /// never shrinks and a request can never wait for a connection that
        /// will not come back.
        pool_rx: Receiver<Option<TcpConn>>,

        /// TCP address of the target.
        address: String,

        /// Maximum time allowed to establish a connection.
        connect_timeout: Duration,

        /// Maximum time allowed for one write + response round trip.
        request_timeout: Duration,

        /// How the end of a response is recognised.
        read: TcpRead,
    },
}

/// How Cannon recognises the end of a response in raw TCP mode.
///
/// TCP has no message boundaries, so Cannon must be told how much to read
/// after each write. Reading too little leaves bytes in the socket that are
/// then mistaken for the *next* response, which makes latencies meaningless.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TcpRead {
    /// Read exactly this many bytes (`0` sends the payload and does not wait
    /// for any response).
    Bytes(usize),

    /// Read until the response ends with this delimiter (for example `\r\n`).
    Until(Vec<u8>),
}

impl Default for TcpRead {
    fn default() -> Self {
        Self::Bytes(1)
    }
}

/// Largest response accepted in [`TcpRead::Until`] mode.
const MAX_TCP_RESPONSE: usize = 1024 * 1024;

/// A pooled TCP connection.
pub struct TcpConn {
    stream: TcpStream,
    requests: u32,
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
        Self::new_http_with_status(client, url, method, headers, expected_body, None)
    }

    /// Creates an HTTP target with a custom definition of success.
    ///
    /// Responses whose status is accepted by `accepted_status` count as
    /// successful; with `None`, any 2xx response does.
    pub fn new_http_with_status(
        client: reqwest::Client,
        url: String,
        method: reqwest::Method,
        headers: Arc<Vec<String>>,
        expected_body: Option<Arc<String>>,
        accepted_status: Option<Arc<StatusFilter>>,
    ) -> Self {
        // Parsing and validating headers on every request is wasted work,
        // so it is done once. Malformed entries are skipped (the CLI already
        // rejects them before the run starts).
        let mut header_map = reqwest::header::HeaderMap::new();

        for header in headers.iter() {
            if let Some((name, value)) = header.split_once(':') {
                if let (Ok(name), Ok(value)) = (
                    reqwest::header::HeaderName::from_bytes(name.trim().as_bytes()),
                    reqwest::header::HeaderValue::from_str(value.trim()),
                ) {
                    header_map.append(name, value);
                }
            }
        }

        Self::Http {
            client,
            url,
            method,
            header_map,
            expected_body,
            accepted_status,
        }
    }

    /// Creates a raw TCP target using the default timeouts (5 s to connect,
    /// 30 s per request) that reads one byte per response.
    ///
    /// See [`Target::new_tcp_with_options`] to configure them.
    ///
    /// # Errors
    ///
    /// Returns an error if the initial connection pool cannot be established.
    pub async fn new_tcp(address: &str, workers: u32) -> Result<Self, String> {
        Self::new_tcp_with_timeouts(
            address,
            workers,
            Duration::from_millis(5_000),
            Duration::from_millis(30_000),
        )
        .await
    }

    /// Creates a raw TCP target with explicit timeouts that reads one byte
    /// per response.
    ///
    /// # Errors
    ///
    /// Returns an error if the initial connection pool cannot be established.
    pub async fn new_tcp_with_timeouts(
        address: &str,
        workers: u32,
        connect_timeout: Duration,
        request_timeout: Duration,
    ) -> Result<Self, String> {
        Self::new_tcp_with_options(
            address,
            workers,
            connect_timeout,
            request_timeout,
            TcpRead::default(),
        )
        .await
    }

    /// Creates a raw TCP target.
    ///
    /// The target establishes a pool of `workers` TCP connections that are
    /// reused for every request. `connect_timeout` bounds every connection
    /// attempt, `request_timeout` bounds each write plus response, and `read`
    /// says how a response is delimited.
    ///
    /// If a connection breaks, the request that noticed fails and the next
    /// one transparently opens a new connection, so a target that goes away
    /// produces fast, explicit failures instead of a hung run.
    ///
    /// # Errors
    ///
    /// Returns an error if the initial connection pool cannot be established.
    pub async fn new_tcp_with_options(
        address: &str,
        workers: u32,
        connect_timeout: Duration,
        request_timeout: Duration,
        read: TcpRead,
    ) -> Result<Self, String> {
        let (tx, rx) = async_channel::bounded(workers.max(1) as usize);
        println!("🔌 Establishing a pool of {} TCP connections...", workers);
        for _ in 0..workers {
            let stream = Self::connect(address, connect_timeout)
                .await
                .map_err(|error| format!("Failed to connect: {error}"))?;

            let _ = tx
                .send(Some(TcpConn {
                    stream,
                    requests: 0,
                }))
                .await;
        }
        Ok(Self::Tcp {
            pool_tx: tx,
            pool_rx: rx,
            address: address.to_string(),
            connect_timeout,
            request_timeout,
            read,
        })
    }

    async fn connect(address: &str, connect_timeout: Duration) -> Result<TcpStream, String> {
        match tokio::time::timeout(connect_timeout, TcpStream::connect(address)).await {
            Ok(Ok(stream)) => Ok(stream),
            Ok(Err(error)) => Err(error.to_string()),
            Err(_) => Err(format!("connection timed out after {connect_timeout:?}")),
        }
    }

    /// Writes `payload` and reads one response; returns the bytes received.
    async fn tcp_round_trip(
        conn: &mut TcpConn,
        payload: &[u8],
        read: &TcpRead,
    ) -> Result<u64, String> {
        conn.stream
            .write_all(payload)
            .await
            .map_err(|error| format!("Broken Pipe: {error}"))?;

        let received = match read {
            TcpRead::Bytes(0) => 0,
            TcpRead::Bytes(count) => {
                let mut buffer = [0u8; 256];

                if *count <= buffer.len() {
                    conn.stream.read_exact(&mut buffer[..*count]).await
                } else {
                    conn.stream.read_exact(&mut vec![0u8; *count]).await
                }
                .map_err(|error| format!("Connection Reset: {error}"))?;

                *count as u64
            }
            TcpRead::Until(delimiter) => {
                let mut response: Vec<u8> = Vec::with_capacity(64);
                let mut chunk = [0u8; 1024];

                while !response.ends_with(delimiter) {
                    let read_now = conn
                        .stream
                        .read(&mut chunk)
                        .await
                        .map_err(|error| format!("Connection Reset: {error}"))?;

                    if read_now == 0 {
                        return Err("Connection Closed before the response delimiter".to_string());
                    }

                    response.extend_from_slice(&chunk[..read_now]);

                    if response.len() > MAX_TCP_RESPONSE {
                        return Err(
                            "Response Framing: no delimiter found in the first 1 MiB".to_string()
                        );
                    }
                }

                response.len() as u64
            }
        };

        // Early on, check that the response did not contain more than was
        // read: leftover bytes would be taken for the next response and make
        // every following latency look instantaneous.
        if conn.requests < 3 && !matches!(read, TcpRead::Bytes(0)) {
            let mut extra = [0u8; 1];

            if matches!(conn.stream.try_read(&mut extra), Ok(count) if count > 0) {
                return Err(
                    "Response Framing: the server sent more data than Cannon read; \
                            set --tcp-read-bytes or --tcp-read-until to match the protocol"
                        .to_string(),
                );
            }
        }

        Ok(received)
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
        self.execute(payload, None).await
    }

    /// Executes a request whose payload never changes.
    ///
    /// For HTTP the body is shared by reference count instead of being
    /// copied for every request.
    #[inline(always)]
    pub async fn fire_shared(&self, payload: &bytes::Bytes) -> TargetResult {
        self.execute(payload, Some(payload)).await
    }

    async fn execute(&self, payload: &[u8], shared: Option<&bytes::Bytes>) -> TargetResult {
        let start = std::time::Instant::now();

        match self {
            Target::Http {
                client,
                url,
                method,
                header_map,
                expected_body,
                accepted_status,
            } => {
                let mut req = client.request(method.clone(), url);
                if !payload.is_empty() {
                    req = match shared {
                        Some(body) => req.body(body.clone()),
                        None => req.body(payload.to_vec()),
                    };
                }
                if !header_map.is_empty() {
                    req = req.headers(header_map.clone());
                }

                match req.send().await {
                    Ok(resp) => {
                        let status = resp.status().as_u16();
                        let (error_msg, bytes_recv, assert_ok) = match resp.bytes().await {
                            Ok(bytes) => {
                                let mut err = None;
                                let mut ok = true;
                                if let Some(expected) = expected_body {
                                    let text = String::from_utf8_lossy(&bytes);

                                    if !text.contains(expected.as_ref().as_str()) {
                                        err = Some(format!("Mismatch: missing '{}'", expected));
                                        ok = false;
                                    }
                                }
                                (err, bytes.len() as u64, ok)
                            }
                            Err(e) => (
                                Some(format!("Read Error: {}", e)),
                                0,
                                expected_body.is_none(),
                            ),
                        };
                        TargetResult {
                            duration: start.elapsed(),
                            status_code: Some(status),
                            error: error_msg,
                            bytes_sent: payload.len() as u64,
                            bytes_received: bytes_recv,
                            success: accepted_status
                                .as_ref()
                                .map_or((200..300).contains(&status), |filter| {
                                    filter.accepts(status)
                                })
                                && assert_ok,
                            assertion_success: assert_ok,
                        }
                    }
                    Err(e) => TargetResult::fail(start.elapsed(), classify_http_error(&e)),
                }
            }

            Target::Tcp {
                pool_tx,
                pool_rx,
                address,
                connect_timeout,
                request_timeout,
                read,
            } => {
                let Ok(slot) = pool_rx.recv().await else {
                    return TargetResult::fail(start.elapsed(), "TCP Pool Closed".to_string());
                };

                // Every path below hands a slot back to the pool, so waiting
                // for one can never block forever.
                let mut conn = match slot {
                    Some(conn) => conn,
                    None => match Self::connect(address, *connect_timeout).await {
                        Ok(stream) => TcpConn {
                            stream,
                            requests: 0,
                        },
                        Err(error) => {
                            let _ = pool_tx.send(None).await;

                            return TargetResult::fail(
                                start.elapsed(),
                                format!("Connect Error: {error}"),
                            );
                        }
                    },
                };

                let round_trip = Self::tcp_round_trip(&mut conn, payload, read);

                let outcome = match tokio::time::timeout(*request_timeout, round_trip).await {
                    Ok(result) => result,
                    Err(_) => Err(format!("Timeout: no response within {request_timeout:?}")),
                };

                match outcome {
                    Ok(received) => {
                        conn.requests = conn.requests.saturating_add(1);

                        let _ = pool_tx.send(Some(conn)).await;

                        TargetResult::success(start.elapsed(), payload.len() as u64, received)
                    }
                    Err(message) => {
                        // The connection is in an unknown state: drop it and
                        // let the next request open a fresh one.
                        drop(conn);

                        let _ = pool_tx.send(None).await;

                        TargetResult::fail(start.elapsed(), message)
                    }
                }
            }
        }
    }
}

/// Set of HTTP status codes that count as a successful response.
///
/// Parsed from a comma-separated list of exact codes (`404`) and classes
/// (`2xx`), for example `200,301,4xx`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusFilter {
    ranges: Vec<(u16, u16)>,
}

impl StatusFilter {
    /// Parses a specification such as `200,404` or `2xx,3xx`.
    ///
    /// # Errors
    ///
    /// Returns a description of the problem for an empty list, an unknown
    /// class or a code outside `100..=599`.
    pub fn parse(spec: &str) -> Result<Self, String> {
        let mut ranges = Vec::new();

        for part in spec
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
        {
            let lower = part.to_ascii_lowercase();

            let range = if let Some(class) = lower.strip_suffix("xx") {
                match class.parse::<u16>() {
                    Ok(digit @ 1..=5) => (digit * 100, digit * 100 + 99),
                    _ => return Err(format!("invalid status class '{part}' (use 1xx to 5xx)")),
                }
            } else {
                match lower.parse::<u16>() {
                    Ok(code @ 100..=599) => (code, code),
                    _ => return Err(format!("invalid status code '{part}' (use 100-599)")),
                }
            };

            ranges.push(range);
        }

        if ranges.is_empty() {
            return Err("expect-status must list at least one status code".to_string());
        }

        Ok(Self { ranges })
    }

    /// Returns `true` when `status` counts as success.
    pub fn accepts(&self, status: u16) -> bool {
        self.ranges
            .iter()
            .any(|&(low, high)| (low..=high).contains(&status))
    }
}

/// Turns a `reqwest` error into a short, actionable category.
///
/// `reqwest` reports timeouts, DNS failures, refused connections, TLS errors
/// and exhausted file descriptors all as "error sending request", which hides
/// the real cause. The categories are stable strings so they aggregate well
/// in the error breakdown.
fn classify_http_error(error: &reqwest::Error) -> String {
    let mut chain = Vec::new();
    let mut os_error = None;
    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(error);

    while let Some(current) = source {
        chain.push(current.to_string());

        if let Some(io) = current.downcast_ref::<std::io::Error>() {
            os_error = os_error.or(io.raw_os_error());
        }

        source = current.source();
    }

    describe_http_error(
        error.is_timeout(),
        error.is_builder(),
        error.is_redirect(),
        error.is_connect(),
        os_error,
        &chain.join(": "),
    )
}

fn describe_http_error(
    timeout: bool,
    builder: bool,
    redirect: bool,
    connect: bool,
    os_error: Option<i32>,
    chain: &str,
) -> String {
    let lower = chain.to_lowercase();

    // EMFILE (24) / ENFILE (23) on Unix: the process ran out of descriptors.
    // (On Windows those numbers mean something else, so only the text counts.)
    if (cfg!(unix) && matches!(os_error, Some(23 | 24))) || lower.contains("too many open files") {
        return "File Descriptor Limit: too many open files (raise `ulimit -n` or use fewer workers)"
            .to_string();
    }

    let category = if timeout {
        "Timeout"
    } else if builder {
        "Invalid Request"
    } else if redirect {
        "Redirect Error"
    } else if lower.contains("dns error")
        || lower.contains("failed to lookup")
        || lower.contains("name or service not known")
        || lower.contains("no such host")
    {
        "DNS Error"
    } else if lower.contains("certificate")
        || lower.contains("tls")
        || lower.contains("ssl")
        || lower.contains("handshake")
    {
        "TLS Error"
    } else if lower.contains("connection refused")
        || lower.contains("actively refused")
        // ECONNREFUSED on Linux, macOS and Windows (WSAECONNREFUSED).
        || matches!(os_error, Some(111 | 61 | 10061))
    {
        "Connection Refused"
    } else if lower.contains("connection reset") {
        "Connection Reset"
    } else if connect {
        "Connect Error"
    } else {
        "Network Error"
    };

    format!("{category}: {chain}")
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
        assert!(
            result.assertion_success,
            "a network error is not an assertion failure"
        );
        assert!(result.error.is_some());
        assert!(result
            .error
            .as_deref()
            .unwrap()
            .starts_with("Connection Refused:"));
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
        assert!(first_result.assertion_success);
        assert!(first_result.error.is_some());

        let second_result = target.fire(b"ping").await;

        assert!(second_result.success);
        assert!(second_result.assertion_success);
        assert_eq!(second_result.bytes_sent, 4);
        assert_eq!(second_result.bytes_received, 1);
        assert!(second_result.error.is_none());

        server.await.unwrap();
    }

    #[tokio::test]
    async fn tcp_request_times_out_when_server_never_replies() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();

        // Accept connections but never answer.
        tokio::spawn(async move {
            let mut held = Vec::new();

            while let Ok((stream, _)) = listener.accept().await {
                held.push(stream);
            }
        });

        let target = Target::new_tcp_with_timeouts(
            &address,
            1,
            Duration::from_millis(500),
            Duration::from_millis(150),
        )
        .await
        .unwrap();

        let started = std::time::Instant::now();
        let result = target.fire(b"x").await;

        assert!(!result.success);
        assert!(result.error.unwrap().contains("Timeout"));
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "request must not hang past its timeout"
        );
    }

    #[tokio::test]
    async fn tcp_connect_fails_fast_for_unreachable_port() {
        // Bind then drop to obtain a port nothing is listening on.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        drop(listener);

        let result = Target::new_tcp_with_timeouts(
            &address,
            1,
            Duration::from_millis(500),
            Duration::from_millis(500),
        )
        .await;

        assert!(result.is_err());
    }

    async fn serve_once(reply: &'static [u8]) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buffer = [0u8; 16];

            while let Ok(count) = stream.read(&mut buffer).await {
                if count == 0 {
                    break;
                }

                stream.write_all(reply).await.unwrap();
            }
        });

        (address, server)
    }

    async fn tcp_with(address: &str, read: TcpRead) -> Target {
        Target::new_tcp_with_options(
            address,
            1,
            Duration::from_millis(500),
            Duration::from_secs(2),
            read,
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn tcp_reads_a_fixed_number_of_bytes() {
        let (address, _server) = serve_once(b"12345678").await;
        let target = tcp_with(&address, TcpRead::Bytes(8)).await;

        for _ in 0..5 {
            let result = target.fire(b"ping").await;

            assert!(result.success, "{:?}", result.error);
            assert_eq!(result.bytes_received, 8);
        }
    }

    #[tokio::test]
    async fn tcp_reads_until_a_delimiter() {
        let (address, _server) = serve_once(b"+OK\r\n").await;
        let target = tcp_with(&address, TcpRead::Until(b"\r\n".to_vec())).await;

        let result = target.fire(b"ping").await;

        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.bytes_received, 5);
    }

    #[tokio::test]
    async fn tcp_without_response_does_not_wait() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();

        tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.unwrap();

            tokio::time::sleep(Duration::from_secs(5)).await;
        });

        let target = tcp_with(&address, TcpRead::Bytes(0)).await;
        let result = target.fire(b"ping").await;

        assert!(result.success);
        assert_eq!(result.bytes_received, 0);
    }

    #[tokio::test]
    async fn tcp_detects_a_response_longer_than_the_framing() {
        let (address, _server) = serve_once(b"12345678").await;
        let target = tcp_with(&address, TcpRead::Bytes(1)).await;

        // Give the whole reply time to arrive before the check.
        let result = target.fire(b"ping").await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        let second = target.fire(b"ping").await;

        assert!(
            !result.success || !second.success,
            "framing mismatch must be reported"
        );
    }

    #[tokio::test]
    async fn tcp_fails_fast_forever_after_the_server_dies() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();

            drop(stream);
            // The listener is dropped here too, so reconnecting is refused.
        });

        let target = tcp_with(&address, TcpRead::Bytes(1)).await;

        server.await.unwrap();

        // More requests than connections: none of them may hang. (Windows
        // can take close to the connect timeout to refuse a connection, so
        // the count is kept small and the bound generous.)
        let all = tokio::time::timeout(Duration::from_secs(30), async {
            for _ in 0..5 {
                assert!(!target.fire(b"ping").await.success);
            }
        })
        .await;

        assert!(all.is_ok(), "requests hung after the server died");
    }

    #[test]
    fn http_errors_are_classified_by_cause() {
        let describe = |timeout, builder, connect, os, text: &str| {
            describe_http_error(timeout, builder, false, connect, os, text)
        };

        assert!(describe(true, false, false, None, "x").starts_with("Timeout:"));
        assert!(describe(false, true, false, None, "builder error").starts_with("Invalid Request"));
        assert!(describe(
            false,
            false,
            true,
            None,
            "dns error: failed to lookup address"
        )
        .starts_with("DNS Error"));
        assert!(describe(
            false,
            false,
            true,
            None,
            "invalid peer certificate: UnknownIssuer"
        )
        .starts_with("TLS Error"));
        assert!(describe(
            false,
            false,
            true,
            None,
            "tcp connect error: Connection refused (os error 111)"
        )
        .starts_with("Connection Refused"));
        // errno 24 is EMFILE only on Unix; elsewhere it must not be guessed.
        let by_code = describe(false, false, true, Some(24), "tcp connect error");

        if cfg!(unix) {
            assert!(by_code.starts_with("File Descriptor Limit"));
        } else {
            assert!(by_code.starts_with("Connect Error"));
        }

        assert!(describe(
            false,
            false,
            true,
            None,
            "Too many open files (os error 24)"
        )
        .starts_with("File Descriptor Limit"));
        assert!(describe(false, false, false, None, "boom").starts_with("Network Error"));
    }

    #[test]
    fn status_filter_accepts_codes_and_classes() {
        let filter = StatusFilter::parse("200, 404,3xx").unwrap();

        assert!(filter.accepts(200) && filter.accepts(404) && filter.accepts(301));
        assert!(!filter.accepts(201) && !filter.accepts(500));

        assert!(StatusFilter::parse("").is_err());
        assert!(StatusFilter::parse("abc").is_err());
        assert!(StatusFilter::parse("99").is_err());
        assert!(StatusFilter::parse("6xx").is_err());
    }
}
