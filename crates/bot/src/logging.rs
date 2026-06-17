use std::sync::OnceLock;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::field::RecordFields;
use tracing_subscriber::fmt::FormatFields;
use tracing_subscriber::fmt::format::{DefaultFields, Writer};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer, fmt};

/// Default directives when `RUST_LOG` is unset: our own crates verbose, everything
/// else at info so dependency / gateway problems still surface on a crash.
const DEFAULT_DIRECTIVES: &str = "info,bot=debug,beetroot_core=debug";

static SENSITIVE: OnceLock<bool> = OnceLock::new();

/// A field formatter that is a distinct *type* from the console layer's.
///
/// When two `fmt` layers use the same field-formatter type, tracing caches the
/// formatted span fields in the span's extensions keyed by that type, so the
/// second layer reuses the first's output (ANSI codes and all). Giving the file
/// layer its own type forces it to format span fields itself, honoring its own
/// `with_ansi(false)`.
#[derive(Default)]
struct PlainFields(DefaultFields);

impl<'writer> FormatFields<'writer> for PlainFields {
    fn format_fields<R: RecordFields>(&self, writer: Writer<'writer>, fields: R) -> std::fmt::Result {
        self.0.format_fields(writer, fields)
    }
}

/// Parse a boolean-ish env var. Returns `None` if unset or unrecognized.
fn env_flag(name: &str) -> Option<bool> {
    match std::env::var(name).ok()?.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" | "" => Some(false),
        _ => None,
    }
}

/// Whether sensitive logging is enabled (`LOG_SENSITIVE`, default false). Cached.
pub fn sensitive() -> bool {
    *SENSITIVE.get_or_init(|| env_flag("LOG_SENSITIVE").unwrap_or(false))
}

/// Display wrapper that hides its inner value unless sensitive logging is on.
///
/// Use it in structured fields so identifiers are redacted by default:
/// `tracing::info!(user = %logging::redact(id), "command invoked")`.
pub struct Redacted<T>(T);

impl<T: std::fmt::Display> std::fmt::Display for Redacted<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if sensitive() {
            write!(f, "{}", self.0)
        } else {
            f.write_str("<redacted>")
        }
    }
}

/// Wrap a value (e.g. a Discord user id) so it is only shown in logs when
/// `LOG_SENSITIVE` is enabled.
pub fn redact<T: std::fmt::Display>(value: T) -> Redacted<T> {
    Redacted(value)
}

/// Initialize the global subscriber.
///
/// Returns the file-appender worker guards; the caller must keep them alive for
/// the lifetime of the process (dropping them flushes and stops the writer).
pub fn init() -> Vec<WorkerGuard> {
    let mut guards = Vec::new();

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(DEFAULT_DIRECTIVES));

    let format = std::env::var("LOG_FORMAT")
        .map(|s| s.trim().to_ascii_lowercase())
        .unwrap_or_else(|_| "pretty".to_string());

    // Default colors on (Portainer / docker log viewers render ANSI), but honor
    // the de-facto NO_COLOR standard and an explicit LOG_COLOR override.
    let color = std::env::var_os("NO_COLOR").is_none() && env_flag("LOG_COLOR").unwrap_or(true);

    let console = {
        let base = fmt::layer().with_ansi(color).with_target(true);
        match format.as_str() {
            "json" => base.json().boxed(),
            "compact" => base.compact().boxed(),
            "full" => base.boxed(),
            _ => base.pretty().boxed(),
        }
    };

    let file_layer = if env_flag("LOG_FILE").unwrap_or(true) {
        let dir = std::env::var("LOG_DIR").unwrap_or_else(|_| "logs".to_string());
        match std::fs::create_dir_all(&dir) {
            Ok(()) => {
                let appender = tracing_appender::rolling::daily(&dir, "beetroot.log");
                let (writer, guard) = tracing_appender::non_blocking(appender);
                guards.push(guard);
                // File logs are never colored; mirror the console format otherwise.
                let base = fmt::layer().with_ansi(false).with_target(true).with_writer(writer);
                Some(if format == "json" {
                    base.json().boxed()
                } else {
                    // Distinct field-formatter type so span fields are not reused
                    // from the (possibly colored) console layer.
                    base.fmt_fields(PlainFields::default()).boxed()
                })
            }
            Err(e) => {
                eprintln!("[logging] could not create log dir '{dir}': {e}; file logging disabled");
                None
            }
        }
    } else {
        None
    };

    tracing_subscriber::registry()
        .with(filter)
        .with(console)
        .with(file_layer)
        .init();

    guards
}

/// Log a verbose medical-data diagnostic. Expands to nothing unless
/// `LOG_SENSITIVE` is enabled, and always logs at DEBUG under the `medical`
/// target, so raw glucose / treatment / profile payloads never reach a normal
/// production log. Intended only for local debugging of one's own instance.
#[macro_export]
macro_rules! log_medical {
    ($($arg:tt)*) => {
        if $crate::logging::sensitive() {
            tracing::debug!(target: "medical", $($arg)*);
        }
    };
}
