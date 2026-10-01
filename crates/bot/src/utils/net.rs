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
/// User-Agent for outbound fetches. Some image CDNs reject requests that carry
/// no User-Agent, which silently broke sticker image downloads; a named agent
/// is also friendlier to Nightscout hosts.
const USER_AGENT: &str = concat!("Beetroot/", env!("CARGO_PKG_VERSION"));

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

/// Base configuration shared by every client that talks to user-supplied hosts.
fn guarded_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .dns_resolver(Arc::new(SsrfResolver))
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

        guarded_builder()
            .redirect(redirect)
            .build()
            .expect("failed to build SSRF-guarded HTTP client")
    })
}

/// SSRF-hardened HTTP client for Nightscout requests. Unlike [`guarded_client`]
/// it never follows redirects: reqwest forwards custom headers such as
/// `api-secret` across them, which would hand the credentials to another host.
fn nightscout_http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        guarded_builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("failed to build Nightscout HTTP client")
    })
}

/// Shared client for our own hard-coded endpoints
/// Carries the same timeouts as [`guarded_client`] but keeps reqwest's default
/// DNS and redirects, since the targets are not user-supplied. Cloning shares the
/// connection pool, so callers should reuse this instead of building their own.
pub fn shared_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .build()
            .expect("failed to build shared HTTP client")
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
/// Accepts a bare host (assumes `https://`), enforces https, rejects
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

    // Cinnamon refuses to send credentials or health data over plain HTTP.
    if url.scheme() != "https" {
        return Err("Nightscout URL must use https://".to_string());
    }

    if !url.path().ends_with('/')
        && let Ok(mut segments) = url.path_segments_mut()
    {
        segments.pop_if_empty().push("");
    }

    Ok(url)
}

/// True if `token` has the shape of a Nightscout access token
/// (`<subject abbreviation>-<16 hex digits>`, as generated by Admin Tools).
fn looks_like_access_token(token: &str) -> bool {
    token.rsplit_once('-').is_some_and(|(name, digest)| {
        name.len() <= 10
            && name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
            && digest.len() == 16
            && digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// Pick the cinnamon credentials for whatever the user stored as their "token":
/// an access token is exchanged for a JWT, anything else is treated as the
/// site's `API_SECRET`.
fn nightscout_credentials(token: Option<&str>) -> cinnamon::Credentials {
    use cinnamon::Credentials;

    match token.map(str::trim) {
        Some(t) if looks_like_access_token(t) => Credentials::access_token(t),
        Some(t) if !t.is_empty() => Credentials::api_secret(t),
        _ => Credentials::none(),
    }
}

/// Build a cinnamon Nightscout client backed by the SSRF-guarded HTTP client.
///
/// Cinnamon's default client uses the system DNS resolver. We hand it
/// [`nightscout_http_client`] so every Nightscout request is screened instead.
pub fn nightscout_client(base_url: &str, token: Option<&str>) -> Result<cinnamon::Client, String> {
    let url = Url::parse(base_url).map_err(|e| format!("Invalid URL: {e}"))?;
    // Defense in depth for IP-literal hosts; host names are screened at connect.
    check_public_url(&url)?;

    cinnamon::Client::builder(url.as_str())
        .http_client(nightscout_http_client().clone())
        .app_name("beetroot")
        .credentials(nightscout_credentials(token))
        .build()
        .map_err(|e| e.to_string())
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
    fn normalize_rejects_plain_http() {
        assert!(parse_and_normalize_url("http://example.com").is_err());
    }

    #[test]
    fn access_tokens_are_told_apart_from_api_secrets() {
        assert!(looks_like_access_token("beetroot-0123456789abcdef"));
        assert!(looks_like_access_token("my_app-0123456789abcdef"));
        assert!(!looks_like_access_token("my-long-api-secret"));
        assert!(!looks_like_access_token("0123456789abcdef"));
        assert!(!looks_like_access_token("beetroot-0123456789ABCDEF"));
    }

    #[test]
    fn nightscout_client_requires_https() {
        assert!(nightscout_client("https://example.com/", None).is_ok());
        assert!(nightscout_client("http://example.com/", Some("secret")).is_err());
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
