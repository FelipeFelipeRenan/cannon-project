<div align="center">

# 🚀 Cannon

### High-performance load testing for HTTP & TCP services

**Fast. Predictable. Lightweight.**

[![Release](https://github.com/FelipeFelipeRenan/cannon-project/actions/workflows/release.yml/badge.svg)](https://github.com/FelipeFelipeRenan/cannon-project/actions/workflows/release.yml)
[![CI](https://github.com/FelipeFelipeRenan/cannon-project/actions/workflows/ci.yml/badge.svg)](https://github.com/FelipeFelipeRenan/cannon-project/actions/workflows/ci.yml)

</div>

---

Cannon is a **high-performance load testing tool written in Rust** for HTTP APIs and raw TCP services.

It is designed around a simple idea:

> **The load generator should introduce as little overhead as possible so you can focus on the system you're actually testing.**

Cannon provides concurrent request generation, constant-RPS workloads, warm-up phases, dynamic payloads, latency histograms, assertions, and structured reports — all delivered as a single command-line tool.

---

## ⚡ Why Cannon?

Load testing is more than sending a lot of requests.

A useful load generator needs to answer questions like:

* How much traffic did I actually generate?
* What happened to tail latency?
* Did the service remain stable under sustained load?
* How many requests actually succeeded?
* What errors occurred?
* Did the workload behave as expected?
* How did this run compare with a previous baseline?

Cannon is built to make those questions measurable.

---

# 📦 Installation

You **don't need Rust** to use Cannon.

Pre-built binaries are published with each release.

### One-line installation

**Linux / macOS**

```bash
curl -sSL https://raw.githubusercontent.com/FelipeFelipeRenan/cannon-project/main/install.sh | sh
```

The installer downloads the latest compatible release and places `cannon` in `/usr/local/bin`.

Verify:

```bash
cannon --version
```

### 🔄 Update

Already have Cannon installed?

```bash
cannon --update
```

Cannon can check for and install the latest available release.

### 📥 Manual installation

Pre-built binaries are available on the **[Releases](https://github.com/FelipeFelipeRenan/cannon-project/releases)** page.

For example, on Linux:

```bash
chmod +x cannon-linux-x64
sudo mv cannon-linux-x64 /usr/local/bin/cannon
```

### 🦀 Build from source

For development or unreleased changes:

```bash
git clone https://github.com/FelipeFelipeRenan/cannon-project.git
cd cannon-project
cargo build --release
```

The binary will be available at:

```text
target/release/cannon
```

---

# 🚀 Quick Start

## Simple HTTP load test

Send 5,000 requests using 20 workers:

```bash
cannon \
  -u http://localhost:8080/api \
  -c 5000 \
  -w 20
```

That's it.

---

## 🎯 Constant-RPS workload

Generate **100 requests per second**:

```bash
cannon \
  -u http://localhost:8080/api \
  -c 5000 \
  -w 20 \
  --rps 100
```

Cannon separates the **requested load** from the **measured results**, allowing you to compare the target rate against the actual throughput achieved during the test.

---

## 🔥 Warm-up + measurement

Warm up the service for 10 seconds, then measure 5,000 requests:

```bash
cannon \
  -u http://localhost:8080/api \
  -c 5000 \
  -w 20 \
  --rps 100 \
  --warmup 10
```

The warm-up generates real traffic, but its requests are **excluded from the reported measurements**.

This avoids mixing cold-start behavior with the actual benchmark.

---

## 🧬 Dynamic payloads

Generate different data for every request:

```bash
cannon \
  -u http://localhost:8080/users \
  -X POST \
  -c 1000 \
  --body '{"id":"{{uuid}}","email":"{{email}}"}' \
  -H 'Content-Type: application/json'
```

Available dynamic values include:

| Tag             | Description            |
| --------------- | ---------------------- |
| `{{uuid}}`      | UUID v4                |
| `{{email}}`     | Random email           |
| `{{username}}`  | Random username        |
| `{{number}}`    | Random number          |
| `{{timestamp}}` | Current Unix timestamp |

---

# 📊 What Cannon Measures

Cannon collects latency information using **HdrHistogram** and reports configurable percentiles.

By default:

```text
p50   p95   p99
```

You can choose your own:

```bash
cannon \
  -u http://localhost:8080 \
  -c 10000 \
  --percentiles 50,90,95,99,99.9
```

### Terminal output

The final report includes information such as:

* total requests
* successful requests
* failed requests
* actual RPS
* latency percentiles
* minimum / average / maximum latency
* status-code distribution
* failure breakdown
* bytes sent / received
* Apdex
* target vs actual RPS

The goal is not simply to tell you **"it handled 10k requests."**

The goal is to show **how the system behaved while handling them**.

---

# 📈 Reports

Cannon can export results in multiple formats.

### JSON

```bash
cannon \
  -u http://localhost:8080 \
  -c 5000 \
  -o report.json
```

Useful for automation, CI/CD pipelines and post-processing.

### CSV

```bash
cannon \
  -u http://localhost:8080 \
  -c 5000 \
  --csv results.csv
```

Exports per-request data for external analysis.

### HTML

```bash
cannon \
  -u http://localhost:8080 \
  -c 5000 \
  --html dashboard.html
```

Generates an interactive report with charts and summarized metrics.

---

# 🧪 Response Assertions

You can validate that responses contain an expected value:

```bash
cannon \
  -u http://localhost:8080/api/user \
  -c 1000 \
  --expect '"status":"success"'
```

A request can therefore fail not only because the connection failed, but also because the response did not satisfy the expected assertion.

This is particularly useful for automated performance checks.

---

# 🔌 Raw TCP

Cannon is not limited to HTTP.

Use TCP mode for custom protocols and binary payloads:

```bash
cannon \
  --mode tcp \
  -u 127.0.0.1:9000 \
  -c 1000 \
  -w 20
```

Binary payloads can be generated directly:

```text
{{number:u8}}
{{number:u16be}}
{{number:u16le}}
{{number:u32be}}
{{number:u32le}}
{{number:u64be}}
{{number:u64le}}
```

Fixed binary values are also supported:

```text
{{value:42:u8}}
{{value:1000:u16be}}
```

This makes Cannon useful for testing services that don't speak HTTP at all.

---

# 🛠️ CLI

Cannon exposes a deliberately small command-line interface.

| Option              | Description                          |
| ------------------- | ------------------------------------ |
| `-u, --url`         | Target endpoint                      |
| `-c, --count`       | Number of measured requests          |
| `-w, --workers`     | Number of concurrent workers         |
| `-r, --rps`         | Target request rate                  |
| `--warmup`          | Warm-up duration                     |
| `--mode`            | `http` or `tcp`                      |
| `-X, --method`      | HTTP method                          |
| `-b, --body`        | Request payload                      |
| `-H, --header`      | HTTP header                          |
| `-A, --user-agent`  | User-Agent                           |
| `-t, --timeout`     | Request timeout                      |
| `--connect-timeout` | TCP connection timeout               |
| `--expect`          | Response assertion                   |
| `--percentiles`     | Custom latency percentiles           |
| `--apdex-t`         | Apdex threshold                      |
| `-o, --output`      | JSON report                          |
| `--csv`             | CSV report                           |
| `--html`            | HTML report                          |
| `--http2`           | Force HTTP/2 prior knowledge         |
| `-k, --insecure`    | Disable TLS certificate verification |
| `--update`          | Update Cannon                        |

For the complete list:

```bash
cannon --help
```

---

# 🧠 Measurement Model

One important design decision in Cannon is the separation between **warm-up traffic** and **measured traffic**.

With:

```bash
cannon -c 250000 --warmup 4
```

the execution is conceptually:

```text
┌──────────────────────┐
│      Prepare         │
└──────────┬───────────┘
           │
           ▼
┌──────────────────────┐
│      Warm-up         │
│       4 seconds      │
│                      │
│   Metrics ignored    │
└──────────┬───────────┘
           │
           ▼
┌──────────────────────┐
│     Measurement      │
│                      │
│   250,000 requests   │
│      measured        │
└──────────┬───────────┘
           │
           ▼
┌──────────────────────┐
│        Report        │
└──────────────────────┘
```

Without warm-up, the requested count represents the measured workload directly.

This makes benchmark runs easier to reason about and compare.

---

# 🏗️ Architecture

Cannon is built around a small set of focused components:

```text
                         ┌───────────────┐
                         │   CLI / Args  │
                         └───────┬───────┘
                                 │
                                 ▼
                         ┌───────────────┐
                         │     Engine    │
                         └───────┬───────┘
                                 │
                    ┌────────────┴────────────┐
                    │                         │
                    ▼                         ▼
             ┌──────────────┐         ┌──────────────┐
             │    Workers   │         │    Payload   │
             │              │         │   Generator  │
             └──────┬───────┘         └──────────────┘
                    │
                    ▼
             ┌──────────────┐
             │    Target    │
             ├──────────────┤
             │ HTTP │ TCP   │
             └──────┬───────┘
                    │
                    ▼
                 Service
                    │
                    ▼
             ┌──────────────┐
             │   Metrics    │
             │  Histogram   │
             └──────┬───────┘
                    │
                    ▼
             ┌──────────────┐
             │    Report    │
             └──────────────┘
```

### Core technologies

* **Rust** — systems-level control and predictable runtime characteristics
* **Tokio** — asynchronous networking and task scheduling
* **Reqwest** — HTTP client
* **HdrHistogram** — latency distribution
* **Clap** — CLI parsing
* **MiMalloc** — alternative allocator
* **rustls** — TLS without native OpenSSL dependencies

---

# ⚙️ Design Goals

Cannon is intentionally opinionated.

### Small runtime overhead

The generator itself should not become the bottleneck of the benchmark.

### Predictable workloads

Constant-RPS mode and explicit warm-up/measurement phases make test behavior easier to reason about.

### Reusable connections

HTTP clients and TCP connections are reused instead of creating a new connection for every request.

### Low-level control

Rust makes it possible to control allocation, concurrency, networking and binary payload generation without requiring a large abstraction stack.

### One executable

The end-user experience should be simple:

```text
install
  ↓
cannon
  ↓
measure
  ↓
report
```

---

# 🔬 Performance

Performance claims should be backed by measurements rather than marketing numbers.

Cannon therefore includes a performance-oriented architecture and is intended to be benchmarked against representative workloads.

Areas of interest include:

* request throughput
* latency overhead
* memory usage
* allocation behavior
* concurrency scaling
* constant-RPS accuracy
* CPU utilization

Benchmark results will depend heavily on the target protocol, payload, machine, network and workload configuration.

---

# 🤖 CI/CD

Cannon can be used as part of automated performance checks.

For example:

```bash
cannon \
  -u "$API_URL/health" \
  -c 1000 \
  -w 20 \
  --expect '"status":"ok"' \
  -o results.json
```

The generated JSON can then be consumed by scripts or CI pipelines.

Example:

```bash
jq '.failures == 0' results.json
```

---

# 🧰 Development

Run the test suite:

```bash
cargo test
```

Run documentation tests:

```bash
cargo test --doc
```

Run Clippy:

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

Build the optimized binary:

```bash
cargo build --release
```

Generate API documentation:

```bash
cargo doc --no-deps
```

---

# 🗺️ Roadmap

Cannon is actively evolving.

### Completed

* [x] HTTP load testing
* [x] Raw TCP mode
* [x] Concurrent workers
* [x] Constant-RPS scheduling
* [x] Warm-up phases
* [x] Dynamic payload generation
* [x] Binary payload generation
* [x] Response assertions
* [x] Latency histograms
* [x] Custom percentiles
* [x] JSON reports
* [x] CSV export
* [x] HTML reports
* [x] Apdex
* [x] Baseline comparison
* [x] CPU affinity
* [x] Automatic updates

### Planned

* [ ] Distributed load generation
* [ ] Chaos engineering workloads
* [ ] More benchmark tooling
* [ ] Additional protocol capabilities

---

# 📜 License

Cannon is open source and distributed under the terms of the project's license.

See [`LICENSE`](LICENSE) for details.

---

<div align="center">

### Built in Rust 🦀

**Cannon — generate the load. Measure the system.**

</div>



### **👨‍💻 Author**

**Felipe Fernandes** – [LinkedIn](https://linkedin.com/in/felipefernandesss) | [GitHub](https://github.com/FelipeFelipeRenan)

*"Robust systems require relentless testing."*

