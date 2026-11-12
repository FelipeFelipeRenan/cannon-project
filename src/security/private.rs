//! `--block-private`: refuse targets that resolve to internal addresses.
//!
//! Useful when Cannon runs somewhere it must not be able to reach the local
//! network (a shared runner, or an agent taking targets from someone else):
//! loopback, private ranges, link-local (including cloud metadata services at
//! `169.254.169.254`), carrier-grade NAT and unique-local IPv6.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};

/// Whether TCP connections must only go to public addresses.
///
/// Set once at startup from `--block-private`; read on every TCP connect.
static PUBLIC_ONLY: AtomicBool = AtomicBool::new(false);

/// Turns the public-only rule for TCP connections on or off.
pub fn set_public_only(enabled: bool) {
    PUBLIC_ONLY.store(enabled, Ordering::Relaxed);
}

/// Whether TCP connections are restricted to public addresses.
#[must_use]
pub fn public_only() -> bool {
    PUBLIC_ONLY.load(Ordering::Relaxed)
}

/// Connects to `address` (`host:port`), but only to public IPs.
///
/// The name is resolved here, at connection time, and the connection goes to
/// the very addresses that passed the check, so a name that starts answering
/// with an internal address later (DNS rebinding) cannot slip through.
///
/// # Errors
///
/// Fails when resolution fails, every address is private, or no public
/// address accepts the connection.
pub async fn connect_public(address: &str) -> std::io::Result<tokio::net::TcpStream> {
    let public: Vec<SocketAddr> = tokio::net::lookup_host(address)
        .await?
        .filter(|candidate| !is_private_ip(candidate.ip()))
        .collect();

    if public.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!("--block-private: '{address}' only resolves to private addresses"),
        ));
    }

    tokio::net::TcpStream::connect(&public[..]).await
}

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

    let [_, _, c, _] = ip.octets();

    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
        || (a == 100 && (64..=127).contains(&b)) // carrier-grade NAT, 100.64.0.0/10
        || a == 0
        || a >= 240 // reserved, 240.0.0.0/4
        || (a == 198 && (b == 18 || b == 19)) // benchmarking, 198.18.0.0/15
        || (a == 192 && b == 0 && c == 0) // IETF assignments, 192.0.0.0/24
        || (a == 192 && b == 0 && c == 2) // documentation
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
}

fn is_private_v6(ip: Ipv6Addr) -> bool {
    if let Some(mapped) = ip.to_ipv4_mapped() {
        return is_private_v4(mapped);
    }

    let segments = ip.segments();
    let first = segments[0];

    // Forms that carry an IPv4 address inside: judge that address instead.
    let embedded = |high: u16, low: u16| {
        let [a, b] = high.to_be_bytes();
        let [c, d] = low.to_be_bytes();
        Ipv4Addr::new(a, b, c, d)
    };

    if segments[..6] == [0x64, 0xff9b, 0, 0, 0, 0] {
        return is_private_v4(embedded(segments[6], segments[7])); // NAT64
    }

    if first == 0x2002 {
        return is_private_v4(embedded(segments[1], segments[2])); // 6to4
    }

    if segments[..6] == [0, 0, 0, 0, 0, 0] && !ip.is_unspecified() && !ip.is_loopback() {
        return is_private_v4(embedded(segments[6], segments[7])); // IPv4-compatible
    }

    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (first & 0xfe00) == 0xfc00 // unique local, fc00::/7
        || (first & 0xffc0) == 0xfe80 // link-local, fe80::/10
        || (first & 0xffc0) == 0xfec0 // site-local (deprecated), fec0::/10
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
            "64:ff9b::a9fe:a9fe", // NAT64 of 169.254.169.254
            "2002:7f00:1::",      // 6to4 of 127.0.0.1
            "::7f00:1",           // IPv4-compatible 127.0.0.1
            "fec0::1",
            "240.0.0.1",
            "198.18.0.1",
            "192.0.0.192",
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
            "64:ff9b::808:808", // NAT64 of 8.8.8.8
            "2002:808:808::",   // 6to4 of 8.8.8.8
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
    fn odd_ip_spellings_are_normalised_before_the_check() {
        // `url` turns decimal, octal and hex forms into dotted quads, which is
        // what the target check sees.
        for spelling in [
            "http://2130706433/",
            "http://0x7f.1/",
            "http://0177.0.0.1/",
            "http://127.1/",
        ] {
            let url = url::Url::parse(spelling).unwrap();

            assert!(is_private_literal(url.host_str().unwrap()), "{spelling}");
        }
    }

    #[tokio::test]
    async fn tcp_connections_to_names_that_resolve_privately_are_refused() {
        let error = connect_public("localhost:9").await.unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
        assert!(error.to_string().contains("--block-private"), "{error}");
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
