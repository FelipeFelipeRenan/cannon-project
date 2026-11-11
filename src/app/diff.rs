//! `--diff BEFORE AFTER`: prints how two saved results differ.

use super::{finish::EXIT_GATE_FAILED, AppResult};
use crate::report::diff::{compare, Side};
use std::process::ExitCode;

/// Compares two result files and returns the gate exit code (0, or 2 on a
/// regression).
///
/// # Errors
///
/// Returns an error if a file cannot be read or is not a Cannon result.
pub fn run(files: &[String], tolerance: f64) -> AppResult<ExitCode> {
    let [before_path, after_path] = files else {
        return Err("--diff needs exactly two files: BEFORE AFTER".into());
    };

    let load = |path: &String| -> AppResult<Side> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("cannot read '{path}': {error}"))?;

        Ok(Side::parse(&text).map_err(|error| format!("'{path}': {error}"))?)
    };

    let diff = compare(&load(before_path)?, &load(after_path)?, tolerance);

    println!("before: {before_path}\nafter:  {after_path}\n");

    for line in &diff.lines {
        println!("{line}");
    }

    println!();

    for note in &diff.notes {
        println!("{note}");
    }

    if diff.failures.is_empty() {
        println!("✅ no regression (p99 tolerance {tolerance:.2}%)");
        return Ok(ExitCode::SUCCESS);
    }

    for failure in &diff.failures {
        println!("{failure}");
    }

    Ok(ExitCode::from(EXIT_GATE_FAILED))
}
