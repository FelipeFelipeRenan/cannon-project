use clap::ArgMatches;

use super::parser::{Args, FileConfig};

/// Merges values from a YAML configuration file into the parsed arguments.
///
/// Command-line arguments take precedence over values loaded from the
/// configuration file.
///
/// # Errors
///
/// Returns an error if the configuration file cannot be read or its contents
/// cannot be parsed.
pub fn merge_with_yaml(
    args: &mut Args,
    matches: &ArgMatches,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(config_path) = &args.config {
        let yaml_str = std::fs::read_to_string(config_path)?;
        let conf: FileConfig = serde_yaml::from_str(&yaml_str)?;

        if let Some(url) = conf.url {
            if !matches.contains_id("url") {
                args.url = Some(url);
            }
        }

        if let Some(workers) = conf.workers {
            if !matches.contains_id("workers") {
                args.workers = workers;
            }
        }

        if let Some(count) = conf.count {
            if !matches.contains_id("count") {
                args.count = count;
            }
        }

        if let Some(rps) = conf.rps {
            if !matches.contains_id("rps") {
                args.rps = Some(rps);
            }
        }

        if let Some(timeout) = conf.timeout {
            if !matches.contains_id("timeout") {
                args.timeout = timeout;
            }
        }

        if let Some(method) = conf.method {
            if !matches.contains_id("method") {
                args.method = method;
            }
        }

        if let Some(body) = conf.body {
            if !matches.contains_id("body") {
                args.body = Some(body);
            }
        }

        if let Some(expect) = conf.expect {
            if !matches.contains_id("expect") {
                args.expect = Some(expect);
            }
        }

        if let Some(apdex_t) = conf.apdex_t {
            if !matches.contains_id("apdex_t") {
                args.apdex_t = apdex_t;
            }
        }

        if let Some(insecure) = conf.insecure {
            if !matches.contains_id("insecure") {
                args.insecure = insecure;
            }
        }

        if let Some(csv) = conf.csv {
            if !matches.contains_id("csv") {
                args.csv = Some(csv);
            }
        }

        if let Some(http2) = conf.http2 {
            if !matches.contains_id("http2") {
                args.http2 = http2;
            }
        }

        if let Some(connect_timeout) = conf.connect_timeout {
            if !matches.contains_id("connect_timeout") {
                args.connect_timeout = connect_timeout;
            }
        }

        if let Some(mode) = conf.mode {
            if !matches.contains_id("mode") {
                args.mode = mode;
            }
        }

        if let Some(warmup) = conf.warmup {
            if !matches.contains_id("warmup") {
                args.warmup = warmup;
            }
        }

        if let Some(save_baseline) = conf.save_baseline {
            if !matches.contains_id("save_baseline") {
                args.save_baseline = Some(save_baseline);
            }
        }

        if let Some(compare_baseline) = conf.compare_baseline {
            if !matches.contains_id("compare_baseline") {
                args.compare_baseline = Some(compare_baseline);
            }
        }

        if let Some(tolerance) = conf.tolerance {
            if !matches.contains_id("tolerance") {
                args.tolerance = tolerance;
            }
        }

        if let Some(pin_threads) = conf.pin_threads {
            if !matches.contains_id("pin_threads") {
                args.pin_threads = pin_threads;
            }
        }

        if let Some(yaml_headers) = conf.headers {
            if !matches.contains_id("header") {
                args.headers = yaml_headers;
            } else {
                let mut merged = yaml_headers;
                merged.extend(args.headers.iter().cloned());
                args.headers = merged;
            }
        }
    }

    Ok(())
}
