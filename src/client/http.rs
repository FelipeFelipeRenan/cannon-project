use crate::args::parser::Args;
use reqwest::Client;
use std::time::Duration;

/// Builds the HTTP client used by Cannon.
///
/// The client is configured according to the supplied command-line arguments
/// and is reused across requests to benefit from connection pooling.
///
/// # Errors
///
/// Returns an error if the underlying HTTP client cannot be constructed.
pub fn build_optimized_client(args: &Args) -> Result<Client, String> {
    let mut builder = Client::builder()
        .tcp_nodelay(true)
        .tcp_keepalive(Duration::from_secs(60))
        .pool_max_idle_per_host(args.workers as usize)
        .pool_idle_timeout(Some(Duration::from_secs(90)))
        // Redirects would silently multiply the traffic (each one is another
        // request) while being reported as a single response, so they are
        // surfaced as the 3xx responses they are.
        .redirect(if args.max_redirects == 0 {
            reqwest::redirect::Policy::none()
        } else {
            reqwest::redirect::Policy::limited(args.max_redirects)
        })
        .user_agent(&args.user_agent)
        .connect_timeout(Duration::from_millis(args.connect_timeout))
        .timeout(Duration::from_millis(args.timeout));

    if args.no_proxy {
        builder = builder.no_proxy();
    }

    if args.insecure {
        builder = builder.danger_accept_invalid_certs(true);
    }

    if args.http2 {
        builder = builder.http2_prior_knowledge();
    }

    if let Some(path) = &args.cacert {
        let pem = std::fs::read(path)
            .map_err(|error| format!("cannot read --cacert '{path}': {error}"))?;

        let certificates = reqwest::Certificate::from_pem_bundle(&pem)
            .map_err(|error| format!("invalid --cacert '{path}': {error}"))?;

        for certificate in certificates {
            builder = builder.add_root_certificate(certificate);
        }
    }

    if let (Some(cert), Some(key)) = (&args.cert, &args.key) {
        let mut pem =
            std::fs::read(cert).map_err(|error| format!("cannot read --cert '{cert}': {error}"))?;

        pem.push(b'\n');
        pem.extend(
            std::fs::read(key).map_err(|error| format!("cannot read --key '{key}': {error}"))?,
        );

        let identity = reqwest::Identity::from_pem(&pem)
            .map_err(|error| format!("invalid --cert/--key pair: {error}"))?;

        builder = builder.identity(identity);
    }

    builder
        .build()
        .map_err(|error| format!("cannot build the HTTP client: {error}"))
}
