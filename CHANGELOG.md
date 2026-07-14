# Changelog

All notable changes to Cannon will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

- **Producer-Consumer Pattern:** Request engine and metrics aggregator communicate via Asynchronous MPSC Channels.
- **Backpressure Management:** Strictly managed concurrency via semaphores to prevent socket exhaustion.
- **Polymorphic Enum:** `Target` enum with static dispatch (zero vtable overhead) for HTTP/TCP.
- **HdrHistogram:** High-precision metrics recording latencies in microseconds, avoiding "coordinated omission".
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
