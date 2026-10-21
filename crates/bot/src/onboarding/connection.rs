//! Checking a Nightscout connection, and telling people why one failed in
//! words they can act on.

use crate::utils::net::{self, parse_and_normalize_url};
use chrono::{DateTime, Utc};
use url::Url;

/// What someone typed into the setup form, tidied up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteInput {
    pub url: String,
    pub token: Option<String>,
}

/// The newest glucose reading found while checking a site.
#[derive(Debug, Clone, Copy)]
pub struct Latest {
    pub mgdl: f64,
    pub mmol: f64,
    pub at: DateTime<Utc>,
}

/// Why a Nightscout site could not be used. Each one comes with its own
/// explanation and things to try.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    EmptyUrl,
    /// The address could not be read, with the reason.
    BadUrl(String),
    PrivateAddress,
    HostNotFound {
        host: String,
    },
    Unreachable {
        host: String,
    },
    Certificate {
        host: String,
    },
    Timeout {
        host: String,
    },
    /// The site wants credentials and none were given.
    Locked {
        host: String,
    },
    TokenRejected {
        host: String,
    },
    /// The site redirects elsewhere, with the base address it seems to have
    /// moved to when that can be told.
    Moved {
        host: String,
        to: Option<String>,
    },
    /// Something answered, but not with Nightscout data. `root` is the site's
    /// main address when a longer one was given.
    NotNightscout {
        host: String,
        root: Option<String>,
    },
    SiteDown {
        host: String,
        status: u16,
    },
    Other {
        host: String,
        detail: String,
    },
}

/// Clean up what people commonly paste: angle brackets Discord users add to
/// hide link previews, `http://` (Nightscout needs https), and a link copied
/// from Admin Tools that carries the token as `?token=…`.
pub fn tidy(url: &str, token: Option<&str>) -> SiteInput {
    let mut url = url
        .trim()
        .trim_start_matches('<')
        .trim_end_matches('>')
        .trim()
        .to_owned();
    if url
        .get(..7)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("http://"))
    {
        url = format!("https://{}", &url[7..]);
    }

    let mut token = token
        .map(str::trim)
        .map(|t| t.strip_prefix("token=").unwrap_or(t).trim().to_owned())
        .filter(|t| !t.is_empty());

    let with_scheme = if url.contains("://") {
        url.clone()
    } else {
        format!("https://{url}")
    };
    if let Ok(mut parsed) = Url::parse(&with_scheme)
        && let Some(found) = parsed
            .query_pairs()
            .find(|(key, _)| key == "token")
            .map(|(_, value)| value.trim().to_owned())
    {
        if token.is_none() && !found.is_empty() {
            token = Some(found);
        }
        parsed.set_query(None);
        url = parsed.to_string();
    }

    SiteInput { url, token }
}

/// Read a Nightscout address, explaining what is wrong with it if it can't be
/// used.
pub fn parse_site(input: &str) -> Result<Url, Problem> {
    if input.trim().is_empty() {
        return Err(Problem::EmptyUrl);
    }
    parse_and_normalize_url(input).map_err(|reason| {
        if reason == net::INTERNAL_ADDRESS {
            Problem::PrivateAddress
        } else {
            Problem::BadUrl(reason)
        }
    })
}

/// Fetch the newest reading from a site, the same way every command will.
/// `Ok(None)` means the site works but holds no readings yet.
pub async fn probe(site: &Url, token: Option<&str>) -> Result<Option<Latest>, Problem> {
    let host = host_of(site);
    let client = net::nightscout_client(site.as_str(), token).map_err(Problem::BadUrl)?;
    let entries = client
        .entries()
        .sgv()
        .list()
        .limit(1)
        .send()
        .await
        .map_err(|e| {
            let problem = classify(&e, site, token.is_some());
            tracing::debug!(error = %e, problem = problem.kind(), "nightscout check failed");
            problem
        })?;

    tracing::debug!(host = %crate::logging::redact(&host), readings = entries.len(), "nightscout check passed");
    Ok(entries.first().map(|e| Latest {
        mgdl: e.sgv.as_mgdl(),
        mmol: e.sgv.as_mmol(),
        at: e.date.to_datetime(),
    }))
}

/// Turn a failed request into the problem behind it.
fn classify(err: &cinnamon::Error, site: &Url, has_token: bool) -> Problem {
    use cinnamon::Error as E;

    let host = host_of(site);
    match err {
        E::Unauthorized { .. } | E::Forbidden { .. } if has_token => {
            Problem::TokenRejected { host }
        }
        E::Unauthorized { .. } | E::Forbidden { .. } => Problem::Locked { host },
        E::Redirected { location, .. } => Problem::Moved {
            to: moved_to(site, location),
            host,
        },
        E::NotFound | E::Decode { .. } => Problem::NotNightscout {
            root: (site.path() != "/").then(|| format!("{}/", site.origin().ascii_serialization())),
            host,
        },
        E::Server { status, .. } if *status >= 500 => Problem::SiteDown {
            host,
            status: *status,
        },
        E::Timeout => Problem::Timeout { host },
        E::Transport(source) => from_transport(&error_chain(source.as_ref()), host),
        E::InvalidUrl(reason) => Problem::BadUrl(reason.clone()),
        other => Problem::Other {
            host,
            detail: other.to_string(),
        },
    }
}

/// Every message in an error's source chain, lowercased, for matching.
fn error_chain(err: &(dyn std::error::Error + 'static)) -> String {
    let mut text = err.to_string();
    let mut source = err.source();
    while let Some(inner) = source {
        text.push_str(": ");
        text.push_str(&inner.to_string());
        source = inner.source();
    }
    text.to_lowercase()
}

/// Sort a connection failure by the words the network stack used.
fn from_transport(chain: &str, host: String) -> Problem {
    let has = |needles: &[&str]| needles.iter().any(|n| chain.contains(n));

    if has(&["private or reserved"]) {
        Problem::PrivateAddress
    } else if has(&[
        "dns error",
        "failed to lookup",
        "name or service not known",
        "nodename nor servname",
        "no address associated",
        "no such host",
    ]) {
        Problem::HostNotFound { host }
    } else if has(&["certificate", "tls", "ssl", "handshake"]) {
        Problem::Certificate { host }
    } else if has(&["timed out", "timeout"]) {
        Problem::Timeout { host }
    } else {
        Problem::Unreachable { host }
    }
}

/// The base address a redirect points to, when it moved the API itself. A
/// redirect to anything else (a login page, a proxy) has no address to offer.
fn moved_to(site: &Url, location: &str) -> Option<String> {
    let mut target = site.join(location).ok()?;
    let api = target.path().find("/api/v")?;
    let base = target.path()[..=api].to_owned();
    target.set_path(&base);
    target.set_query(None);
    target.set_fragment(None);
    if target.scheme() == "http" {
        target.set_scheme("https").ok()?;
    }
    (target.as_str() != site.as_str()).then(|| target.to_string())
}

fn host_of(site: &Url) -> String {
    site.host_str().unwrap_or("your site").to_owned()
}

impl Problem {
    /// A short name for logs.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::EmptyUrl => "empty url",
            Self::BadUrl(_) => "bad url",
            Self::PrivateAddress => "private address",
            Self::HostNotFound { .. } => "host not found",
            Self::Unreachable { .. } => "unreachable",
            Self::Certificate { .. } => "certificate",
            Self::Timeout { .. } => "timeout",
            Self::Locked { .. } => "locked",
            Self::TokenRejected { .. } => "token rejected",
            Self::Moved { .. } => "moved",
            Self::NotNightscout { .. } => "not nightscout",
            Self::SiteDown { .. } => "site down",
            Self::Other { .. } => "other",
        }
    }

    /// A better address to try, when the problem points to one.
    pub fn suggestion(&self) -> Option<&str> {
        match self {
            Self::Moved { to, .. } => to.as_deref(),
            Self::NotNightscout { root, .. } => root.as_deref(),
            _ => None,
        }
    }

    /// Whether the token, rather than the address, is what needs changing.
    pub fn is_about_token(&self) -> bool {
        matches!(self, Self::Locked { .. } | Self::TokenRejected { .. })
    }

    pub fn title(&self) -> &'static str {
        match self {
            Self::EmptyUrl => "Your Nightscout address is missing",
            Self::BadUrl(_) => "That doesn't look like a web address",
            Self::PrivateAddress => "That address is on a private network",
            Self::HostNotFound { .. } => "We couldn't find that site",
            Self::Unreachable { .. } => "Your site didn't answer",
            Self::Certificate { .. } => "Your site's security certificate has a problem",
            Self::Timeout { .. } => "Your site took too long to answer",
            Self::Locked { .. } => "Your Nightscout is locked",
            Self::TokenRejected { .. } => "Your site didn't accept that token",
            Self::Moved { .. } => "Your site sent us somewhere else",
            Self::NotNightscout { .. } => "That doesn't look like a Nightscout site",
            Self::SiteDown { .. } => "Your Nightscout is having trouble",
            Self::Other { .. } => "Something went wrong",
        }
    }

    /// What happened and what to try, as Discord markdown.
    pub fn explain(&self) -> String {
        match self {
            Self::EmptyUrl => "Paste the address you open to see your Nightscout, for example \
                 `https://yourname.herokuapp.com`."
                .to_owned(),
            Self::BadUrl(reason) => format!(
                "Beetroot couldn't read it as an address ({reason}).\n\n\
                 - Copy it straight from your browser's address bar\n\
                 - It should look like `https://yourname.herokuapp.com`, with no spaces"
            ),
            Self::PrivateAddress => "Beetroot runs on the internet, so it can't reach addresses \
                 inside a home or local network, like `192.168.…`, `10.…` or `localhost`.\n\n\
                 - Use your site's public address: the one that opens on your phone with Wi-Fi off"
                .to_owned(),
            Self::HostNotFound { host } => format!(
                "No website answers at **{host}**. This is usually a small typo.\n\n\
                 - Check the spelling, letter by letter\n\
                 - Open the address in your browser: does your Nightscout appear?\n\
                 - Moved your Nightscout to a new host? Use the new address"
            ),
            Self::Unreachable { host } => format!(
                "**{host}** exists, but it refused or dropped the connection.\n\n\
                 - Open it in your browser to check it's running\n\
                 - Hosting plans that sleep or pause apps (free ones often do) need the app \
                 started again from their dashboard"
            ),
            Self::Certificate { host } => format!(
                "**{host}** answered, but its HTTPS certificate is missing, expired or for \
                 another address, so Beetroot won't send anything there.\n\n\
                 - Open it in your browser: a security warning confirms it\n\
                 - Renew or fix the certificate from your hosting dashboard"
            ),
            Self::Timeout { host } => format!(
                "**{host}** didn't respond in time. Sites on free plans often fall asleep \
                 and take a while to wake up.\n\n\
                 - Open your Nightscout in a browser and wait until it has loaded\n\
                 - Then try again"
            ),
            Self::Locked { host } => format!(
                "**{host}** only shares its data with visitors who have a key, so Beetroot \
                 needs a **token**.\n\n{TOKEN_STEPS}"
            ),
            Self::TokenRejected { host } => format!(
                "**{host}** works, but it said no to the token.\n\n\
                 - Copy it again and check for missing characters or extra spaces\n\
                 - Use the **access token** (like `beetroot-1a2b3c4d5e6f7a8b`), not the \
                 subject's name\n\
                 - Check that the subject has the `readable` role\n\
                 - Using your API_SECRET? It's case-sensitive and must match your site's \
                 settings exactly"
            ),
            Self::Moved { host, to: Some(to) } => format!(
                "**{host}** redirects to `{to}`. Beetroot never follows redirects, so your \
                 token can't end up somewhere you didn't choose.\n\n\
                 - Use the new address instead"
            ),
            Self::Moved { host, to: None } => format!(
                "**{host}** redirects to another page, perhaps a login page or a proxy in \
                 front of your Nightscout. Beetroot never follows redirects, so your token \
                 can't end up somewhere you didn't choose.\n\n\
                 - Open your site in a browser and use the address it ends up on\n\
                 - If a login page sits in front of your Nightscout, Beetroot can't get past it"
            ),
            Self::NotNightscout { host, root } => {
                let mut text = format!(
                    "Something answered at **{host}**, but not with the Nightscout data \
                     Beetroot expected.\n\n"
                );
                if let Some(root) = root {
                    text.push_str(&format!(
                        "- Use the main address, `{root}`, without anything after it\n"
                    ));
                }
                text.push_str(
                    "- Check it's your **Nightscout** address, not your uploader's or a \
                     follower app's\n\
                     - Seeing a \"no such app\" or parked page? Your site may have moved",
                );
                text
            }
            Self::SiteDown { host, status } => format!(
                "**{host}** answered with an error (HTTP {status}). The site itself is down \
                 or restarting: nothing you typed is wrong.\n\n\
                 - Open it in your browser to check\n\
                 - Look at your hosting dashboard: the app may be stopped, out of free hours \
                 or crashed, and a restart often fixes it"
            ),
            Self::Other { host, detail } => format!(
                "Beetroot couldn't connect to **{host}**, and isn't sure why.\n\n\
                 - Open your site in a browser to check it works\n\
                 - Try again in a minute\n\n\
                 If it keeps happening, this helps us find out why: `{detail}`"
            ),
        }
    }
}

/// How to make a read-only token, shared by the locked-site help and the
/// setup questions.
pub const TOKEN_STEPS: &str = "**Making a read-only token:**\n\
     1. Open your Nightscout, then the menu ☰ → **Admin Tools**\n\
     2. Under **Subjects**, press **Add new Subject**\n\
     3. Name it `beetroot`, give it the `readable` role and save\n\
     4. Copy the access token shown next to it, like `beetroot-1a2b3c4d5e6f7a8b`\n\n\
     Your API_SECRET works too, but it gives full access to your site, so a \
     read-only token is safer.";

#[cfg(test)]
mod tests {
    use super::*;

    fn site(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn tidy_fixes_common_pastes() {
        let input = tidy("  <http://Example.com/>  ", Some("  "));
        assert_eq!(input.url, "https://Example.com/");
        assert_eq!(input.token, None);

        let input = tidy("HTTP://example.com", Some("token=abc"));
        assert_eq!(input.url, "https://example.com");
        assert_eq!(input.token.as_deref(), Some("abc"));
    }

    #[test]
    fn tidy_takes_the_token_out_of_an_admin_tools_link() {
        let input = tidy("example.com/?token=beetroot-0123456789abcdef", None);
        assert_eq!(input.url, "https://example.com/");
        assert_eq!(input.token.as_deref(), Some("beetroot-0123456789abcdef"));

        // A token typed into its own field wins over one in the link.
        let input = tidy(
            "https://example.com/?token=old-0123456789abcdef",
            Some("new"),
        );
        assert_eq!(input.url, "https://example.com/");
        assert_eq!(input.token.as_deref(), Some("new"));
    }

    #[test]
    fn parse_site_names_the_problem() {
        assert_eq!(parse_site("   "), Err(Problem::EmptyUrl));
        assert_eq!(
            parse_site("https://192.168.1.10"),
            Err(Problem::PrivateAddress)
        );
        assert!(matches!(
            parse_site("https://my site.com"),
            Err(Problem::BadUrl(_))
        ));
        assert!(parse_site("example.com").is_ok());
    }

    #[test]
    fn credentials_problems_depend_on_whether_a_token_was_given() {
        let err = cinnamon::Error::Unauthorized {
            message: "Unauthorized".into(),
        };
        let url = site("https://example.com/");
        assert!(matches!(
            classify(&err, &url, false),
            Problem::Locked { .. }
        ));
        assert!(matches!(
            classify(&err, &url, true),
            Problem::TokenRejected { .. }
        ));
    }

    #[test]
    fn transport_failures_are_told_apart() {
        let host = || "example.com".to_owned();
        let cases = [
            (
                "error sending request: client error (connect): dns error: failed to lookup address information: nodename nor servname provided, or not known",
                "host not found",
            ),
            (
                "client error (connect): dns error: refusing to connect to 'nas.local': resolves only to private or reserved addresses",
                "private address",
            ),
            (
                "client error (connect): invalid peer certificate: expired",
                "certificate",
            ),
            (
                "client error (connect): tcp connect error: connection refused (os error 61)",
                "unreachable",
            ),
            ("client error (connect): operation timed out", "timeout"),
        ];
        for (chain, kind) in cases {
            assert_eq!(from_transport(chain, host()).kind(), kind, "{chain}");
        }
    }

    #[test]
    fn redirects_suggest_the_new_base_address() {
        let url = site("https://example.com/");
        assert_eq!(
            moved_to(
                &url,
                "https://www.example.com/api/v1/entries/sgv.json?count=1"
            )
            .as_deref(),
            Some("https://www.example.com/")
        );
        assert_eq!(
            moved_to(&url, "/ns/api/v1/entries/sgv.json").as_deref(),
            Some("https://example.com/ns/")
        );
        // A login page is no Nightscout address.
        assert_eq!(moved_to(&url, "https://login.example.com/sign-in"), None);
    }

    #[test]
    fn a_long_address_suggests_the_main_one() {
        let url = site("https://example.com/api/v1/entries/");
        let problem = classify(&cinnamon::Error::NotFound, &url, false);
        assert_eq!(problem.suggestion(), Some("https://example.com/"));

        let url = site("https://example.com/");
        let problem = classify(&cinnamon::Error::NotFound, &url, false);
        assert_eq!(problem.suggestion(), None);
    }

    #[test]
    fn every_problem_has_words() {
        let host = || "example.com".to_owned();
        let all = [
            Problem::EmptyUrl,
            Problem::BadUrl("Invalid URL format".into()),
            Problem::PrivateAddress,
            Problem::HostNotFound { host: host() },
            Problem::Unreachable { host: host() },
            Problem::Certificate { host: host() },
            Problem::Timeout { host: host() },
            Problem::Locked { host: host() },
            Problem::TokenRejected { host: host() },
            Problem::Moved {
                host: host(),
                to: None,
            },
            Problem::Moved {
                host: host(),
                to: Some("https://www.example.com/".into()),
            },
            Problem::NotNightscout {
                host: host(),
                root: None,
            },
            Problem::SiteDown {
                host: host(),
                status: 503,
            },
            Problem::Other {
                host: host(),
                detail: "boom".into(),
            },
        ];
        for problem in all {
            assert!(!problem.title().is_empty());
            // Discord caps embed descriptions at 4096 characters.
            assert!(problem.explain().len() < 1500, "{}", problem.kind());
        }
    }
}
