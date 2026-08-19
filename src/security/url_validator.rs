use colored::Colorize;

pub fn validate_and_extract(url_option: &Option<String>) -> String {
    let url_str = url_option.clone().unwrap_or_else(|| {
        eprintln!(
            "{} You must provide a URL via the flag (-u) or in the YAML file. (--config)",
            "❌ Error:".red().bold()
        );
        std::process::exit(1);
    });

    if !url_str.starts_with("http://") && !url_str.starts_with("https://") {
        eprintln!(
            "{} The target URL must begin with http:// or https://",
            "❌ Critical Error:".red().bold()
        );
        std::process::exit(1);
    }

    url_str
}
