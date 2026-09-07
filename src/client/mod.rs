//! Network clients used by Cannon.
//!
//! This module provides the protocol-specific clients and the unified target
//! abstraction used by the load-generation engine.

/// HTTP client construction and configuration.
pub mod http;

/// Protocol-independent load-testing targets.
pub mod target;
