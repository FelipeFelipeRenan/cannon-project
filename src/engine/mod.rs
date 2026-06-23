pub mod worker;

// Re-exporta a função principal para não quebrar o main.rs
pub use worker::run_workers;
