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

#[tokio::test]
async fn warmup_requests_are_excluded_from_metrics() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind test server");

    let address = listener
        .local_addr()
        .expect("failed to get test server address");

    let server = tokio::spawn(async move {
        for _ in 0..2 {
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
        }
    });

    let client = reqwest::Client::new();

    let target = Target::new_http(
        client,
        format!("http://{}", address),
        Method::GET,
        Arc::new(Vec::new()),
        None,
    );

    let shared_metrics = Arc::new(SharedMetrics::default());

    let start_time = Instant::now();

    // Warm-up will remain active for the entire test.
    let warmup_end = start_time + std::time::Duration::from_secs(60);

    let results = run_workers(
        2,
        1,
        None,
        None,
        Arc::new(target),
        shared_metrics.clone(),
        None,
        start_time,
        warmup_end,
    )
    .await;

    server.await.expect("test server task failed");

    assert_eq!(results.len(), 1);

    let result = &results[0];

    assert_eq!(
        shared_metrics
            .successes
            .load(std::sync::atomic::Ordering::Relaxed),
        0
    );

    assert_eq!(
        shared_metrics
            .failures
            .load(std::sync::atomic::Ordering::Relaxed),
        0
    );

    assert_eq!(
        shared_metrics
            .bytes_sent
            .load(std::sync::atomic::Ordering::Relaxed),
        0
    );

    assert_eq!(
        shared_metrics
            .bytes_received
            .load(std::sync::atomic::Ordering::Relaxed),
        0
    );

    assert!(result.status_counts.is_empty());
    assert!(result.error_counts.is_empty());
    assert_eq!(result.assertion_failures, 0);
    assert_eq!(result.histogram.len(), 0);
}

#[tokio::test]
async fn measured_requests_are_recorded() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind test server");

    let address = listener
        .local_addr()
        .expect("failed to get test server address");

    let server = tokio::spawn(async move {
        for _ in 0..1 {
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
        }
    });

    let client = reqwest::Client::new();

    let target = Target::new_http(
        client,
        format!("http://{}", address),
        Method::GET,
        Arc::new(Vec::new()),
        None,
    );

    let shared_metrics = Arc::new(SharedMetrics::default());

    let start_time = Instant::now();

    // Warm-up has already finished.
    let warmup_end = start_time;

    let results = run_workers(
        1,
        1,
        None,
        None,
        Arc::new(target),
        shared_metrics.clone(),
        None,
        start_time,
        warmup_end,
    )
    .await;

    server.await.expect("test server task failed");

    assert_eq!(results.len(), 1);

    let result = &results[0];

    assert_eq!(
        shared_metrics
            .successes
            .load(std::sync::atomic::Ordering::Relaxed),
        1
    );

    assert_eq!(
        shared_metrics
            .failures
            .load(std::sync::atomic::Ordering::Relaxed),
        0
    );

    assert_eq!(result.status_counts.get(&200), Some(&1));

    assert_eq!(result.histogram.len(), 1);
}
