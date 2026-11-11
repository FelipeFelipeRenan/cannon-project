//! Diagnostics that do not generate load: `--print-config` and `--check`.

use super::{plan::RunPlan, AppResult};
use crate::args::parser::Args;
use colored::Colorize;
use std::process::ExitCode;

/// Prints the effective configuration (after merging the config file and the
/// command line). Header values are hidden because they often hold secrets.
pub fn print_config(args: &Args, plan: &RunPlan) {
    let mut lines: Vec<(&str, String)> = vec![
        ("mode", args.mode.clone()),
        ("url", plan.url.clone()),
        ("count", args.count.to_string()),
        ("workers", args.workers.to_string()),
        (
            "load",
            match args.rps {
                Some(rps) => format!("open-loop, {rps} req/s"),
                None => "closed-loop".to_string(),
            },
        ),
        ("warmup", format!("{}s", args.warmup)),
        (
            "ramp_up",
            args.ramp_up.clone().unwrap_or_else(|| "none".to_string()),
        ),
        ("timeout", format!("{}ms", args.timeout)),
        ("connect_timeout", format!("{}ms", args.connect_timeout)),
        (
            "threads",
            args.threads
                .map_or_else(|| "one per core".to_string(), |n| n.to_string()),
        ),
        ("pin_threads", args.pin_threads.to_string()),
    ];

    if plan.is_tcp {
        lines.push((
            "tcp_read",
            match (&args.tcp_read_until, args.tcp_read_bytes) {
                (Some(delimiter), _) => format!("until {delimiter:?}"),
                (None, bytes) => format!("{} byte(s)", bytes.unwrap_or(1)),
            },
        ));
    } else {
        lines.push(("method", args.method.clone()));
        lines.push((
            "headers",
            if args.headers.is_empty() {
                "none".to_string()
            } else {
                args.headers
                    .iter()
                    .map(|header| header.split(':').next().unwrap_or("").trim().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
                    + " (values hidden)"
            },
        ));
        lines.push(("http2", args.http2.to_string()));
        lines.push(("insecure", args.insecure.to_string()));
        lines.push(("max_redirects", args.max_redirects.to_string()));
        lines.push((
            "expect_status",
            args.expect_status
                .clone()
                .unwrap_or_else(|| "2xx".to_string()),
        ));
    }

    lines.push((
        "body",
        if plan.template.is_some() {
            "template".to_string()
        } else {
            "none".to_string()
        },
    ));
    lines.push((
        "expect",
        args.expect.clone().unwrap_or_else(|| "none".to_string()),
    ));
    lines.push((
        "abort_on_error_rate",
        args.abort_on_error_rate
            .map_or_else(|| "off".to_string(), |limit| format!("{limit}%")),
    ));

    for (name, value) in lines {
        println!("{:<20} {}", name.bold(), value);
    }
}

/// Fires a single request and reports the outcome, without generating load.
///
/// # Errors
///
/// Returns an error if the target cannot be set up at all (for example the
/// HTTP client cannot be built); a failing request is reported and gives a
/// failure exit code instead.
pub async fn run(args: &Args, plan: &RunPlan) -> AppResult<ExitCode> {
    println!("{} configuration is valid", "✔".green());
    println!("{} target: {}", "•".cyan(), plan.url.bold());

    // One connection is all a check needs.
    let mut single = args.clone();
    single.workers = 1;

    let target = match plan.build_target(&single).await {
        Ok(target) => target,
        Err(error) => {
            println!("{} cannot reach the target: {error}", "✘".red());
            return Ok(ExitCode::FAILURE);
        }
    };

    let mut payload = Vec::new();
    if let Some(template) = &plan.template {
        template.render(&mut payload);
    }

    let result = target.fire(&payload).await;
    let latency = result.duration.as_secs_f64() * 1000.0;

    if result.success {
        let status = result
            .status_code
            .map_or_else(String::new, |code| format!(" status {code},"));
        println!(
            "{} request succeeded:{status} {latency:.1} ms, {} B sent, {} B received",
            "✔".green(),
            result.bytes_sent,
            result.bytes_received
        );
        Ok(ExitCode::SUCCESS)
    } else {
        let error = result.error.unwrap_or_else(|| "unknown error".to_string());
        let status = result
            .status_code
            .map_or_else(String::new, |code| format!(" (status {code})"));
        println!(
            "{} request failed{status} after {latency:.1} ms: {error}",
            "✘".red()
        );
        if !result.assertion_success {
            println!("  the response did not match --expect");
        }
        Ok(ExitCode::FAILURE)
    }
}
