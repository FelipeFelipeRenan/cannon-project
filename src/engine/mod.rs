pub mod worker;

// Re-exports the main function to avoid breaking main.rs
pub use worker::run_workers;
