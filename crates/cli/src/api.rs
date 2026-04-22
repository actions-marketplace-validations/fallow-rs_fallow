//! Shared HTTP layer for fallow-cloud backend calls.
//!
//! Provides a common `ureq::Agent` builder, URL resolution (respecting the
//! `FALLOW_API_URL` env override), typed error-envelope parsing, and an
//! actionable-hint mapper for backend error codes. Consumed by:
//!
//! - `license/`: trial activation, license refresh (5s connect, 10s total).
//! - `coverage/upload_inventory`: static inventory POST (5s connect, 30s total).
//!
//! The trait [`ResponseBodyReader`] decouples the status/body accessors from
//! `ureq::Response` so error-path code can be unit-tested with a lightweight
//! stub.

use std::time::Duration;

use serde::Deserialize;
use serde::de::DeserializeOwned;

/// Default fallow cloud API base URL.
pub const DEFAULT_API_URL: &str = "https://api.fallow.cloud";

/// Exit code for network failures (connect error, timeout, auth rejection).
/// Used by any subcommand that reaches fallow cloud; keeps error classification
/// consistent across `license` and `coverage` surfaces.
pub const NETWORK_EXIT_CODE: u8 = 7;

/// Default connect timeout (seconds).
const DEFAULT_CONNECT_TIMEOUT_SECS: u64 = 5;
/// Default total request timeout (seconds).
const DEFAULT_TOTAL_TIMEOUT_SECS: u64 = 10;

/// Construct a `ureq::Agent` with the default timeouts (5s connect, 10s total).
///
/// Suitable for small-body JSON requests (license trial / refresh). For larger
/// payloads (inventory upload), use [`api_agent_with_timeout`].
pub fn api_agent() -> ureq::Agent {
    api_agent_with_timeout(DEFAULT_CONNECT_TIMEOUT_SECS, DEFAULT_TOTAL_TIMEOUT_SECS)
}

/// Construct a `ureq::Agent` with custom timeouts.
///
/// Both timeouts are honored: connect applies to the initial TCP handshake,
/// total bounds the full request/response cycle. `http_status_as_error(false)`
/// is set so callers can inspect non-2xx responses via [`http_status_message`]
/// instead of having them surface as transport errors.
pub fn api_agent_with_timeout(connect_timeout_secs: u64, total_timeout_secs: u64) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(connect_timeout_secs)))
        .timeout_global(Some(Duration::from_secs(total_timeout_secs)))
        .http_status_as_error(false)
        .build()
        .new_agent()
}

/// Resolve an API endpoint path to a full URL.
///
/// Honors `FALLOW_API_URL` for staging/local development. Trailing slashes on
/// the base are trimmed so `/v1/...` paths never double-slash.
pub fn api_url(path: &str) -> String {
    let base = std::env::var("FALLOW_API_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_API_URL.to_owned());
    format!("{}{path}", base.trim_end_matches('/'))
}

/// Structured error payload returned by fallow cloud on non-2xx responses.
#[derive(Debug, Deserialize, Default)]
pub struct ErrorEnvelope {
    /// Machine-readable code (e.g. `rate_limit_exceeded`, `payload_too_large`).
    #[serde(default)]
    pub code: Option<String>,
    /// Human-readable message from the backend.
    #[serde(default)]
    pub message: Option<String>,
}

/// Map a backend error-code + operation pair to an actionable user-facing
/// hint. Returns `None` for unknown codes; callers fall back to the generic
/// "HTTP N: body" shape produced by [`http_status_message`].
pub fn actionable_error_hint(operation: &str, code: &str) -> Option<&'static str> {
    match (operation, code) {
        ("refresh", "token_stale") => Some(
            "your stored license is too stale to refresh. Reactivate with: fallow license activate --trial --email <addr>",
        ),
        ("refresh", "invalid_token") => Some(
            "your stored license token is missing required claims. Reactivate with: fallow license activate --trial --email <addr>",
        ),
        // Trial + refresh are license-JWT flows: a stale / invalid JWT is
        // fixed by reactivating via the trial endpoint.
        ("refresh" | "trial", "unauthorized") => Some(
            "authentication failed. Reactivate with: fallow license activate --trial --email <addr>",
        ),
        // upload-inventory uses a separate API key (`fallow_live_k1_*`), not
        // the license JWT. Reactivating the trial does NOT rotate the API
        // key. Point users at key generation instead.
        ("upload-inventory", "unauthorized") => Some(
            "authentication failed. Generate an API key at https://fallow.cloud/settings#api-keys and set FALLOW_API_KEY on the runner. Note: this key is separate from the license JWT; `fallow license activate --trial` will not fix this error.",
        ),
        ("trial", "rate_limit_exceeded") => Some(
            "trial creation is rate-limited to 5 per hour per IP. Wait an hour or retry from a different network (in CI, start the trial locally and set FALLOW_LICENSE on the runner).",
        ),
        ("upload-inventory", "payload_too_large") => Some(
            "inventory exceeds the 200,000-function server limit. Scope the walk with --exclude-paths, or open an issue if this is a legitimately large repo.",
        ),
        _ => None,
    }
}

/// Abstraction over an HTTP response's status + body accessors.
///
/// Implemented for `http::Response<ureq::Body>` and exposed as a trait so
/// error-path tests can substitute a lightweight stub without a real network
/// round-trip.
pub trait ResponseBodyReader {
    /// HTTP status code (200, 401, 429, ...).
    fn status(&self) -> u16;
    /// Deserialize the response body as JSON into `T`.
    fn read_json<T: DeserializeOwned>(&mut self) -> Result<T, ureq::Error>;
    /// Read the response body as a UTF-8 string.
    fn read_to_string(&mut self) -> Result<String, ureq::Error>;
}

impl ResponseBodyReader for http::Response<ureq::Body> {
    fn status(&self) -> u16 {
        self.status().as_u16()
    }

    fn read_json<T: DeserializeOwned>(&mut self) -> Result<T, ureq::Error> {
        self.body_mut().read_json::<T>()
    }

    fn read_to_string(&mut self) -> Result<String, ureq::Error> {
        self.body_mut().read_to_string()
    }
}

/// Format a non-2xx response into a user-facing error string.
///
/// Tries to parse the body as an [`ErrorEnvelope`]. When the envelope has a
/// known `code` for the given `operation`, the mapped hint is returned with
/// the HTTP status and code appended. Otherwise the backend's `message`
/// (or raw body) is appended to a generic "HTTP N" line.
pub fn http_status_message(response: &mut impl ResponseBodyReader, operation: &str) -> String {
    let status = response.status();
    let body = response.read_to_string().unwrap_or_default();
    let envelope: Option<ErrorEnvelope> = serde_json::from_str(&body).ok();
    if let Some(envelope) = envelope.as_ref()
        && let Some(code) = envelope.code.as_deref()
        && let Some(hint) = actionable_error_hint(operation, code)
    {
        return format!("{hint} (HTTP {status}, code {code})");
    }
    let body_suffix = match envelope.as_ref().and_then(|e| e.message.as_deref()) {
        Some(message) if !message.trim().is_empty() => format!(": {}", message.trim()),
        _ if !body.trim().is_empty() => format!(": {}", body.trim()),
        _ => String::new(),
    };
    format!("{operation} request failed with HTTP {status}{body_suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StubResponse {
        status: u16,
        body: String,
    }

    impl ResponseBodyReader for StubResponse {
        fn status(&self) -> u16 {
            self.status
        }

        fn read_json<T: DeserializeOwned>(&mut self) -> Result<T, ureq::Error> {
            unreachable!("error-path tests do not read JSON")
        }

        fn read_to_string(&mut self) -> Result<String, ureq::Error> {
            Ok(std::mem::take(&mut self.body))
        }
    }

    #[test]
    fn refresh_token_stale_hint_points_to_reactivation() {
        let mut response = StubResponse {
            status: 401,
            body: r#"{"error":true,"message":"token stale","code":"token_stale"}"#.to_owned(),
        };
        let message = http_status_message(&mut response, "refresh");
        assert!(
            message.contains("Reactivate with: fallow license activate --trial"),
            "expected reactivation hint, got: {message}"
        );
        assert!(message.contains("token_stale"));
    }

    #[test]
    fn refresh_invalid_token_hint_points_to_reactivation() {
        let mut response = StubResponse {
            status: 401,
            body: r#"{"error":true,"code":"invalid_token"}"#.to_owned(),
        };
        let message = http_status_message(&mut response, "refresh");
        assert!(message.contains("missing required claims"));
        assert!(message.contains("invalid_token"));
    }

    #[test]
    fn upload_inventory_unauthorized_points_to_api_keys_not_trial() {
        let mut response = StubResponse {
            status: 401,
            body: r#"{"error":true,"code":"unauthorized"}"#.to_owned(),
        };
        let message = http_status_message(&mut response, "upload-inventory");
        // API keys are a distinct secret from the license JWT. Sending trial
        // users to `license activate --trial` when they get a 401 on upload
        // is a dead-end support loop. The hint MUST both direct them to the
        // API-keys page AND explain that the trial flow won't fix it, so we
        // require the disqualifier to appear adjacent to "will not fix".
        // Regression test for BLOCK 3 from the public-readiness panel.
        assert!(
            message.contains("https://fallow.cloud/settings#api-keys"),
            "expected api-keys URL, got: {message}"
        );
        assert!(
            message.contains("FALLOW_API_KEY"),
            "expected FALLOW_API_KEY mention, got: {message}"
        );
        assert!(
            message.contains("will not fix"),
            "expected explicit 'will not fix this error' disqualifier so users do not retry via --trial; got: {message}"
        );
    }

    #[test]
    fn trial_rate_limit_hint_mentions_five_per_hour() {
        let mut response = StubResponse {
            status: 429,
            body: r#"{"error":true,"code":"rate_limit_exceeded"}"#.to_owned(),
        };
        let message = http_status_message(&mut response, "trial");
        assert!(message.contains("5 per hour per IP"));
        assert!(message.contains("FALLOW_LICENSE"));
    }

    #[test]
    fn unknown_code_falls_back_to_backend_message_when_present() {
        let mut response = StubResponse {
            status: 500,
            body: r#"{"error":true,"code":"checkout_error","message":"stripe returned no session url"}"#
                .to_owned(),
        };
        let message = http_status_message(&mut response, "refresh");
        assert!(message.starts_with("refresh request failed with HTTP 500"));
        assert!(
            message.ends_with(": stripe returned no session url"),
            "expected backend message on fallback, got: {message}"
        );
    }

    #[test]
    fn unknown_code_without_message_falls_back_to_raw_body() {
        let mut response = StubResponse {
            status: 500,
            body: r#"{"error":true,"code":"checkout_error"}"#.to_owned(),
        };
        let message = http_status_message(&mut response, "refresh");
        assert!(message.starts_with("refresh request failed with HTTP 500"));
        assert!(message.contains("checkout_error"));
    }

    #[test]
    fn empty_body_still_produces_minimal_message() {
        let mut response = StubResponse {
            status: 502,
            body: String::new(),
        };
        let message = http_status_message(&mut response, "trial");
        assert_eq!(message, "trial request failed with HTTP 502");
    }

    // Env-var assertions run in one test to avoid interleaving with parallel
    // tests that also touch `FALLOW_API_URL`. Restores the prior value.
    #[test]
    #[expect(unsafe_code, reason = "env var mutation requires unsafe")]
    fn api_url_respects_env_override_and_default() {
        let prior = std::env::var("FALLOW_API_URL").ok();

        // SAFETY: env mutation is unsafe because it is not thread-safe. This
        // test serializes its own writes and restores the prior value before
        // returning; no other test in this module touches FALLOW_API_URL.
        unsafe {
            std::env::remove_var("FALLOW_API_URL");
        }
        assert_eq!(
            api_url("/v1/coverage/repo/inventory"),
            "https://api.fallow.cloud/v1/coverage/repo/inventory",
        );

        // SAFETY: see the `remove_var` safety note above.
        unsafe {
            std::env::set_var("FALLOW_API_URL", "http://127.0.0.1:3000/");
        }
        assert_eq!(
            api_url("/v1/coverage/a/inventory"),
            "http://127.0.0.1:3000/v1/coverage/a/inventory",
        );

        // SAFETY: see the `remove_var` safety note above.
        unsafe {
            if let Some(value) = prior {
                std::env::set_var("FALLOW_API_URL", value);
            } else {
                std::env::remove_var("FALLOW_API_URL");
            }
        }
    }
}
