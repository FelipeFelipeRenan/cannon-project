use serde::Serialize;
use tabled::{Tabled};

pub struct ShotResult {
    pub success: bool,
    pub duration: std::time::Duration,
}

#[derive(Serialize, Tabled)]
pub struct FinalReport {

    #[tabled(rename = "Target URL")]
    pub target: String,
    #[tabled(rename = "Total")]
    pub total_requests: u32,
    #[tabled(rename = "Workers")]
    pub concurrency: u32,
    #[tabled(rename = "Success")]
    pub successes: u64,
    #[tabled(rename = "Failures")]
    pub failures: u64,
    pub min_ms: f64,
    pub avg_ms: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub max_ms: f64,
}

#[derive(Tabled)]
pub struct LatencyMetrics {
    pub metric: String,
    pub value: String,
}

pub fn to_ms(us: u64) -> f64 {
    us as f64 / 1000.0
}