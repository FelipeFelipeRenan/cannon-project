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
        None,
    )
    .await
    .expect("run_workers failed");

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
        None,
    )
    .await
    .expect("run_workers failed");

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
        None,
    )
    .await
    .expect("run_workers failed");

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

/// Starts an HTTP server that waits `delay` before answering every request.
async fn spawn_slow_server(listener: TcpListener, delay: Duration) -> tokio::task::JoinHandle<()> {
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

                tokio::time::sleep(delay).await;

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

fn http_target(address: std::net::SocketAddr) -> Arc<Target> {
    Arc::new(Target::new_http(
        reqwest::Client::new(),
        format!("http://{address}"),
        Method::GET,
        Arc::new(Vec::new()),
        None,
    ))
}

#[tokio::test]
async fn paced_requests_are_timed_from_their_scheduled_send_time() {
    // One worker, a 60 ms server and a request every 10 ms: every request
    // queues behind the previous ones. A client measuring only the service
    // time would report ~60 ms for all of them (coordinated omission).
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = spawn_slow_server(listener, Duration::from_millis(60)).await;

    let (results, _) = run_workers(
        6,
        1,
        None,
        Some(100),
        http_target(address),
        Arc::new(SharedMetrics::default()),
        None,
        Instant::now(),
        Duration::ZERO,
        None,
    )
    .await
    .unwrap();

    server.abort();

    let response_max = results[0].histogram.max();
    let service_max = results[0].service_histogram.max();

    assert!(
        service_max < 150_000,
        "service time should stay near 60 ms, was {service_max} us"
    );
    assert!(
        response_max > 2 * service_max,
        "response time ({response_max} us) must include queueing on top of the \
         service time ({service_max} us)"
    );
}

#[tokio::test]
async fn unpaced_runs_report_the_plain_service_time() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = spawn_slow_server(listener, Duration::from_millis(20)).await;

    let (results, _) = run_workers(
        4,
        1,
        None,
        None,
        http_target(address),
        Arc::new(SharedMetrics::default()),
        None,
        Instant::now(),
        Duration::ZERO,
        None,
    )
    .await
    .unwrap();

    server.abort();

    assert_eq!(
        results[0].histogram.max(),
        results[0].service_histogram.max()
    );
    assert_eq!(results[0].histogram.len(), 4);
}

#[tokio::test]
async fn failed_requests_are_not_recorded_in_latency_histograms() {
    // Bind then drop to obtain a port nothing is listening on.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);

    let metrics = Arc::new(SharedMetrics::default());

    let (results, _) = run_workers(
        3,
        1,
        None,
        None,
        http_target(address),
        metrics.clone(),
        None,
        Instant::now(),
        Duration::ZERO,
        None,
    )
    .await
    .unwrap();

    assert_eq!(metrics.failures.load(Ordering::Relaxed), 3);
    assert_eq!(metrics.successes.load(Ordering::Relaxed), 0);
    assert_eq!(
        results[0].histogram.len(),
        0,
        "fast failures must not skew latency"
    );
    assert_eq!(results[0].service_histogram.len(), 0);
}

#[tokio::test]
async fn cancel_stops_the_run_and_keeps_partial_results() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = spawn_slow_server(listener, Duration::from_millis(100)).await;

    let metrics = Arc::new(SharedMetrics::default());
    let started = Instant::now();

    let run = tokio::spawn(run_workers(
        1_000_000,
        2,
        None,
        None,
        http_target(address),
        metrics.clone(),
        None,
        Instant::now(),
        Duration::ZERO,
        None,
    ));

    tokio::time::sleep(Duration::from_millis(450)).await;
    metrics.cancel();

    let (results, _) = tokio::time::timeout(Duration::from_secs(3), run)
        .await
        .expect("run must stop promptly after cancel")
        .unwrap()
        .unwrap();

    server.abort();

    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(metrics.is_cancelled());

    let successes = metrics.successes.load(Ordering::Relaxed);

    assert!(successes > 0, "requests finished before cancel are kept");
    assert!(successes < 100, "the run stopped early, got {successes}");

    let recorded: u64 = results.iter().map(|result| result.histogram.len()).sum();

    assert_eq!(recorded, successes);
}

#[tokio::test]
async fn cancel_during_warmup_skips_the_measurement_phase() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = spawn_test_server(listener).await;

    let metrics = Arc::new(SharedMetrics::default());

    let run = tokio::spawn(run_workers(
        100,
        2,
        None,
        Some(50),
        http_target(address),
        metrics.clone(),
        None,
        Instant::now(),
        Duration::from_secs(30),
        None,
    ));

    tokio::time::sleep(Duration::from_millis(300)).await;
    metrics.cancel();

    let (results, duration) = tokio::time::timeout(Duration::from_secs(3), run)
        .await
        .expect("cancelling during warm-up must be immediate")
        .unwrap()
        .unwrap();

    server.abort();

    assert!(results.is_empty());
    assert_eq!(duration, Duration::ZERO);
    assert_eq!(metrics.measured_requests.load(Ordering::Relaxed), 0);
}
