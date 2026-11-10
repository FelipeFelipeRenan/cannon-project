//! After the run: terminal summary, report files, baseline and the
//! pass/fail decision of the performance gate.

use super::plan::RunPlan;
use super::session::Completed;
use super::AppResult;
use crate::args::parser::Args;
use crate::report::cli::{generate_html_report, print_summary, FinalReport};
use crate::report::gate::{self, Baseline, GateOptions};
use crate::report::summary::RunSummary;
use colored::Colorize;
use std::process::ExitCode;

/// Exit status used when a performance gate (regression or error rate) fails.
pub const EXIT_GATE_FAILED: u8 = 2;

/// Prints the summary, writes the requested files and applies the gate.
///
/// Returns the process exit code: success, [`EXIT_GATE_FAILED`] or the
/// status of the signal that interrupted the run.
///
/// # Errors
///
/// Returns an error if the results cannot be merged or a report file cannot
/// be written.
pub fn finish(args: &Args, plan: &RunPlan, completed: Completed) -> AppResult<ExitCode> {
    let Completed {
        worker_results,
        duration,
        metrics,
        interrupted_status,
        aborted,
    } = completed;

    let summary = RunSummary::aggregate(worker_results, &metrics, duration, args.rps.is_some())?;

    print_summary(
        &summary,
        args.count,
        args.rps,
        &plan.percentiles,
        args.apdex_t,
    );

    // Reports are always written, even when the gate fails, so that CI can
    // attach them to the failed job.
    let report = FinalReport::new(
        &summary,
        &plan.url,
        args.workers,
        args.rps,
        args.apdex_t,
        &plan.percentiles,
    );

    if args.output.is_some() || args.html.is_some() {
        let json_data = serde_json::to_string_pretty(&report)?;

        if let Some(path) = &args.output {
            write_atomic(path, json_data.as_bytes())
                .map_err(|error| format!("cannot write JSON report '{path}': {error}"))?;
            println!(
                "📂 JSON report saved successfully to {}!",
                path.bright_cyan()
            );
        }

        if let Some(path) = &args.html {
            generate_html_report(path, &json_data)
                .map_err(|error| format!("cannot write HTML report '{path}': {error}"))?;
            println!(
                "🌐 HTML report saved successfully to {}!",
                path.bright_cyan()
            );
        }
    }

    let mut gate_failures = Vec::new();

    if let Some(path) = &args.save_baseline {
        match Baseline::from_summary(&summary) {
            Ok(baseline) => {
                write_atomic(path, serde_json::to_string_pretty(&baseline)?.as_bytes())
                    .map_err(|error| format!("cannot write baseline '{path}': {error}"))?;

                println!(
                    "\n💾 Performance baseline saved successfully to: {}",
                    path.bright_green()
                );
            }
            Err(reason) => gate_failures.push(format!("❌ Baseline not saved: {reason}")),
        }
    }

    if let Some(reason) = aborted {
        println!("\n{}", reason.red().bold());
        eprintln!("{} test aborted early", "error:".red().bold());

        return Ok(ExitCode::from(EXIT_GATE_FAILED));
    }

    if let Some(status) = interrupted_status {
        println!("\nGate checks are skipped because the run was interrupted.");

        return Ok(ExitCode::from(status));
    }

    let outcome = gate::evaluate(
        &summary,
        plan.baseline.as_ref(),
        &GateOptions {
            tolerance_percent: args.tolerance,
            max_error_rate_percent: args.max_error_rate,
        },
    );

    for note in &outcome.notes {
        println!("\n{note}");
    }

    gate_failures.extend(outcome.failures);

    if gate_failures.is_empty() {
        return Ok(ExitCode::SUCCESS);
    }

    for failure in &gate_failures {
        println!("\n{}", failure.red().bold());
    }

    eprintln!("{} performance gate failed", "error:".red().bold());

    Ok(ExitCode::from(EXIT_GATE_FAILED))
}

/// Writes a file so that readers (and a crash) never see a half-written one:
/// the contents go to a temporary file next to the target, which is then
/// renamed over it. This matters for baselines, which CI compares against.
fn write_atomic(path: &str, contents: &[u8]) -> std::io::Result<()> {
    let temporary = format!("{path}.tmp-{}", std::process::id());

    let result =
        std::fs::write(&temporary, contents).and_then(|()| std::fs::rename(&temporary, path));

    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::write_atomic;

    #[test]
    fn atomic_write_replaces_the_file_and_leaves_no_temporary() {
        let dir = std::env::temp_dir().join(format!("cannon-atomic-{}", std::process::id()));

        std::fs::create_dir_all(&dir).unwrap();

        let path = dir.join("baseline.json");
        let path = path.to_str().unwrap();

        write_atomic(path, b"first").unwrap();
        write_atomic(path, b"second").unwrap();

        assert_eq!(std::fs::read_to_string(path).unwrap(), "second");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);

        std::fs::remove_dir_all(dir).unwrap();
    }
}
