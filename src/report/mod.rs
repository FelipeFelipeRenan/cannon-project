//! Reporting and result presentation for Cannon.
//!
//! This module contains the data structures and utilities used to summarize,
//! visualize, and export load-test results.

/// Command-line reporting utilities.
pub mod cli;

/// Pass/fail evaluation for CI pipelines (baselines and error-rate limits).
pub mod gate;

/// Aggregation of worker results into report-ready numbers.
pub mod summary;
