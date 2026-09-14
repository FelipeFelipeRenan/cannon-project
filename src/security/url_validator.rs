use url::Url;

/// Validates and extracts the target URL from an optional value.
///
/// The URL must use either the `http://` or `https://` scheme. If no URL is
/// provided or the URL uses an unsupported scheme, an error message is printed
/// and the process exits with a non-zero status.
pub fn validate_and_extract(url_option: &Option<String>) -> Result<String, String> {
    let url_str = url_option.as_ref().ok_or_else(|| {
        "You must provide a URL via the flag (-u) or in the YAML file. (--config)".to_string()
    })?;

    let url = Url::parse(url_str).map_err(|error| format!("Invalid target URL: {error}"))?;

    match url.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(format!(
                "Unsupported URL scheme '{scheme}'. Only http:// and https:// are supported."
            ));
        }
    }

    if url.host_str().is_none() {
        return Err("Target URL must contain a valid host".to_string());
    }

    Ok(url_str.clone())
}

#[cfg(test)]
mod tests {
    use super::validate_and_extract;

    #[test]
    fn accepts_http_url() {
        let url = Some("http://example.com".to_string());

        assert_eq!(validate_and_extract(&url).unwrap(), "http://example.com");
    }

    #[test]
    fn accepts_https_url_with_path() {
        let url = Some("https://example.com/api/v1".to_string());

        assert_eq!(
            validate_and_extract(&url).unwrap(),
            "https://example.com/api/v1"
        );
    }

    #[test]
    fn rejects_missing_url() {
        let url = None;

        let error = validate_and_extract(&url).unwrap_err();

        assert!(error.contains("You must provide a URL"));
    }

    #[test]
    fn rejects_unsupported_scheme() {
        let url = Some("ftp://example.com".to_string());

        let error = validate_and_extract(&url).unwrap_err();

        assert!(error.contains("Unsupported URL scheme 'ftp'"));
    }

    #[test]
    fn rejects_malformed_url() {
        let url = Some("not a valid url".to_string());

        let error = validate_and_extract(&url).unwrap_err();

        assert!(error.contains("Invalid target URL"));
    }

    #[test]
    fn parses_userinfo_as_part_of_url() {
        let url = Some("http://localhost@evil.com".to_string());

        let parsed = validate_and_extract(&url).unwrap();

        assert_eq!(parsed, "http://localhost@evil.com");
    }

    #[test]
    fn rejects_url_without_host() {
        let url = Some("http://".to_string());

        let error = validate_and_extract(&url).unwrap_err();

        assert!(
            error.contains("Invalid target URL")
                || error.contains("Target URL must contain a valid host")
        );
    }
}
