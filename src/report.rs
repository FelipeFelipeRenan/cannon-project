use serde::Serialize;
use tabled::{Tabled};
use colored::Colorize;


pub struct ShotResult {
    pub success: bool,
    pub duration: std::time::Duration,
    pub status_code: Option<u16>,
    pub error: Option<String>,
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


pub fn render_ascii_histogram(hist: &hdrhistogram::Histogram<u64>){

    println!("\n{}", "📊 DISTRIBUIÇÃO DE LATÊNCIA".bold().bright_white());

    let min = hist.min();
    let max = hist.max();

    let step = (max - min) / 10;

    let step = if step == 0 {1} else {step};

    let mut max_count = 0;

    for bucket in hist.iter_linear(step){
        if bucket.count_since_last_iteration() > max_count{
            max_count = bucket.count_since_last_iteration();
        }
    }

    if max_count == 0 {return;}

    for bucket in hist.iter_linear(step){
        let count = bucket.count_since_last_iteration();
        let percent = (count as f64 / hist.len() as f64) * 100.0;

        let bar_width = (count as f64 / max_count as f64 * 30.0) as usize;
        let bar = "█".repeat(bar_width);

        println!(
            "{:>8.2}ms [{:<30}] {:>6} ({:.1}%)",
            to_ms(bucket.value_iterated_to()),
            bar.cyan(),
            count,
            percent
        );

        if bucket.value_iterated_to() >= max{ break;}
    }

}