//! End-to-end tests: run the real `cannon` binary against local servers.
//!
//! The servers are tiny `std::net` implementations so the tests need no
//! extra dependencies and no network access beyond loopback.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const CANNON: &str = env!("CARGO_BIN_EXE_cannon");

/// A minimal HTTP/1.1 server that answers every request with `200 OK`.
struct HttpServer {
    address: String,
    requests: Arc<AtomicU64>,
}

impl HttpServer {
    fn start(body: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let requests = Arc::new(AtomicU64::new(0));
        let counter = requests.clone();

        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let counter = counter.clone();

                thread::spawn(move || serve_http_connection(stream, body, counter));
            }
        });

        Self { address, requests }
    }

    fn url(&self) -> String {
        format!("http://{}/", self.address)
    }

    fn requests(&self) -> u64 {
        self.requests.load(Ordering::SeqCst)
    }
}

fn serve_http_connection(mut stream: TcpStream, body: &str, counter: Arc<AtomicU64>) {
    let mut buffer = [0u8; 8192];
    let mut pending = Vec::new();

    loop {
        let read = match stream.read(&mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(read) => read,
        };

        pending.extend_from_slice(&buffer[..read]);

        // Answer once per complete request head (these tests send no bodies
        // large enough to matter).
        while let Some(end) = find_head_end(&pending) {
            pending.drain(..end);
            counter.fetch_add(1, Ordering::SeqCst);

            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\n\r\n{}",
                body.len(),
                body
            );

            if stream.write_all(response.as_bytes()).is_err() {
                return;
            }
        }
    }
}

fn find_head_end(data: &[u8]) -> Option<usize> {
    data.windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|position| position + 4)
}

/// A TCP server that echoes one byte for every byte received.
fn start_tcp_echo_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();

    thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            thread::spawn(move || {
                let mut byte = [0u8; 1];

                while stream.read_exact(&mut byte).is_ok() {
                    if stream.write_all(&[1u8]).is_err() {
                        return;
                    }
                }
            });
        }
    });

    address
}

/// A TCP server that answers every received chunk with `reply`, after `delay`.
fn start_tcp_reply_server(reply: &'static [u8], delay: Duration) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();

    thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            thread::spawn(move || {
                let mut buffer = [0u8; 64];

                while let Ok(count) = stream.read(&mut buffer) {
                    if count == 0 {
                        return;
                    }

                    thread::sleep(delay);

                    if stream.write_all(reply).is_err() {
                        return;
                    }
                }
            });
        }
    });

    address
}

/// A TCP server that serves a few requests and then disappears entirely.
fn start_dying_tcp_server(requests_before_death: usize) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();

    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buffer = [0u8; 64];

        for _ in 0..requests_before_death {
            if stream.read(&mut buffer).unwrap_or(0) == 0 {
                return;
            }

            let _ = stream.write_all(&[1u8]);
        }

        // Dropping the stream and the listener makes every reconnect fail.
    });

    address
}

/// A TCP server that accepts connections and never answers.
fn start_silent_tcp_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();

    thread::spawn(move || {
        let mut held = Vec::new();

        for stream in listener.incoming().flatten() {
            held.push(stream);
        }
    });

    address
}

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("cannon-e2e-{}-{name}", std::process::id()))
}

fn run_cannon(args: &[&str]) -> Output {
    Command::new(CANNON)
        .args(args)
        .env("NO_COLOR", "1")
        .output()
        .expect("failed to run cannon")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn read_json(path: &PathBuf) -> serde_json::Value {
    let content = std::fs::read_to_string(path).expect("report file must exist");

    serde_json::from_str(&content).expect("report must be valid JSON")
}

#[test]
fn http_run_completes_every_request_and_writes_json_report() {
    let server = HttpServer::start(r#"{"status":"ok"}"#);
    let report = temp_path("http-basic.json");

    let output = run_cannon(&[
        "-u",
        &server.url(),
        "-c",
        "200",
        "-w",
        "8",
        "-o",
        report.to_str().unwrap(),
    ]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));

    let json = read_json(&report);

    assert_eq!(json["successes"], 200);
    assert_eq!(json["failures"], 0);
    assert_eq!(json["total_requests"], 200);
    assert_eq!(json["status_codes"]["200"], 200);
    assert_eq!(server.requests(), 200);
    assert!(stdout(&output).contains("Apdex"));

    let _ = std::fs::remove_file(report);
}

#[test]
fn response_assertion_passes_when_body_matches() {
    let server = HttpServer::start(r#"{"status":"ok"}"#);

    let output = run_cannon(&[
        "-u",
        &server.url(),
        "-c",
        "20",
        "--expect",
        r#""status":"ok""#,
    ]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("Failures: 0"));
}

#[test]
fn response_assertion_fails_requests_when_body_does_not_match() {
    let server = HttpServer::start(r#"{"status":"ok"}"#);
    let report = temp_path("assertion.json");

    let output = run_cannon(&[
        "-u",
        &server.url(),
        "-c",
        "20",
        "--expect",
        "definitely-not-in-the-body",
        "-o",
        report.to_str().unwrap(),
    ]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));

    let json = read_json(&report);

    assert_eq!(json["successes"], 0);
    assert_eq!(json["failures"], 20);

    let _ = std::fs::remove_file(report);
}

#[test]
fn warmup_traffic_is_excluded_from_the_measurements() {
    let server = HttpServer::start("ok");
    let report = temp_path("warmup.json");

    let output = run_cannon(&[
        "-u",
        &server.url(),
        "-c",
        "30",
        "-w",
        "2",
        "--rps",
        "50",
        "--warmup",
        "1",
        "-o",
        report.to_str().unwrap(),
    ]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));

    let json = read_json(&report);

    assert_eq!(json["successes"], 30, "only measured requests are reported");
    assert!(
        server.requests() > 30,
        "warm-up must generate real traffic (server saw {})",
        server.requests()
    );

    let _ = std::fs::remove_file(report);
}

#[test]
fn constant_rps_paces_the_requests() {
    let server = HttpServer::start("ok");
    let started = Instant::now();

    let output = run_cannon(&["-u", &server.url(), "-c", "30", "-w", "4", "--rps", "50"]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));

    // 30 requests at 50 rps take about 0.6 s; unthrottled they take a few ms.
    assert!(
        started.elapsed() >= Duration::from_millis(450),
        "run finished too quickly for --rps 50: {:?}",
        started.elapsed()
    );
}

#[test]
fn ramp_up_flag_works_end_to_end() {
    let server = HttpServer::start("ok");
    let report = temp_path("ramp.json");

    let output = run_cannon(&[
        "-u",
        &server.url(),
        "-c",
        "100",
        "-w",
        "8",
        "--rps",
        "200",
        "--ramp-up",
        "1s",
        "-o",
        report.to_str().unwrap(),
    ]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));

    let json = read_json(&report);

    assert_eq!(json["successes"], 100);
    // A linear ramp to 200 rps over 1 s covers ~100 requests in ~1 s; a flat
    // 200 rps would need only ~0.5 s.
    assert!(
        json["duration_secs"].as_f64().unwrap() > 0.8,
        "ramp-up did not slow the start: {}",
        json["duration_secs"]
    );

    let _ = std::fs::remove_file(report);
}

#[test]
fn yaml_config_is_applied_and_cli_overrides_it() {
    let server = HttpServer::start("ok");
    let config = temp_path("config.yml");
    let report = temp_path("config.json");

    std::fs::write(
        &config,
        format!("url: \"{}\"\ncount: 500\nworkers: 4\n", server.url()),
    )
    .unwrap();

    let output = run_cannon(&[
        "-f",
        config.to_str().unwrap(),
        "-c",
        "40",
        "-o",
        report.to_str().unwrap(),
    ]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));

    let json = read_json(&report);

    assert_eq!(
        json["successes"], 40,
        "CLI --count overrides the YAML count"
    );
    assert_eq!(json["concurrency"], 4, "workers come from the YAML file");

    let _ = std::fs::remove_file(config);
    let _ = std::fs::remove_file(report);
}

#[test]
fn csv_export_has_a_header_and_one_row_per_request() {
    let server = HttpServer::start("ok");
    let csv = temp_path("rows.csv");

    let output = run_cannon(&[
        "-u",
        &server.url(),
        "-c",
        "25",
        "--csv",
        csv.to_str().unwrap(),
    ]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));

    let content = std::fs::read_to_string(&csv).unwrap();
    let mut lines = content.lines();

    assert_eq!(
        lines.next(),
        Some("relative_time_ms,status,latency_ms,error,service_ms")
    );
    assert_eq!(lines.count(), 25);

    let _ = std::fs::remove_file(csv);
}

#[test]
fn html_report_embeds_the_results() {
    let server = HttpServer::start("ok");
    let html = temp_path("report.html");

    let output = run_cannon(&[
        "-u",
        &server.url(),
        "-c",
        "10",
        "--html",
        html.to_str().unwrap(),
    ]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));

    let content = std::fs::read_to_string(&html).unwrap();

    assert!(content.contains("<html"));
    assert!(
        !content.contains("/*JSON_PAYLOAD*/"),
        "payload not injected"
    );

    let _ = std::fs::remove_file(html);
}

#[test]
fn baseline_comparison_fails_on_regression_and_passes_otherwise() {
    let server = HttpServer::start("ok");
    let baseline = temp_path("baseline.json");

    // Save a baseline from a real run.
    let saved = run_cannon(&[
        "-u",
        &server.url(),
        "-c",
        "50",
        "--save-baseline",
        baseline.to_str().unwrap(),
    ]);
    assert!(saved.status.success(), "stderr: {}", stderr(&saved));
    assert!(baseline.exists());

    // An absurdly tight baseline must be reported as a regression.
    std::fs::write(&baseline, r#"{"p99_ms": 0.000001, "rps": 1.0}"#).unwrap();

    let regressed = run_cannon(&[
        "-u",
        &server.url(),
        "-c",
        "50",
        "--compare-baseline",
        baseline.to_str().unwrap(),
        "--tolerance",
        "5",
    ]);
    assert_eq!(
        regressed.status.code(),
        Some(2),
        "gate failures exit with 2"
    );
    assert!(stdout(&regressed).contains("Regression detected"));

    // A very generous baseline passes.
    std::fs::write(&baseline, r#"{"p99_ms": 100000.0, "rps": 1.0}"#).unwrap();

    let improved = run_cannon(&[
        "-u",
        &server.url(),
        "-c",
        "50",
        "--compare-baseline",
        baseline.to_str().unwrap(),
    ]);
    assert!(improved.status.success(), "stderr: {}", stderr(&improved));

    let _ = std::fs::remove_file(baseline);
}

#[test]
fn tcp_mode_round_trips_a_binary_payload() {
    let address = start_tcp_echo_server();
    let report = temp_path("tcp.json");

    let output = run_cannon(&[
        "--mode",
        "tcp",
        "-u",
        &address,
        "-c",
        "100",
        "-w",
        "4",
        "--body",
        "{{value:1:u8}}",
        "-o",
        report.to_str().unwrap(),
    ]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));

    let json = read_json(&report);

    assert_eq!(json["successes"], 100);
    assert_eq!(json["failures"], 0);

    let _ = std::fs::remove_file(report);
}

#[test]
fn tcp_read_bytes_measures_the_full_multi_byte_response() {
    // The server needs ~50 ms to produce an 8-byte reply. Reading only one
    // byte used to report ~0 ms for the following requests.
    let address = start_tcp_reply_server(b"12345678", Duration::from_millis(50));
    let report = temp_path("tcp-multibyte.json");

    let output = run_cannon(&[
        "--mode",
        "tcp",
        "-u",
        &address,
        "-c",
        "20",
        "-w",
        "1",
        "--body",
        "ping",
        "--tcp-read-bytes",
        "8",
        "-o",
        report.to_str().unwrap(),
    ]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));

    let json = read_json(&report);

    assert_eq!(json["successes"], 20);
    assert_eq!(json["bytes_received"], 160);
    assert!(
        json["service_time"]["p50_ms"].as_f64().unwrap() >= 45.0,
        "p50 should reflect the real round trip: {json}"
    );

    let _ = std::fs::remove_file(report);
}

#[test]
fn tcp_with_the_wrong_framing_reports_a_clear_error() {
    let address = start_tcp_reply_server(b"12345678", Duration::from_millis(5));
    let report = temp_path("tcp-framing.json");

    let output = run_cannon(&[
        "--mode",
        "tcp",
        "-u",
        &address,
        "-c",
        "10",
        "-w",
        "1",
        "--body",
        "ping",
        "-o",
        report.to_str().unwrap(),
    ]);

    let combined = format!("{}{}", stdout(&output), stderr(&output));

    assert!(combined.contains("Response Framing"), "{combined}");
    assert!(combined.contains("--tcp-read-bytes"), "{combined}");

    let _ = std::fs::remove_file(report);
}

#[test]
fn tcp_read_until_waits_for_the_delimiter() {
    let address = start_tcp_reply_server(b"+OK\r\n", Duration::from_millis(20));
    let report = temp_path("tcp-until.json");

    let output = run_cannon(&[
        "--mode",
        "tcp",
        "-u",
        &address,
        "-c",
        "10",
        "-w",
        "1",
        "--body",
        "ping",
        "--tcp-read-until",
        "\\r\\n",
        "-o",
        report.to_str().unwrap(),
    ]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));

    let json = read_json(&report);

    assert_eq!(json["successes"], 10);
    assert_eq!(json["bytes_received"], 50);

    let _ = std::fs::remove_file(report);
}

#[test]
fn tcp_read_bytes_zero_does_not_wait_for_a_response() {
    let address = start_silent_tcp_server();

    let started = Instant::now();

    let output = run_cannon(&[
        "--mode",
        "tcp",
        "-u",
        &address,
        "-c",
        "20",
        "-w",
        "2",
        "--body",
        "ping",
        "--tcp-read-bytes",
        "0",
    ]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn tcp_server_dying_mid_run_does_not_hang_cannon() {
    let address = start_dying_tcp_server(3);
    let report = temp_path("tcp-dead.json");

    let started = Instant::now();

    let _ = run_cannon(&[
        "--mode",
        "tcp",
        "-u",
        &address,
        "-c",
        "200",
        "-w",
        "2",
        "--body",
        "ping",
        "--timeout",
        "500",
        "--connect-timeout",
        "500",
        "-o",
        report.to_str().unwrap(),
    ]);

    assert!(
        started.elapsed() < Duration::from_secs(15),
        "cannon hung after the server died"
    );

    let json = read_json(&report);

    assert!(json["successes"].as_u64().unwrap() >= 1);
    assert!(json["failures"].as_u64().unwrap() > 100);

    let _ = std::fs::remove_file(report);
}

#[test]
fn tcp_mode_without_body_is_rejected_instead_of_hanging() {
    let address = start_tcp_echo_server();

    let output = run_cannon(&["--mode", "tcp", "-u", &address, "-c", "10"]);

    assert!(!output.status.success());
    assert!(stderr(&output).contains("TCP mode requires a request payload"));
}

#[test]
fn tcp_mode_times_out_when_the_server_never_replies() {
    let address = start_silent_tcp_server();
    let report = temp_path("tcp-timeout.json");
    let started = Instant::now();

    let output = run_cannon(&[
        "--mode",
        "tcp",
        "-u",
        &address,
        "-c",
        "3",
        "-w",
        "1",
        "-t",
        "200",
        "--body",
        "{{value:1:u8}}",
        "-o",
        report.to_str().unwrap(),
    ]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "TCP run hung past its --timeout"
    );

    let json = read_json(&report);

    assert_eq!(json["failures"], 3);
    assert!(json["errors"]
        .as_object()
        .unwrap()
        .keys()
        .any(|key| key.contains("Timeout")));

    let _ = std::fs::remove_file(report);
}

#[test]
fn invalid_configuration_exits_with_a_clear_error() {
    let output = run_cannon(&["-u", "http://127.0.0.1:1", "-c", "0"]);

    assert!(!output.status.success());
    assert!(stderr(&output).contains("count must be greater than 0"));

    let output = run_cannon(&["-u", "ftp://example.com"]);

    assert!(!output.status.success());
    assert!(stderr(&output).contains("Unsupported URL scheme"));

    let output = run_cannon(&["--rps", "10", "--ramp-up", "banana", "-u", "http://x.test"]);

    assert!(!output.status.success());
}

#[test]
fn version_and_help_work() {
    let version = run_cannon(&["--version"]);

    assert!(version.status.success());
    assert!(stdout(&version).contains(env!("CARGO_PKG_VERSION")));

    let help = run_cannon(&["--help"]);
    let text = stdout(&help);

    assert!(help.status.success());
    assert!(text.contains("--ramp-up"));
    assert!(
        !text.contains("#[arg"),
        "help text must not leak source code"
    );
}

#[cfg(unix)]
#[test]
fn ctrl_c_stops_the_run_and_still_prints_a_partial_report() {
    let server = HttpServer::start("ok");
    let report = temp_path("sigint.json");

    let mut child = Command::new(CANNON)
        .args([
            "-u",
            &server.url(),
            "-c",
            "1000000",
            "-w",
            "4",
            "--rps",
            "50",
            "-o",
            report.to_str().unwrap(),
        ])
        .env("NO_COLOR", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    thread::sleep(Duration::from_millis(1500));

    let signalled = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(signalled.success());

    let deadline = Instant::now() + Duration::from_secs(15);

    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }

        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("cannon kept running after Ctrl-C");
        }

        thread::sleep(Duration::from_millis(50));
    };

    assert_eq!(status.code(), Some(130), "interrupted runs exit with 130");

    let mut out = String::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut out)
        .unwrap();

    assert!(out.contains("Interruption detected"));

    let json = read_json(&report);
    let done = json["successes"].as_u64().unwrap();

    assert!(done > 0, "some requests completed before the interrupt");
    assert!(done < 1000, "run stopped early instead of finishing");
    assert_eq!(json["total_requests"].as_u64().unwrap(), done);

    let _ = std::fs::remove_file(report);
}

#[test]
fn shipped_example_scenario_runs_with_headers() {
    // Regression test: YAML files with `headers:` used to panic.
    let server = HttpServer::start("ok");
    let scenario = format!("{}/examples/scenario.yml", env!("CARGO_MANIFEST_DIR"));

    let output = run_cannon(&["-f", &scenario, "-u", &server.url(), "-c", "20", "-w", "4"]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("Failures: 0"));
    assert_eq!(server.requests(), 20);
}

#[test]
fn network_errors_are_not_reported_as_assertion_failures() {
    // Bind then drop to get a port nothing listens on.
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let url = format!("http://127.0.0.1:{port}/");

    let output = run_cannon(&["-u", &url, "-c", "5", "-w", "1", "-t", "500"]);
    let text = stdout(&output);

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(text.contains("Failures: 5"));
    assert!(
        !text.contains("Assertion Failures"),
        "no --expect was configured, so nothing can be an assertion failure:\n{text}"
    );
}

fn unused_port_url() -> String {
    // Bind then drop to obtain a port nothing is listening on.
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();

    format!("http://127.0.0.1:{port}/")
}

#[test]
fn a_service_that_is_down_fails_the_gate_instead_of_looking_faster() {
    let server = HttpServer::start("ok");
    let baseline = temp_path("down-baseline.json");

    let saved = run_cannon(&[
        "-u",
        &server.url(),
        "-c",
        "50",
        "--save-baseline",
        baseline.to_str().unwrap(),
    ]);
    assert!(saved.status.success(), "stderr: {}", stderr(&saved));

    // Same baseline, but the service is now unreachable: every request fails
    // almost instantly, which used to be reported as "87% faster".
    let report = temp_path("down-report.json");

    let output = run_cannon(&[
        "-u",
        &unused_port_url(),
        "-c",
        "20",
        "-w",
        "2",
        "--compare-baseline",
        baseline.to_str().unwrap(),
        "-o",
        report.to_str().unwrap(),
    ]);

    assert_eq!(output.status.code(), Some(2));
    assert!(stdout(&output).contains("no request succeeded"));
    assert!(!stdout(&output).contains("improvement"));

    // The report is still written so CI can attach it to the failed job.
    assert_eq!(read_json(&report)["failures"], 20);

    let _ = std::fs::remove_file(baseline);
    let _ = std::fs::remove_file(report);
}

#[test]
fn max_error_rate_fails_the_gate_only_above_the_limit() {
    let url = unused_port_url();

    let strict = run_cannon(&["-u", &url, "-c", "10", "--max-error-rate", "50"]);
    assert_eq!(strict.status.code(), Some(2));
    assert!(stdout(&strict).contains("Error rate 100.00%"));

    let lenient = run_cannon(&["-u", &url, "-c", "10", "--max-error-rate", "100"]);
    assert!(lenient.status.success(), "stderr: {}", stderr(&lenient));

    // Without any threshold, failures are reported but are not fatal.
    let none = run_cannon(&["-u", &url, "-c", "10"]);
    assert!(none.status.success());
}

#[test]
fn saving_a_baseline_from_a_broken_run_is_refused_and_keeps_the_old_file() {
    let baseline = temp_path("keep-baseline.json");
    let original = r#"{"p99_ms": 12.5, "rps": 100.0}"#;

    std::fs::write(&baseline, original).unwrap();

    let output = run_cannon(&[
        "-u",
        &unused_port_url(),
        "-c",
        "5",
        "--save-baseline",
        baseline.to_str().unwrap(),
    ]);

    assert_eq!(output.status.code(), Some(2));
    assert!(stdout(&output).contains("Baseline not saved"));
    assert_eq!(std::fs::read_to_string(&baseline).unwrap(), original);

    let _ = std::fs::remove_file(baseline);
}

#[test]
fn a_bad_baseline_file_is_reported_before_the_test_runs() {
    let missing = run_cannon(&[
        "-u",
        "http://127.0.0.1:1/",
        "--compare-baseline",
        "/definitely/not/here.json",
    ]);

    assert_eq!(missing.status.code(), Some(1));
    assert!(stderr(&missing).contains("cannot read baseline"));
    assert!(!stdout(&missing).contains("Preparing the Cannon"));
}

#[test]
fn latency_mode_and_service_time_are_reported() {
    let server = HttpServer::start("ok");
    let paced = temp_path("mode-paced.json");
    let closed = temp_path("mode-closed.json");

    let output = run_cannon(&[
        "-u",
        &server.url(),
        "-c",
        "20",
        "--rps",
        "200",
        "-o",
        paced.to_str().unwrap(),
    ]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));

    let json = read_json(&paced);
    assert_eq!(json["latency_mode"], "open-loop");
    assert_eq!(json["target_rps"], 200);
    assert!(json["service_time"]["p99_ms"].is_number());
    assert_eq!(json["schema_version"], 2);

    let output = run_cannon(&[
        "-u",
        &server.url(),
        "-c",
        "20",
        "-o",
        closed.to_str().unwrap(),
    ]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(read_json(&closed)["latency_mode"], "closed-loop");

    let _ = std::fs::remove_file(paced);
    let _ = std::fs::remove_file(closed);
}

#[test]
fn failed_requests_do_not_pollute_latency_statistics() {
    let report = temp_path("failed-latency.json");

    let output = run_cannon(&[
        "-u",
        &unused_port_url(),
        "-c",
        "10",
        "-o",
        report.to_str().unwrap(),
    ]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));

    let json = read_json(&report);

    assert_eq!(json["failures"], 10);
    assert_eq!(json["error_rate_percent"], 100.0);
    assert_eq!(json["p99_ms"], 0.0, "no success, so no latency");
    assert_eq!(json["apdex_score"], 0.0);

    let _ = std::fs::remove_file(report);
}
