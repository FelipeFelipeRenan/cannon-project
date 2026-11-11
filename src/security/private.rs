//! `--block-private`: refuse targets that resolve to internal addresses.
//!
//! Useful when Cannon runs somewhere it must not be able to reach the local
//! network (a shared runner, or an agent taking targets from someone else):
//! loopback, private ranges, link-local (including cloud metadata services at
//! `169.254.169.254`), carrier-grade NAT and unique-local IPv6.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};

/// Whether `ip` is not a publicly routable unicast address.
#[must_use]
pub fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_private_v4(ip),
        IpAddr::V6(ip) => is_private_v6(ip),
    }
}

fn is_private_v4(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();

    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
        || (a == 100 && (64..=127).contains(&b)) // carrier-grade NAT, 100.64.0.0/10
        || a == 0
}

fn is_private_v6(ip: Ipv6Addr) -> bool {
    if let Some(mapped) = ip.to_ipv4_mapped() {
        return is_private_v4(mapped);
    }

    let first = ip.segments()[0];

    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (first & 0xfe00) == 0xfc00 // unique local, fc00::/7
        || (first & 0xffc0) == 0xfe80 // link-local, fe80::/10
}

/// Whether a host name or literal refers to the local machine or an internal
/// address without needing DNS.
#[must_use]
pub fn is_private_literal(host: &str) -> bool {
    let host = host.trim_start_matches('[').trim_end_matches(']');

    if let Ok(ip) = host.parse::<IpAddr>() {
        return is_private_ip(ip);
    }

    let host = host.trim_end_matches('.').to_ascii_lowercase();

    host == "localhost" || host.ends_with(".localhost")
}

/// Checks that `host` (a name or literal) and every address it resolves to
/// are public.
///
/// # Errors
///
/// Returns an explanation naming the blocked address, or the DNS failure.
pub fn ensure_public(host: &str, port: u16) -> Result<(), String> {
    if is_private_literal(host) {
        return Err(format!(
            "--block-private: '{host}' is a local or private address"
        ));
    }

    let bare = host.trim_start_matches('[').trim_end_matches(']');
    let addresses: Vec<SocketAddr> = (bare, port)
        .to_socket_addrs()
        .map_err(|error| format!("cannot resolve '{host}': {error}"))?
        .collect();

    if let Some(blocked) = addresses.iter().find(|a| is_private_ip(a.ip())) {
        return Err(format!(
            "--block-private: '{host}' resolves to the private address {}",
            blocked.ip()
        ));
    }

    Ok(())
}

/// Splits a `host:port` TCP address (IPv6 literals in brackets).
#[must_use]
pub fn split_host_port(address: &str) -> Option<(&str, u16)> {
    let (host, port) = address.rsplit_once(':')?;

    Some((host, port.parse().ok()?))
}

/// DNS resolver for the HTTP client that drops private addresses, so that
/// redirects and DNS rebinding cannot reach internal hosts either.
pub struct PublicOnlyResolver;

impl reqwest::dns::Resolve for PublicOnlyResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        Box::pin(async move {
            let host = name.as_str().to_string();

            let addresses: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), 0))
                .await?
                .filter(|address| !is_private_ip(address.ip()))
                .collect();

            if addresses.is_empty() {
                let error: Box<dyn std::error::Error + Send + Sync> =
                    format!("--block-private: '{host}' only resolves to private addresses").into();

                return Err(error);
            }

            Ok(Box::new(addresses.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(text: &str) -> IpAddr {
        text.parse().unwrap()
    }

    #[test]
    fn internal_addresses_are_private() {
        for text in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "::1",
            "fe80::1",
            "fd00::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
        ] {
            assert!(is_private_ip(ip(text)), "{text} should be private");
        }
    }

    #[test]
    fn public_addresses_are_not_private() {
        for text in [
            "8.8.8.8",
            "1.1.1.1",
            "100.128.0.1",
            "172.32.0.1",
            "2606:4700::1111",
        ] {
            assert!(!is_private_ip(ip(text)), "{text} should be public");
        }
    }

    #[test]
    fn local_names_and_literals_are_caught_without_dns() {
        for host in [
            "localhost",
            "LOCALHOST.",
            "api.localhost",
            "[::1]",
            "127.0.0.1",
        ] {
            assert!(is_private_literal(host), "{host}");
            assert!(ensure_public(host, 80).is_err(), "{host}");
        }

        assert!(!is_private_literal("example.com"));
    }

    #[test]
    fn a_public_literal_passes_without_dns() {
        assert!(ensure_public("8.8.8.8", 443).is_ok());
    }

    #[test]
    fn tcp_addresses_are_split() {
        assert_eq!(split_host_port("example.com:80"), Some(("example.com", 80)));
        assert_eq!(split_host_port("[::1]:9000"), Some(("[::1]", 9000)));
        assert_eq!(split_host_port("nope"), None);
    }
}
