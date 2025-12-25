use serde::Serialize;

pub struct ShotResult {
    pub success: bool,
    pub duration: std::time::Duration,
}

#[derive(Serialize)]
pub struct FinalReport {
    pub target: String,
    pub total_requests: u32,
    pub concurrency: u32,
    pub successes: u64,
    pub failures: u64,
    pub min_ms: f64,
    pub avg_ms: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub max_ms: f64,
}

pub fn to_ms(us: u64) -> f64 {
    us as f64 / 1000.0
}