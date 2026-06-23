use crate::args::parser::Args;
use reqwest::Client;
use std::time::Duration;

pub fn build_optimized_client(args: &Args) -> Result<Client, reqwest::Error> {
    let mut builder = Client::builder()
        .tcp_nodelay(true)
        .tcp_keepalive(Duration::from_secs(60))
        .pool_max_idle_per_host(args.workers as usize)
        .pool_idle_timeout(Some(Duration::from_secs(90)))
        .user_agent(&args.user_agent)
        .connect_timeout(Duration::from_millis(args.connect_timeout))
        .timeout(Duration::from_millis(args.timeout));

    if args.insecure {
        builder = builder.danger_accept_invalid_certs(true);
    }

    if args.http2 {
        builder = builder.http2_prior_knowledge();
    }

    builder.build()
}
