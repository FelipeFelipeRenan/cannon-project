# Changelog

All notable changes to Cannon will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [2.2.0] - Unreleased

### 🐛 Fixed

- **Opaque HTTP errors:** timeouts, DNS failures, refused connections, TLS errors and exhausted file descriptors were all reported as "error sending request". They now appear as `Timeout`, `DNS Error`, `TLS Error`, `Connection Refused`, `Connection Reset`, `File Descriptor Limit`, `Invalid Request`, ... with the underlying cause.
- **Invalid input:** `--rps 4000000000` panicked and `-w 4000000000` hung. Both are now bounded (`rps` ≤ 10 000 000, `workers` ≤ 100 000), and malformed `-H` values (no colon, invalid name/value) are rejected up front instead of silently failing every request.
- **Redirects:** they were followed silently, so each reported request could cost several hits on the server. Redirects are no longer followed; the 3xx response is reported as is.
- **Coordinated omission:** with `--rps`, latency is now measured from the *scheduled* send time, so a server stall is no longer hidden by workers waiting to send. The report shows the latency mode (`open-loop`/`closed-loop`) and the raw service time separately.
- **TCP response framing:** Cannon used to read exactly one byte, so multi-byte replies left data in the socket and corrupted every later latency. Use `--tcp-read-bytes` / `--tcp-read-until`; a mismatch is reported as a `Response Framing` error.
- **TCP hang:** when the server died, the connection pool shrank until requests waited forever (and Ctrl-C did not help). Broken connections are now replaced lazily, so requests fail fast.
- **Regression gate:** a service that was completely down was reported as "87% faster" with exit 0. The gate now also checks the error rate, fails when nothing succeeded, and refuses to save such a baseline.
- **`--ramp-up`:** the flag was parsed as a positional argument (and leaked source code into `--help`). It is now a real `--ramp-up <DURATION>` option and can also be set in YAML.
- **YAML `headers`:** any config file containing `headers:` crashed with a panic. Fixed, with regression tests.
- **TCP timeouts:** `--timeout` and `--connect-timeout` were ignored in TCP mode, so a server that never replied hung Cannon forever. Both are now enforced, and failed requests are reported as `Timeout` and replaced in the connection pool.
- **Ctrl-C:** interrupting a run now stops scheduling new requests, lets in-flight requests finish and still prints/exports the partial report. A second Ctrl-C quits immediately.
- **Assertion accounting:** network/transport errors were counted as "assertion failures" even without `--expect`. `--expect` is now also evaluated against non-UTF-8 bodies instead of silently passing.
- **Percentile labels:** floating-point noise no longer leaks into labels (for example `p7.000000000000001`).
- Removed avoidable panics reachable from user input (missing TCP address, CPU topology failure, histogram bounds).

### 🚀 Added

- A hint is printed when more than half of the requests fail, pointing at the most likely cause (refused connection, DNS, TLS, timeouts, descriptor limit, TCP framing, unexpected status codes).
- Seeded fuzz tests for the payload, status-filter, delimiter and baseline parsers, and a test that `{{email}}`/`{{username}}` do not collide over 200 000 values.
- `--max-redirects N` to follow redirects (default 0), `--expect-status` to define which status codes count as success (`200,404`, `2xx`), `--cacert` for private CAs, `--cert`/`--key` for mutual TLS and `--no-color` (all also available in YAML).
- `--no-proxy` (CLI and YAML), plus a warning when `HTTP_PROXY`/`HTTPS_PROXY`/`ALL_PROXY` is set, since the proxy would be part of what is measured.
- `rust-version = "1.88"` in `Cargo.toml` and a CI job that checks the build with it.
- On Unix, Cannon raises the open-file soft limit to the hard limit at startup and warns when `--workers` still would not fit.
- `--tcp-read-bytes`, `--tcp-read-until` (CLI and YAML) and `Target::new_tcp_with_options` / `TcpRead`.
- `--max-error-rate` (CLI and YAML) and error-rate data in baseline files (older baselines still load).
- Distinct exit codes: `2` gate failure, `130` SIGINT, `143` SIGTERM.
- Apdex score (with its threshold) in the terminal summary.
- `ramp_up`, `user_agent`, `percentiles`, `output` and `html` can now be set in the YAML config file.
- Linux arm64 and macOS arm64 release binaries; `install.sh` and `--update` pick the right asset for the CPU architecture.
- End-to-end test suite that runs the real binary against local HTTP/TCP servers; CI now runs on Linux, macOS and Windows.
- `Target::new_tcp_with_timeouts` in the library API.

### 🔄 Changed

- Scripts renamed: `long-runing.sh` → `long-running.sh`, `profile-tcp-scalling.sh` → `profile-tcp-scaling.sh`; `benchmark-results/` is git-ignored.
- README documents `{{timestamp}}` (milliseconds), `{{seq}}` and the range of `{{number}}`; the TCP benchmark note explains the framing change.
- The HTML dashboard embeds Chart.js 4.4.1 (MIT), so it works offline and no longer depends on an unpinned CDN script.
- Unknown keys in the YAML config file are now an error (a typo such as `worker:` used to be ignored silently).
- Redesigned terminal report: one-line verdict, sections (latency, status codes, failures, throughput), human-readable durations and sizes, and an Apdex rating.
- Removed the unused `rand`, `uuid` and `async-trait` dependencies (`uuid` was what raised the minimum Rust version).
- Latency histograms/percentiles now contain **successful requests only**; failures appear in counters and the error breakdown. Apdex counts failed requests as frustrated.
- The CSV export has an extra column with the service time; baseline files record `schema_version`, `error_rate_percent` and `latency_mode`.
- Interrupting with Ctrl-C now exits with 130 instead of 0.
- TCP mode now requires `--body`: Cannon always waits for a response after writing, so an empty payload could only hang. It now fails fast with an explanatory error.
- The default `User-Agent` is `Cannon/<version>` instead of a hard-coded `Cannon/1.0`.
- The JSON report's `total_requests` is the number of requests actually measured (it matches the requested count unless the run was interrupted).
- Terminal summary: "Transfered/totais" typo fixed, duplicate counters removed, empty sections hidden, "Real RPS" renamed "Actual RPS".
- Release workflow verifies the tag matches `Cargo.toml`, runs tests and a `--version` smoke test per target, and no longer uses the retired `macos-13` runner.
- `scenario_test.yml` moved to `examples/scenario.yml`.

## [2.1.0] - 2026

### 🚀 New Features

- **HTTP/2 Support:** Added `--http2` flag to force HTTP/2 Prior Knowledge for h2c/local testing.
- **Connect Timeout:** Added `--connect-timeout` flag for separate timeout when establishing TCP connections (default: 5000ms).
- **CSV Export:** Added `--csv` flag to export raw request data for external analysis.
- **Custom Percentiles:** Added `--percentiles` flag to configure which latency percentiles to display (default: 50,95,99).
- **Warm-up Period:** Added `--warmup` flag to discard initial metrics during warm-up period to avoid cold-start distortions.
- **TCP Raw Mode:** Added `--mode tcp` for native support of custom binary protocols via direct TCP connections.
- **Binary Payload Tags:** Added support for binary tags in TCP mode: `{{number:u8}}`, `{{number:u16be}}`, `{{number:u16le}}`, `{{number:u32be}}`, `{{number:u32le}}`, `{{number:u64be}}`, `{{number:u64le}}`, `{{value:N:type}}`.

### ⚡ Performance Improvements

- **MiMalloc Allocator:** Integrated `mimalloc` as the global allocator, reducing memory contention in high-concurrency scenarios.
- **LTO Optimization:** Enabled Link-Time Optimization (`lto=true`, `codegen-units=1`, `panic=abort`) for highly optimized binaries.
- **Connection Pooling:** Aggressive TCP socket reuse to prevent ephemeral port exhaustion under heavy loads.
- **Zero-Copy Rendering:** Payload generator avoids unnecessary allocations with reusable buffers.

### 🛠️ Improvements & Bug Fixes

- **Library Modularization:** Refactored into idiomatic `lib.rs`/`bin` structure with isolated components (`engine`, `client`, `report`, `args`, `security`, `payload`, `metrics`, `utils`).
- **Graceful Error Handling:** YAML configuration merge now returns typed `Result`s instead of panicking.
- **TLS via rustls:** Secure and performant TLS implementation without native dependencies.
- **HTML Dashboard:** Interactive dashboard with Chart.js graphs, key metrics cards, PDF export, and responsive dark theme.
- **Apdex Score:** Automatic calculation of Application Performance Index with customizable tolerance threshold.
- **Response Validation:** Added `--expect` flag to assert response body content.
- **Auto-Update:** Built-in `--update` flag to check and install latest version from GitHub Releases.

### 📊 Reporting Enhancements

- **ASCII Histogram:** Visual latency distribution directly in terminal.
- **JSON Reports:** Structured metrics export for CI/CD integration.
- **HTML Dashboard:** Interactive charts and metrics with Chart.js.
- **CSV Export:** Raw data export for external analysis tools.
- **Color-coded Output:** Status codes and errors displayed with colors in CLI.

### 🔧 Technical Changes

- **Producer-Consumer Pattern:** Request engine and workers communicate via bounded async channels.
- **Backpressure Management:** Bounded worker pool and job channel to prevent socket exhaustion.
- **Polymorphic Enum:** `Target` enum with static dispatch (zero vtable overhead) for HTTP/TCP.
- **HdrHistogram:** High-precision metrics recording latencies in microseconds (coordinated omission is only avoided from 2.2.0 on, in `--rps` mode).
- **Tokio Async Runtime:** Non-blocking I/O and multiplexing for maximum efficiency.

---

## [0.2.0] - 2026-06-30

### 🚀 Epic Feature: Multi-Protocol Architecture

Cannon is no longer just an HTTP load tester. The core engine has been entirely decoupled into a Trait-based architecture (`TargetClient`), allowing native support for multiple protocols.

- **Raw TCP Target (`--mode tcp`):** Added support for injecting raw bytes directly into TCP sockets. Ideal for testing in-memory data stores, custom binary protocols, and High-Frequency Trading (HFT) matching engines with zero-copy parsing.

### ⚡ Extreme Performance Optimizations

- **Custom Memory Allocator:** Integrated `mimalloc` (Microsoft's memory allocator) as the global allocator, drastically reducing lock contention across high-concurrency worker threads.
- **Connection Pooling & Keep-Alive:** Hardened the HTTP client builder to aggressively reuse TCP sockets, preventing OS-level ephemeral port exhaustion under heavy loads (10k+ RPS).

### 🛠️ Architecture Refactoring & Stability

- **Library Modularization:** Split the monolithic `main.rs` into an idiomatic `lib.rs`/`bin` structure, isolating components (`engine`, `client`, `report`, `args`, `security`, `payload`).
- **Graceful Error Handling:** Eliminated panics in the YAML configuration merge process. Configuration errors now gracefully return typed `Result`s.
- **TCP Payload Integrity:** Implemented automatic EOF/newline injection (`\n`) for TCP payloads to prevent kernel-level deadlocks on server-side text scanners.
- **Code Quality:** Resolved all Clippy warnings and refactored internal sorting logic to use `sort_by_key`.
