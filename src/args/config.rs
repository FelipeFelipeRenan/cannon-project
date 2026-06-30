use super::parser::{Args, FileConfig};

pub fn merge_with_yaml(args: &mut Args) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(config_path) = &args.config {
        let yaml_str = std::fs::read_to_string(config_path)?;
        let conf: FileConfig = serde_yaml::from_str(&yaml_str)?;
        // Regra de Ouro: O YAML sobrepõe os valores padrão da CLI
        if conf.url.is_some() {
            args.url = conf.url;
        }
        if let Some(w) = conf.workers {
            args.workers = w;
        }
        if let Some(c) = conf.count {
            args.count = c;
        }
        if let Some(rps) = conf.rps {
            args.rps = Some(rps);
        }
        if let Some(t) = conf.timeout {
            args.timeout = t;
        }
        if let Some(m) = conf.method {
            args.method = m;
        }
        if let Some(body) = conf.body {
            args.body = Some(body);
        }
        if let Some(exp) = conf.expect {
            args.expect = Some(exp);
        }
        if let Some(apdex) = conf.apdex_t {
            args.apdex_t = apdex;
        }
        if let Some(ins) = conf.insecure {
            args.insecure = ins;
        }
        if let Some(csv_path) = conf.csv {
            args.csv = Some(csv_path);
        }
        if let Some(h2) = conf.http2 {
            args.http2 = h2;
        }
        if let Some(ct) = conf.connect_timeout {
            args.connect_timeout = ct;
        }

        if let Some(mut yaml_headers) = conf.headers {
            yaml_headers.append(&mut args.headers);
            args.headers = yaml_headers;
        }
        if let Some(mode) = conf.mode {
            args.mode = mode;
        }
    }

    Ok(())
}
