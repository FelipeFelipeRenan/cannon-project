use cannon::client::target::Target;
use cannon::engine::worker::{run_workers, SharedMetrics};
use reqwest::Method;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn spawn_test_server(listener: TcpListener) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };

            tokio::spawn(async move {
                let mut buffer = [0u8; 1024];

                if socket.read(&mut buffer).await.is_err() {
                    return;
                }

                let _ = socket
                    .write_all(
                        b"HTTP/1.1 200 OK\r\n\
                          Content-Length: 2\r\n\
                          Connection: close\r\n\
                          \r\n\
                          OK",
                    )
                    .await;
            });
        }
    })
}

#[tokio::test]
async fn status_code_is_counted_once() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind test server");

    let address = listener
        .local_addr()
        .expect("failed to get test server address");

    let server = spawn_test_server(listener).await;

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

    let (results, _) = run_workers(
        1,
        1,
        None,
        None,
        Arc::new(target),
        shared_metrics.clone(),
        None,
        start_time,
        Duration::ZERO,
    )
    .await;

    server.abort();

    assert_eq!(results.len(), 1);

    let result = &results[0];

    assert_eq!(result.status_counts.get(&200), Some(&1));

    assert_eq!(shared_metrics.successes.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn warmup_requests_are_excluded_from_metrics() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind test server");

    let address = listener
        .local_addr()
        .expect("failed to get test server address");

    let server = spawn_test_server(listener).await;

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

    let (results, measurement_duration) = run_workers(
        2,
        2,
        None,
        None,
        Arc::new(target),
        shared_metrics.clone(),
        None,
        start_time,
        Duration::from_millis(50),
    )
    .await;

    server.abort();

    assert_eq!(results.len(), 2);

    assert!(
        measurement_duration > Duration::ZERO,
        "measurement duration must be greater than zero"
    );

    assert_eq!(
        shared_metrics.measured_requests.load(Ordering::Relaxed),
        2,
        "warm-up requests must not be included in measured requests"
    );

    assert_eq!(
        shared_metrics.successes.load(Ordering::Relaxed),
        2,
        "only measured requests should contribute to successes"
    );

    assert_eq!(shared_metrics.failures.load(Ordering::Relaxed), 0);

    let total_status_count: u64 = results
        .iter()
        .map(|result| result.status_counts.get(&200).copied().unwrap_or(0))
        .sum();

    assert_eq!(
        total_status_count, 2,
        "only measured requests should contribute status codes"
    );

    let total_histogram_count: u64 = results.iter().map(|result| result.histogram.len()).sum();

    assert_eq!(
        total_histogram_count, 2,
        "only measured requests should be recorded in histograms"
    );

    let total_assertion_failures: u64 =
        results.iter().map(|result| result.assertion_failures).sum();

    assert_eq!(total_assertion_failures, 0);
}

#[tokio::test]
async fn measured_requests_are_recorded() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind test server");

    let address = listener
        .local_addr()
        .expect("failed to get test server address");

    let server = spawn_test_server(listener).await;

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

    let (results, measurement_duration) = run_workers(
        1,
        1,
        None,
        None,
        Arc::new(target),
        shared_metrics.clone(),
        None,
        start_time,
        Duration::ZERO,
    )
    .await;

    server.abort();

    assert_eq!(results.len(), 1);

    assert!(
        measurement_duration > Duration::ZERO,
        "measurement duration must be greater than zero"
    );

    assert_eq!(shared_metrics.successes.load(Ordering::Relaxed), 1);

    assert_eq!(shared_metrics.failures.load(Ordering::Relaxed), 0);

    assert_eq!(shared_metrics.measured_requests.load(Ordering::Relaxed), 1);

    assert_eq!(results[0].status_counts.get(&200), Some(&1));

    assert_eq!(results[0].histogram.len(), 1);
}
