use colored::Colorize;

/// Validates and extracts the target URL from an optional value.
///
/// The URL must use either the `http://` or `https://` scheme. If no URL is
/// provided or the URL uses an unsupported scheme, an error message is printed
/// and the process exits with a non-zero status.
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
