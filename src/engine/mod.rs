//! Load-generation engine and worker orchestration.
//!
//! This module coordinates workers, scheduling, request execution, and
//! aggregate metrics during a load test.

/// Worker execution and metrics collection.
pub mod worker;

// Re-exports the main function to avoid breaking main.rs
pub use worker::run_workers;
