use clap::{parser::ValueSource, ArgMatches};

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
            if !was_provided_by_cli(matches, "url") {
                args.url = Some(url);
            }
        }

        if let Some(workers) = conf.workers {
            if !was_provided_by_cli(matches, "workers") {
                args.workers = workers;
            }
        }

        if let Some(count) = conf.count {
            if !was_provided_by_cli(matches, "count") {
                args.count = count;
            }
        }

        if let Some(rps) = conf.rps {
            if !was_provided_by_cli(matches, "rps") {
                args.rps = Some(rps);
            }
        }

        if let Some(timeout) = conf.timeout {
            if !was_provided_by_cli(matches, "timeout") {
                args.timeout = timeout;
            }
        }

        if let Some(method) = conf.method {
            if !was_provided_by_cli(matches, "method") {
                args.method = method;
            }
        }

        if let Some(body) = conf.body {
            if !was_provided_by_cli(matches, "body") {
                args.body = Some(body);
            }
        }

        if let Some(expect) = conf.expect {
            if !was_provided_by_cli(matches, "expect") {
                args.expect = Some(expect);
            }
        }

        if let Some(apdex_t) = conf.apdex_t {
            if !was_provided_by_cli(matches, "apdex_t") {
                args.apdex_t = apdex_t;
            }
        }

        if let Some(insecure) = conf.insecure {
            if !was_provided_by_cli(matches, "insecure") {
                args.insecure = insecure;
            }
        }

        if let Some(csv) = conf.csv {
            if !was_provided_by_cli(matches, "csv") {
                args.csv = Some(csv);
            }
        }

        if let Some(http2) = conf.http2 {
            if !was_provided_by_cli(matches, "http2") {
                args.http2 = http2;
            }
        }

        if let Some(connect_timeout) = conf.connect_timeout {
            if !was_provided_by_cli(matches, "connect_timeout") {
                args.connect_timeout = connect_timeout;
            }
        }

        if let Some(mode) = conf.mode {
            if !was_provided_by_cli(matches, "mode") {
                args.mode = mode;
            }
        }

        if let Some(warmup) = conf.warmup {
            if !was_provided_by_cli(matches, "warmup") {
                args.warmup = warmup;
            }
        }

        if let Some(save_baseline) = conf.save_baseline {
            if !was_provided_by_cli(matches, "save_baseline") {
                args.save_baseline = Some(save_baseline);
            }
        }

        if let Some(compare_baseline) = conf.compare_baseline {
            if !was_provided_by_cli(matches, "compare_baseline") {
                args.compare_baseline = Some(compare_baseline);
            }
        }

        if let Some(tolerance) = conf.tolerance {
            if !was_provided_by_cli(matches, "tolerance") {
                args.tolerance = tolerance;
            }
        }

        if let Some(pin_threads) = conf.pin_threads {
            if !was_provided_by_cli(matches, "pin_threads") {
                args.pin_threads = pin_threads;
            }
        }

        if let Some(yaml_headers) = conf.headers {
            if !was_provided_by_cli(matches, "header") {
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
fn was_provided_by_cli(matches: &ArgMatches, id: &str) -> bool {
    matches.value_source(id) == Some(ValueSource::CommandLine)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, FromArgMatches};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn parse_args(arguments: &[&str]) -> (Args, ArgMatches) {
        let mut argv = vec!["cannon"];
        argv.extend_from_slice(arguments);

        let matches = Args::command().get_matches_from(argv);
        let args = Args::from_arg_matches(&matches).unwrap();

        (args, matches)
    }

    fn write_temp_config(contents: &str) -> String {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let path = std::env::temp_dir().join(format!("cannon-test-{timestamp}.yaml"));

        fs::write(&path, contents).unwrap();

        path.to_string_lossy().into_owned()
    }

    #[test]
    fn yaml_overrides_clap_default() {
        let config_path = write_temp_config(r#"workers: 50"#);

        let (mut args, matches) = parse_args(&["--config", &config_path]);

        merge_with_yaml(&mut args, &matches).unwrap();

        assert_eq!(args.workers, 50);

        fs::remove_file(config_path).unwrap();
    }

    #[test]
    fn cli_overrides_yaml() {
        let config_path = write_temp_config(r#"workers: 50"#);

        let (mut args, matches) = parse_args(&["--config", &config_path, "--workers", "100"]);

        merge_with_yaml(&mut args, &matches).unwrap();

        assert_eq!(args.workers, 100);

        fs::remove_file(config_path).unwrap();
    }

    #[test]
    fn cli_value_is_used_without_yaml() {
        let (mut args, matches) = parse_args(&["--workers", "100"]);

        merge_with_yaml(&mut args, &matches).unwrap();

        assert_eq!(args.workers, 100);
    }

    #[test]
    fn clap_default_is_used_without_yaml_or_cli() {
        let (mut args, matches) = parse_args(&[]);

        merge_with_yaml(&mut args, &matches).unwrap();

        assert_eq!(args.workers, 10);
    }
}
