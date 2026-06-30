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
