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

        if let Some(max_error_rate) = conf.max_error_rate {
            if !was_provided_by_cli(matches, "max_error_rate") {
                args.max_error_rate = Some(max_error_rate);
            }
        }

        if let Some(tcp_read_bytes) = conf.tcp_read_bytes {
            if !was_provided_by_cli(matches, "tcp_read_bytes")
                && !was_provided_by_cli(matches, "tcp_read_until")
            {
                args.tcp_read_bytes = Some(tcp_read_bytes);
            }
        }

        if let Some(tcp_read_until) = conf.tcp_read_until {
            if !was_provided_by_cli(matches, "tcp_read_until")
                && !was_provided_by_cli(matches, "tcp_read_bytes")
            {
                args.tcp_read_until = Some(tcp_read_until);
            }
        }

        if let Some(pin_threads) = conf.pin_threads {
            if !was_provided_by_cli(matches, "pin_threads") {
                args.pin_threads = pin_threads;
            }
        }

        if let Some(yaml_headers) = conf.headers {
            // The clap id is the field name (`headers`); `header` is only the
            // long flag. Using the flag name here used to panic.
            if !was_provided_by_cli(matches, "headers") {
                args.headers = yaml_headers;
            } else {
                let mut merged = yaml_headers;
                merged.extend(args.headers.iter().cloned());
                args.headers = merged;
            }
        }

        if let Some(ramp_up) = conf.ramp_up {
            if !was_provided_by_cli(matches, "ramp_up") {
                args.ramp_up = Some(ramp_up);
            }
        }

        if let Some(user_agent) = conf.user_agent {
            if !was_provided_by_cli(matches, "user_agent") {
                args.user_agent = user_agent;
            }
        }

        if let Some(output) = conf.output {
            if !was_provided_by_cli(matches, "output") {
                args.output = Some(output);
            }
        }

        if let Some(html) = conf.html {
            if !was_provided_by_cli(matches, "html") {
                args.html = Some(html);
            }
        }

        if let Some(percentiles) = conf.percentiles {
            if !was_provided_by_cli(matches, "percentiles") {
                args.percentiles = percentiles;
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
    fn ramp_up_is_a_named_flag() {
        let (args, _) = parse_args(&["--rps", "100", "--ramp-up", "5s"]);

        assert_eq!(args.ramp_up.as_deref(), Some("5s"));
    }

    #[test]
    fn ramp_up_can_come_from_yaml() {
        let config_path = write_temp_config("rps: 100\nramp_up: 10s\n");

        let (mut args, matches) = parse_args(&["--config", &config_path]);

        merge_with_yaml(&mut args, &matches).unwrap();

        assert_eq!(args.ramp_up.as_deref(), Some("10s"));
        assert!(args.validate().is_ok());

        fs::remove_file(config_path).unwrap();
    }

    #[test]
    fn cli_ramp_up_overrides_yaml() {
        let config_path = write_temp_config("rps: 100\nramp_up: 10s\n");

        let (mut args, matches) = parse_args(&["--config", &config_path, "--ramp-up", "2s"]);

        merge_with_yaml(&mut args, &matches).unwrap();

        assert_eq!(args.ramp_up.as_deref(), Some("2s"));

        fs::remove_file(config_path).unwrap();
    }

    #[test]
    fn report_options_can_come_from_yaml() {
        let config_path = write_temp_config(
            "output: out.json\nhtml: out.html\npercentiles: \"50,99.9\"\nuser_agent: test/1\n",
        );

        let (mut args, matches) = parse_args(&["--config", &config_path]);

        merge_with_yaml(&mut args, &matches).unwrap();

        assert_eq!(args.output.as_deref(), Some("out.json"));
        assert_eq!(args.html.as_deref(), Some("out.html"));
        assert_eq!(args.percentiles, "50,99.9");
        assert_eq!(args.user_agent, "test/1");

        fs::remove_file(config_path).unwrap();
    }

    #[test]
    fn yaml_headers_do_not_panic_and_are_applied() {
        let config_path =
            write_temp_config("headers:\n  - \"Authorization: Bearer x\"\n  - \"Accept: */*\"\n");

        let (mut args, matches) = parse_args(&["--config", &config_path]);

        merge_with_yaml(&mut args, &matches).unwrap();

        assert_eq!(args.headers, ["Authorization: Bearer x", "Accept: */*"]);

        fs::remove_file(config_path).unwrap();
    }

    #[test]
    fn yaml_and_cli_headers_are_combined() {
        let config_path = write_temp_config("headers:\n  - \"A: 1\"\n");

        let (mut args, matches) = parse_args(&["--config", &config_path, "-H", "B: 2"]);

        merge_with_yaml(&mut args, &matches).unwrap();

        assert_eq!(args.headers, ["A: 1", "B: 2"]);

        fs::remove_file(config_path).unwrap();
    }

    #[test]
    fn every_supported_yaml_key_merges_without_panicking() {
        let config_path = write_temp_config(
            r#"
url: "http://localhost:8080"
mode: http
method: POST
headers: ["A: 1"]
body: "{}"
expect: ok
workers: 3
count: 7
rps: 10
ramp_up: 1s
warmup: 1
timeout: 1000
connect_timeout: 500
user_agent: ua
insecure: true
http2: true
percentiles: "50,99"
apdex_t: 100
output: o.json
html: o.html
csv: o.csv
save_baseline: b.json
compare_baseline: c.json
tolerance: 7.5
max_error_rate: 1.5
pin_threads: true
"#,
        );

        let (mut args, matches) = parse_args(&["--config", &config_path]);

        merge_with_yaml(&mut args, &matches).unwrap();

        assert_eq!(args.workers, 3);
        assert_eq!(args.count, 7);
        assert_eq!(args.tolerance, 7.5);
        assert!(args.pin_threads && args.http2 && args.insecure);

        fs::remove_file(config_path).unwrap();
    }

    #[test]
    fn clap_default_is_used_without_yaml_or_cli() {
        let (mut args, matches) = parse_args(&[]);

        merge_with_yaml(&mut args, &matches).unwrap();

        assert_eq!(args.workers, 10);
    }
}
