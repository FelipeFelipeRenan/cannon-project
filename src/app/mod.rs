//! Application layer: everything between the parsed command line and the
//! process exit code.
//!
//! `main.rs` only parses arguments and reports errors; the steps of a run live
//! here, one module each, so that they can be reused (for example by future
//! coordinator/agent modes) and tested separately:
//!
//! 1. [`environment`]: process-level setup and the Tokio runtime;
//! 2. [`plan`]: validation and everything loaded before the test starts;
//! 3. [`session`]: the run itself (engine, progress, signals);
//! 4. [`finish`]: summary, report files and the performance gate.

pub mod csv_export;
pub mod environment;
pub mod finish;
pub mod plan;
pub mod session;
pub mod signals;
pub mod update;

use crate::args::parser::Args;
use colored::Colorize;
use plan::RunPlan;
use std::process::ExitCode;
use std::time::Duration;

/// Result type used across the application layer.
pub type AppResult<T> = Result<T, Box<dyn std::error::Error>>;

/// Runs a complete load test and returns the process exit code.
///
/// # Errors
///
/// Returns an error for invalid configuration, an unreachable target, an
/// unwritable output file or any failure while running or reporting.
pub async fn run(args: Args) -> AppResult<ExitCode> {
    // Fail fast: everything that can be checked before the (possibly long)
    // test starts is checked here, not after it.
    let plan = RunPlan::prepare(&args)?;

    print_intro(&args, &plan);

    let target = plan.build_target(&args).await?;

    let completed = session::run(&args, &plan, target).await?;

    finish::finish(&args, &plan, completed)
}

/// Prints the banner and a description of what is about to run.
fn print_intro(args: &Args, plan: &RunPlan) {
    crate::report::cli::print_banner();

    println!("🎯 Target: {}", plan.url.bright_cyan().bold());
    println!(
        "🚀 {}",
        format!(
            "Preparing the Cannon for {} shots with {} workers...",
            args.count.to_string().cyan(),
            args.workers.to_string().magenta()
        )
        .bold()
    );

    println!("⏱️ Timeout: {}ms", args.timeout.to_string().yellow());

    match args.rps {
        Some(rps) => println!(
            "📐 Load: open-loop at {} req/s (latency measured from the scheduled send time)",
            rps.to_string().yellow()
        ),
        None => println!("📐 Load: closed-loop, as fast as the workers can go"),
    }

    if plan.warmup > Duration::ZERO {
        println!(
            "🔥 Warm-up Mode active: Disregarding the first {}s of metrics...",
            args.warmup.to_string().yellow()
        );
    }
}
