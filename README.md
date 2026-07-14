[![Release Cannon](https://github.com/FelipeFelipeRenan/cannon-project/actions/workflows/release.yml/badge.svg)](https://github.com/FelipeFelipeRenan/cannon-project/actions/workflows/release.yml)

[![CI Pipeline](https://github.com/FelipeFelipeRenan/cannon-project/actions/workflows/ci.yml/badge.svg)](https://github.com/FelipeFelipeRenan/cannon-project/actions/workflows/ci.yml)

# **🚀 Cannon**

```
      _____          _   _ _   _  ____  _   _
     / ____|   /\   | \ | | \ | |/ __ \| \ | |
    | |       /  \  |  \| |  \| | |  | |  \| |
    | |      / /\ \ | . ` | . ` | |  | | . ` |
    | |____ / ____ \| |\  | |\  | |__| | |\  |
     \_____/_/    \_\_| \_|_| \_|\____/|_| \_|
```

**The High-Velocity Load Tester**

**Cannon** is a high-performance load testing tool written in **Rust**. Built for mission-critical APIs, it leverages the Tokio async runtime to deliver sub-millisecond precision and extreme concurrency with minimal resource consumption.

## **✨ Key Features**

* **⚡ Extremely Fast:** Built in Rust with zero garbage collection. The bottleneck will be your server, not your testing tool.
* **🎯 Constant RPS Mode:** Integrated metronome logic to fire requests at an exact rate (Requests Per Second) for stable load tests.
* **🔥 Warm-up Period:** Discard initial metrics during warm-up period to avoid cold-start distortions.
* **🧬 Dynamic Payloads:** Real-time data generation engine with zero-copy rendering. Use placeholders like `{{user}}`, `{{email}}`, `{{uuid}}`, `{{timestamp}}`, `{{number}}` to simulate unique users.
* **📊 Comprehensive Reports:** Interactive CLI with real-time RPS counter, ASCII latency histogram, detailed JSON reports, and **interactive HTML dashboard** with Chart.js graphs.
* **🎯 Apdex Score:** User satisfaction metric based on response times (≤50ms satisfactory, 51-200ms tolerable).
* **🛠️ Full REST Support:** GET, POST, PUT, PATCH, DELETE with custom headers, configurable user-agent, and dynamic JSON bodies.
* **🔌 TCP Raw Mode:** Native support for custom binary protocols via direct TCP connections with aggressive connection pooling.
* **✅ Response Validation:** Assert HTTP response content with the `--expect` flag.
* **📦 Zero-Dependency Binary:** Single static binary <5MB. Download and run on Linux, macOS, or Windows without installing runtimes.
* **🔄 Auto-Update:** Automatic update to the latest version with `--update`.
* **📈 Custom Percentiles:** Configure which latency percentiles you want in the report (default: 50,95,99).
* **📊 CSV Export:** Export raw data from each request for later analysis in external tools.
* **🔒 TLS via rustls:** Secure and performant TLS implementation without native dependencies.
* **💾 MiMalloc Allocator:** Optimized memory allocator to reduce contention in high concurrency scenarios.
* **🌐 HTTP/2 Support:** Force HTTP/2 Prior Knowledge for h2c/local testing with `--http2`.
* **⏱️ Connect Timeout:** Separate timeout for establishing TCP connections with `--connect-timeout`.

## **📦 Installation**

### **Pre-compiled Binaries (Recommended)**

Download the latest binary for your architecture from the **Releases** page.

#### Install globally on Linux via cURL:

```bash
curl -sSL https://raw.githubusercontent.com/FelipeFelipeRenan/cannon-project/main/install.sh | sh
```

#### Manual example for Linux:

```bash
chmod +x cannon-linux-x64
sudo mv cannon-linux-x64 /usr/local/bin/cannon
```

#### Verify installation:

```bash
cannon --version
```

#### Update to latest version:

```bash
cannon --update
```

### **Build from Source**

```bash
git clone https://github.com/FelipeFelipeRenan/cannon-project.git
cd cannon-project
cargo build --release
# Binary will be at target/release/cannon
```

## **🛠️ CLI Arguments**

Use these flags to configure your load test.

| Short Flag | Long Flag | Description | Default |
| :---- | :---- | :---- | :---- |
| `-u` | `--url` | **(Required)** Target endpoint URL (e.g., `http://localhost:8080/api`). | - |
| `-f` | `--config` | Path to YAML configuration file. | - |
| `-c` | `--count` | Total number of requests to fire. | `1` |
| `-w` | `--workers` | Number of concurrent workers (simultaneous connections). | `10` |
| `-r` | `--rps` | Limit throughput to a specific RPS (Constant Load). | None |
| `-X` | `--method` | HTTP Method: GET, POST, PUT, PATCH, DELETE. | `GET` |
| `-b` | `--body` | JSON payload for the request. Supports dynamic tags. | None |
| `-o` | `--output` | Path to save detailed report in `.json` format. | None |
| `-t` | `--timeout` | Timeout in milliseconds to cancel slow requests. | `30000` |
| `-H` | `--header` | Custom header (e.g., `Authorization: Bearer token`). Repeat for multiple. | None |
| `-A` | `--user-agent` | Request User-Agent. | `Cannon/1.0` |
| `-k` | `--insecure` | Ignore TLS/SSL certificate validation. | `false` |
| | `--mode` | Protocol mode: `http` or `tcp`. | `http` |
| | `--warmup` | Warm-up time in seconds (metrics discarded). | `0` |
| | `--expect` | Expected string in response body for validation (assertion). | None |
| | `--html` | Path to save interactive HTML report with charts. | None |
| | `--csv` | Path to export raw data in CSV format. | None |
| | `--apdex-t` | Apdex tolerable time in ms (base for calculation). | `50` |
| | `--percentiles` | Percentiles for the report (e.g., `50,95,99,99.9`). | `50,95,99` |
| | `--http2` | Force HTTP/2 Prior Knowledge (useful for localhost/h2c). | `false` |
| | `--connect-timeout` | Timeout only for establishing TCP connection (ms). | `5000` |
| | `--update` | Check and install available update. | - |

## **🧬 Dynamic Payload Tags**

When using the `--body` flag, you can inject dynamic data into the JSON to ensure unique requests and bypass database constraints.

| Tag | Substitution Logic | Usage Example |
| :---- | :---- | :---- |
| `{{user}}` | Generates random 8-character alphanumeric string (lowercase). | `"username": "user_{{user}}"` |
| `{{random}}` | Alias for `{{user}}`. | `"id": "req_{{random}}"` |
| `{{email}}` | Generates random email in format `xxxxxxxx@example.com`. | `"email": "{{email}}"` |
| `{{number}}` | Generates random integer between 10 and 9999. | `"amount": {{number}}` |
| `{{uuid}}` | Generates unique UUID v4. | `"requestId": "{{uuid}}"` |
| `{{timestamp}}` | Generates Unix timestamp in milliseconds. | `"createdAt": {{timestamp}}` |

### **Binary Tags (for TCP Mode)**

For custom binary protocols, use special tags:

| Tag | Description | Example |
| :---- | :---- | :---- |
| `{{number:u8}}` | Random u8 number (0-255) | - |
| `{{number:u16be}}` | Random u16 number big-endian | - |
| `{{number:u16le}}` | Random u16 number little-endian | - |
| `{{number:u32be}}` | Random u32 number big-endian | - |
| `{{number:u32le}}` | Random u32 number little-endian | - |
| `{{number:u64be}}` | Random u64 number big-endian | - |
| `{{number:u64le}}` | Random u64 number little-endian | - |
| `{{value:42:u8}}` | Fixed value 42 as u8 | - |
| `{{value:1000:u16be}}` | Fixed value 1000 as u16 big-endian | - |

## **🚀 Quick Start**

### **1. Simple Stress Test (GET)**

Fire 5000 requests with 20 concurrent workers:

```bash
cannon -u http://localhost:8081/api/v1/accounts -c 5000 -w 20
```

### **2. Stable Load Simulation (RPS + Dynamic POST)**

Create 1000 unique accounts at a constant rate of 100 requests per second:

```bash
cannon -u http://localhost:8081/api/v1/accounts \
    -c 1000 \
    -w 10 \
    -X POST \
    --rps 100 \
    --body '{"clientId": "user_{{user}}_test", "currency": "BRL"}'
```

### **3. Test with Warm-up Period**

Fire 5000 requests, discarding first 10 seconds of metrics:

```bash
cannon -u 'http://localhost:8081/api/v1/accounts' \
  -c 5000 \
  -w 20 \
  --rps 100 \
  --warmup 10 \
  -H 'Authorization: Bearer your_token'
```

### **4. Test with Response Validation (Assertion)**

Validate if response body contains the string `admin_privileges`:

```bash
cannon -u 'http://api.example.com/v1/user' \
  --expect 'admin_privileges' \
  -c 1000 -w 10
```

### **5. Complete Test with JSON and HTML Export**

```bash
cannon -u 'http://localhost:8081/api/v1/accounts' \
  -c 5000 \
  -w 50 \
  --rps 200 \
  --warmup 5 \
  -X POST \
  --body '{"userId": "{{uuid}}", "ts": {{timestamp}}, "email": "{{email}}"}' \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer token123' \
  -A 'CannonLoadTester/2.0' \
  -t 5000 \
  --expect '"status":"success"' \
  -o report.json \
  --html dashboard.html \
  --csv results.csv \
  --percentiles '50,90,95,99,99.9'
```

### **6. TCP Raw Mode (Custom Binary Protocol)**

Test a TCP server that echoes bytes:

```bash
cannon --mode tcp \
  -u '127.0.0.1:9999' \
  -c 1000 \
  -w 20 \
  --body '{{number:u8}}{{value:42:u8}}{{uuid}}'
```

## **🔍 Understanding the Report**

At the end of each execution, Cannon provides a surgical analysis of your API health:

### **Terminal Report (CLI)**

* **Percentile Latency:** p50, p95, p99 (or custom) - Essential for identifying "tail latency" that averages hide.
* **ASCII Histogram:** Visual representation of latency distribution directly in the terminal.
* **Status Codes:** HTTP response distribution (2xx, 4xx, 5xx) with colors.
* **Failure Breakdown:** Error list with occurrence count ordered by frequency.
* **Network Efficiency:** Upload/download throughput in MB/s.
* **Cannon Efficiency:** Comparison between **Target RPS** vs **Actual RPS** to validate test integrity.
* **Apdex Score:** User satisfaction index (0.0 to 1.0).

### **JSON Report (`--output`)**

Exports structured metrics for CI/CD integration, dashboards, or later analysis:

```json
{
  "target": "http://localhost:8081/api/v1/accounts",
  "total_requests": 5000,
  "concurrency": 50,
  "successes": 4987,
  "failures": 13,
  "min_ms": 12.5,
  "avg_ms": 45.3,
  "p50_ms": 38.2,
  "p95_ms": 89.7,
  "p99_ms": 156.4,
  "max_ms": 523.1,
  "actual_rps": 198.6,
  "apdex_score": 0.92,
  "bytes_sent": 1048576,
  "bytes_received": 5242880,
  "duration_secs": 25.1,
  "status_codes": { "200": 4987, "500": 13 },
  "errors": { "Timeout": 5, "Connection Error": 8 }
}
```

### **HTML Dashboard (`--html`)**

Interactive dashboard with:
* Chart.js graphs for latency and status codes
* Cards with key metrics (Apdex, RPS, Successes/Failures)
* Button to export PDF
* Detailed error table
* Responsive dark theme design

### **CSV Export (`--csv`)**

Raw data from each request for analysis in external tools:

```csv
relative_time_ms,status,latency_ms,error
1234,200,45.2,
1235,200,47.8,
1236,500,123.4,"Connection Reset"
```

## **📊 Apdex System**

Cannon automatically calculates the **Application Performance Index (Apdex)**:

* **Satisfactory (≤apdex_t ms):** Full count
* **Tolerable (apdex_t+1 ms - apdex_t×4 ms):** Half point
* **Frustrating (>apdex_t×4 ms):** Zero points

**Formula:** `Apdex = (Satisfactory + Tolerable/2) / Total`

The default `apdex_t` value is 50ms, but can be customized with `--apdex-t`.

**Classification:**
* **≥0.94:** Excellent 🟢
* **≥0.85:** Good 🟢
* **≥0.70:** Fair 🟡
* **≥0.50:** Poor 🟡
* **<0.50:** Unacceptable 🔴

## **🏗️ Technical Architecture**

Cannon is built on modern distributed systems principles:

* **Producer-Consumer Pattern:** Request engine (Producer) and metrics aggregator (Consumer) communicate via **Asynchronous MPSC Channels** to eliminate memory contention and ensure thread safety.
* **Backpressure:** Strictly managed concurrency via semaphores to prevent socket exhaustion and memory spikes.
* **Zero-Copy Rendering:** Payload generator avoids unnecessary allocations with reusable buffers and direct byte formatting.
* **Polymorphic Enum:** `Target` enum with static dispatch (zero vtable overhead) for HTTP/TCP.
* **High-Precision Metrics:** Uses **HdrHistogram** to record latencies in microseconds, avoiding the "coordinated omission" problem common in legacy load testers.
* **Tokio Async Runtime:** Leverages non-blocking I/O and multiplexing for maximum efficiency.
* **TLS via rustls:** Secure and performant TLS implementation without native dependencies.
* **MiMalloc Allocator:** Reduces memory contention in high concurrency scenarios.
* **LTO Optimization:** Highly optimized binary (`lto=true`, `codegen-units=1`, `panic=abort`).

## **📋 Use Case Examples**

### **CI/CD Pipeline Integration**

```bash
# In your GitHub Actions / GitLab CI pipeline
cannon -u "$API_URL/health" -c 100 -w 10 --expect '"status":"ok"' -o results.json

# Parse JSON to validate thresholds
jq '.apdex_score >= 0.9 and .failures == 0' results.json
```

### **Soak Test (Endurance Testing)**

```bash
# 100k requests at 50 RPS constant for ~33 minutes
cannon -u http://api.prod.com/users -c 100000 --rps 50 -o soak-test.json
```

### **Spike Test (Traffic Surge)**

```bash
# Sudden spike of 500 RPS for 60 seconds
cannon -u http://api.prod.com/checkout -c 30000 --rps 500 -w 100
```

### **Cache Validation**

```bash
# First request (cache miss) vs subsequent (cache hit)
cannon -u http://cdn.example.com/assets/logo.png -c 1000 --rps 100
```

### **Binary Protocol Testing**

```bash
# Test TCP server with custom protocol
cannon --mode tcp -u '127.0.0.1:9999' -c 5000 -w 50 --body '{{value:1:u8}}{{number:u32le}}'
```

## **⚠️ Important Considerations**

* **System Limits:** Adjust `ulimit -n` (file descriptors) for high concurrency tests. For >10k workers, may need 65535+.
* **Network Bandwidth:** High RPS tests can saturate your local network connection.
* **Target Capacity:** Ensure the target can handle the generated load to avoid false positives.
* **Rate Limiting:** APIs with rate limiting may return 429 Too Many Requests during intensive tests.
* **Warm-up Usage:** Use `--warmup` when testing systems that need initial stabilization (JIT warming, connection pools, etc).
* **TCP Pool:** In TCP mode, the number of workers defines the connection pool size. Connections are aggressively reused.

## **🤝 Contributing**

Contributions are welcome! Feel free to:

1. Fork the repository
2. Create a branch for your feature (`git checkout -b feature/AmazingFeature`)
3. Commit your changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to the branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

## **📄 License**

Distributed under the MIT License. See `LICENSE` for more information.

### **👨‍💻 Author**

**Felipe Fernandes** – [LinkedIn](https://linkedin.com/in/felipefernandesss) | [GitHub](https://github.com/FelipeFelipeRenan)

*"Robust systems require relentless testing."*

---

**Version:** 2.1.0 | **Build:** LTO-optimized Release | **Edition:** Rust 2021
