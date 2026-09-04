//! # Cannon
//!
//! High-performance load testing for HTTP and raw TCP services.
//!
//! Cannon is designed to generate predictable load while keeping the
//! overhead of the load generator itself low. It provides configurable
//! concurrency, constant-RPS scheduling, warm-up phases, dynamic payload
//! generation, and latency reporting.
//!
//! ## Core concepts
//!
//! - [`engine`] — coordinates workers, scheduling, and metrics collection.
//! - [`client`] — sends requests to HTTP and TCP targets.
//! - [`payload`] — generates reusable dynamic request payloads.
//! - [`report`] — presents and exports test results.
//!
//! The `cannon` binary is the primary interface for running load tests.
//! The library modules expose the underlying building blocks used by the
//! executable.

pub mod args;
pub mod client;
pub mod engine;
pub mod payload;
pub mod report;
pub mod security;
