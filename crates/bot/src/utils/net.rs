use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use url::Url;

/// Time to establish a TCP/TLS connection before giving up.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// Total time for a single request (connect + headers + body).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
/// Maximum redirects we will follow, each re-validated.
const MAX_REDIRECTS: usize = 5;

/// True if an IPv4 address must never be the target of an outbound fetch.
fn is_blocked_ipv4(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    ip.is_loopback()            // 127.0.0.0/8
        || ip.is_private()      // 10/8, 172.16/12, 192.168/16
        || ip.is_link_local()   // 169.254.0.0/16 (incl. 169.254.169.254 metadata)
        || ip.is_broadcast()    // 255.255.255.255
        || ip.is_documentation()// 192.0.2/24, 198.51.100/24, 203.0.113/24
        || ip.is_unspecified()  // 0.0.0.0
        || o[0] == 0                                   // 0.0.0.0/8 "this network"
        || (o[0] == 100 && (o[1] & 0xc0) == 64)        // 100.64.0.0/10 CGNAT
        || (o[0] == 192 && o[1] == 0 && o[2] == 0)     // 192.0.0.0/24 IETF
        || (o[0] == 198 && (o[1] & 0xfe) == 18)        // 198.18.0.0/15 benchmarking
        || o[0] >= 224 // 224/4 multicast + 240/4 reserved
}

/// True if an IPv6 address must never be the target of an outbound fetch.
fn is_blocked_ipv6(ip: Ipv6Addr) -> bool {
    // Anything embedding an IPv4 address (mapped `::ffff:a.b.c.d` or the
    // deprecated compatible `::a.b.c.d`) is judged by its IPv4 rules.
    if let Some(v4) = ip.to_ipv4() {
        return is_blocked_ipv4(v4);
    }

    let seg = ip.segments();
    ip.is_loopback()                              // ::1
        || ip.is_unspecified()                   // ::
        || ip.is_multicast()                     // ff00::/8
        || (seg[0] & 0xfe00) == 0xfc00           // fc00::/7 unique-local
        || (seg[0] & 0xffc0) == 0xfe80           // fe80::/10 link-local
        || (seg[0] == 0x2001 && seg[1] == 0x0db8) // 2001:db8::/32 documentation
}

/// True if `ip` is loopback / private / link-local / reserved and so off limits.
pub fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_blocked_ipv4(v4),
        IpAddr::V6(v6) => is_blocked_ipv6(v6),
    }
}

/// A reqwest DNS resolver that filters out internal/reserved addresses.
///
/// Resolution happens through the system resolver and then every candidate
/// address is screened. If a host resolves only to blocked space the connection
/// fails. Because this runs at actual connect time (and again for each redirect
/// target host), it is not vulnerable to the resolve-then-rebind race that a
/// one-shot pre-check would be.
#[derive(Debug, Clone, Default)]
struct SsrfResolver;

impl Resolve for SsrfResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let resolved = tokio::net::lookup_host((host.as_str(), 0))
                .await
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;

            let safe: Vec<SocketAddr> = resolved.filter(|a| !is_blocked_ip(a.ip())).collect();

            if safe.is_empty() {
                let err: Box<dyn std::error::Error + Send + Sync> = format!(
                    "refusing to connect to '{host}': resolves only to private or reserved addresses"
                )
                .into();
                return Err(err);
            }

            Ok(Box::new(safe.into_iter()) as Addrs)
        })
    }
}

/// Shared, SSRF-hardened HTTP client. Built once and cloned (clones share the
/// connection pool), so callers should never construct their own client for
/// user-supplied URLs.
pub fn guarded_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        let redirect = reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= MAX_REDIRECTS {
                return attempt.error(format!("too many redirects (> {MAX_REDIRECTS})"));
            }
            if !matches!(attempt.url().scheme(), "http" | "https") {
                return attempt.error("refusing redirect to a non-http(s) URL");
            }
            // IP-literal redirect targets bypass the DNS resolver, so screen them
            // here; host-name targets are screened by SsrfResolver at connect.
            let blocked = match attempt.url().host() {
                Some(url::Host::Ipv4(ip)) => is_blocked_ipv4(ip),
                Some(url::Host::Ipv6(ip)) => is_blocked_ipv6(ip),
                _ => false,
            };
            if blocked {
                return attempt.error("refusing redirect to a private/reserved address");
            }
            attempt.follow()
        });

        reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .redirect(redirect)
            .dns_resolver(Arc::new(SsrfResolver))
            .build()
            .expect("failed to build SSRF-guarded HTTP client")
    })
}

/// Reject a URL whose scheme is not http(s) or whose host is an IP literal in an
/// internal/reserved range. Host names are intentionally allowed through here
/// and screened at connect time by [`SsrfResolver`].
pub fn check_public_url(url: &Url) -> Result<(), String> {
    match url.scheme() {
        "http" | "https" => {}
        other => {
            return Err(format!(
                "URL scheme '{other}' is not allowed; use http or https"
            ));
        }
    }

    match url.host() {
        None => Err("URL must have a host".to_string()),
        Some(url::Host::Ipv4(ip)) if is_blocked_ipv4(ip) => Err(
            "That address points to an internal/reserved network and is not allowed.".to_string(),
        ),
        Some(url::Host::Ipv6(ip)) if is_blocked_ipv6(ip) => Err(
            "That address points to an internal/reserved network and is not allowed.".to_string(),
        ),
        Some(_) => Ok(()),
    }
}

/// Parse and normalize a user-supplied Nightscout URL.
///
/// Accepts a bare host (assumes `https://`), enforces http(s), rejects
/// IP-literal internal hosts, and appends a trailing slash. Shared by `/setup`
/// and `/url` (previously duplicated in both).
pub fn parse_and_normalize_url(input: &str) -> Result<Url, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("URL cannot be empty".to_string());
    }

    let mut url = match Url::parse(input) {
        Ok(u) => u,
        Err(url::ParseError::RelativeUrlWithoutBase) => {
            Url::parse(&format!("https://{}", input))
                .map_err(|_| "Invalid URL format".to_string())?
        }
        Err(e) => return Err(format!("Invalid URL: {}", e)),
    };

    check_public_url(&url)?;

    if !url.path().ends_with('/')
        && let Ok(mut segments) = url.path_segments_mut()
    {
        segments.pop_if_empty().push("");
    }

    Ok(url)
}

/// Build a cinnamon Nightscout client backed by the SSRF-guarded HTTP client.
///
/// Cinnamon's `NightscoutClient::new` builds a default reqwest client (no
/// timeout, default DNS, follows redirects). We construct the client directly so
/// every Nightscout request goes through [`guarded_client`] instead.
pub fn nightscout_client(
    base_url: &str,
    token: Option<&str>,
) -> Result<cinnamon::client::NightscoutClient, String> {
    use cinnamon::client::{NightscoutClient, NightscoutClientInner};

    let url = Url::parse(base_url).map_err(|e| format!("Invalid URL: {e}"))?;
    // Defense in depth for IP-literal hosts; host names are screened at connect.
    check_public_url(&url)?;

    let client = NightscoutClient {
        inner: Arc::new(NightscoutClientInner {
            base_url: url,
            http: guarded_client().clone(),
            api_secret_hash: None,
        }),
    };

    Ok(match token {
        Some(t) if !t.is_empty() => client.with_secret(t),
        _ => client,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blocked(s: &str) -> bool {
        is_blocked_ip(s.parse().unwrap())
    }

    #[test]
    fn blocks_internal_and_reserved() {
        for ip in [
            "127.0.0.1",
            "10.0.0.5",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254", // cloud metadata
            "100.64.0.1",      // CGNAT
            "0.0.0.0",
            "255.255.255.255",
            "::1",
            "fe80::1",
            "fc00::1",
            "::ffff:127.0.0.1", // IPv4-mapped loopback
        ] {
            assert!(blocked(ip), "expected {ip} to be blocked");
        }
    }

    #[test]
    fn allows_public() {
        for ip in [
            "8.8.8.8",
            "1.1.1.1",
            "93.184.216.34",
            "2606:4700:4700::1111",
        ] {
            assert!(!blocked(ip), "expected {ip} to be allowed");
        }
    }

    #[test]
    fn check_public_url_rejects_ip_literals_and_schemes() {
        assert!(check_public_url(&Url::parse("http://127.0.0.1/").unwrap()).is_err());
        assert!(check_public_url(&Url::parse("http://169.254.169.254/").unwrap()).is_err());
        assert!(check_public_url(&Url::parse("ftp://example.com/").unwrap()).is_err());
        assert!(check_public_url(&Url::parse("https://example.com/").unwrap()).is_ok());
    }

    #[test]
    fn normalize_adds_scheme_and_trailing_slash() {
        let u = parse_and_normalize_url("example.com").unwrap();
        assert_eq!(u.as_str(), "https://example.com/");
    }

    #[test]
    fn normalize_rejects_internal_hosts() {
        assert!(parse_and_normalize_url("http://127.0.0.1").is_err());
        assert!(parse_and_normalize_url("http://192.168.0.1/api").is_err());
    }

    #[tokio::test]
    async fn resolver_blocks_names_pointing_at_loopback() {
        // `localhost` resolves to 127.0.0.1 / ::1, which are all blocked, so the
        // resolver must surface an error instead of any address to connect to.
        let name: Name = "localhost".parse().unwrap();
        let result = SsrfResolver.resolve(name).await;
        assert!(
            result.is_err(),
            "localhost should not be resolvable for outbound fetches"
        );
    }
}
