use cannon::client::target::Target;
use cannon::engine::worker::{run_workers, SharedMetrics};
use reqwest::Method;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[tokio::test]
async fn status_code_is_counted_once() {
    // Start a minimal HTTP server on an ephemeral port.
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind test server");

    let address = listener
        .local_addr()
        .expect("failed to get test server address");

    let server = tokio::spawn(async move {
        let (mut socket, _) = listener
            .accept()
            .await
            .expect("failed to accept connection");

        let mut buffer = [0u8; 1024];

        socket
            .read(&mut buffer)
            .await
            .expect("failed to read HTTP request");

        socket
            .write_all(
                b"HTTP/1.1 200 OK\r\n\
                  Content-Length: 2\r\n\
                  Connection: close\r\n\
                  \r\n\
                  OK",
            )
            .await
            .expect("failed to write HTTP response");
    });

    let client = reqwest::Client::builder()
        .build()
        .expect("failed to build HTTP client");

    let target = Target::new_http(
        client,
        format!("http://{}", address),
        Method::GET,
        Arc::new(Vec::new()),
        None,
    );

    let shared_metrics = Arc::new(SharedMetrics::default());
    let start_time = Instant::now();

    let results = run_workers(
        1,
        1,
        None,
        None,
        Arc::new(target),
        shared_metrics.clone(),
        None,
        start_time,
        start_time,
    )
    .await;

    server.await.expect("test server task failed");

    assert_eq!(results.len(), 1);

    let result = &results[0];

    assert_eq!(
        result.status_counts.get(&200),
        Some(&1),
        "HTTP 200 must be counted exactly once"
    );

    assert_eq!(
        shared_metrics
            .successes
            .load(std::sync::atomic::Ordering::Relaxed),
        1,
        "exactly one request should be successful"
    );
}
