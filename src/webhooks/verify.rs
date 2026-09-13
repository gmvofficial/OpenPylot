//! Webhook signature verification.
//!
//! # Why
//!
//! The webhook port is public by design — providers have to reach it — and
//! until now nothing checked that a request actually came from the provider it
//! claimed to. Anyone who could reach the port could forge a GitHub or Slack
//! event, which at minimum sends the user a Telegram notification they will
//! believe, and at worst drives whatever the event queue is later wired to.
//!
//! GitHub and Slack both sign their deliveries. This verifies those signatures.
//!
//! # The unconfigured case
//!
//! A secret that is not configured cannot be verified. Rejecting everything
//! would break every existing install on upgrade; accepting silently is what
//! got us here. So: **when a secret is configured for a provider, its signature
//! is required and a bad one is rejected; when it is not, the request is
//! accepted and startup prints a loud warning saying how to fix it.** That is
//! the standard migration path, and it makes the insecure state visible rather
//! than invisible.

use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Secrets used to verify inbound webhooks.
#[derive(Debug, Clone, Default)]
pub struct WebhookSecrets {
    /// GitHub webhook secret (`X-Hub-Signature-256`).
    pub github: Option<String>,
    /// Slack signing secret (`X-Slack-Signature`).
    pub slack: Option<String>,
}

impl WebhookSecrets {
    /// Providers with no secret configured, for the startup warning.
    pub fn unverified(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.github.is_none() {
            out.push("github");
        }
        if self.slack.is_none() {
            out.push("slack");
        }
        out
    }

    pub fn any_configured(&self) -> bool {
        self.github.is_some() || self.slack.is_some()
    }
}

/// Why a request was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rejection {
    /// The provider signs its requests and this one carried no signature.
    MissingSignature,
    /// The signature was present and did not match.
    BadSignature,
    /// The signature was well-formed but the request is too old to accept.
    ///
    /// Without this, a signature captured once can be replayed forever — the
    /// signature stays valid because the body never changes.
    Stale,
    /// The signature header could not be parsed at all.
    Malformed,
}

impl Rejection {
    /// A message safe to return to the caller.
    ///
    /// Deliberately vague about *which* check failed: telling an attacker
    /// whether their signature was merely stale versus wrong is free
    /// information about the secret.
    pub fn message(&self) -> &'static str {
        "Webhook signature verification failed."
    }

    pub fn detail(&self) -> &'static str {
        match self {
            Rejection::MissingSignature => "no signature header",
            Rejection::BadSignature => "signature mismatch",
            Rejection::Stale => "timestamp outside the accepted window",
            Rejection::Malformed => "malformed signature header",
        }
    }
}

/// How far out of date a Slack request may be, in seconds.
///
/// Slack's own guidance is five minutes; beyond that the delivery is assumed
/// replayed rather than merely slow.
pub const MAX_SKEW_SECS: i64 = 5 * 60;

/// Verify a GitHub delivery.
///
/// GitHub signs the raw body with HMAC-SHA256 and sends
/// `X-Hub-Signature-256: sha256=<hex>`.
///
/// Returns `Ok(())` when the secret is `None` — see the module docs on the
/// unconfigured case.
pub fn github(secret: Option<&str>, signature: Option<&str>, body: &[u8]) -> Result<(), Rejection> {
    let Some(secret) = secret else {
        return Ok(());
    };
    let Some(signature) = signature else {
        return Err(Rejection::MissingSignature);
    };

    let hex = signature
        .strip_prefix("sha256=")
        .ok_or(Rejection::Malformed)?;
    let expected = hex_hmac(secret.as_bytes(), body);

    if constant_time_eq(hex.as_bytes(), expected.as_bytes()) {
        Ok(())
    } else {
        Err(Rejection::BadSignature)
    }
}

/// Verify a Slack delivery.
///
/// Slack signs `v0:<timestamp>:<body>` and sends `X-Slack-Signature: v0=<hex>`
/// alongside `X-Slack-Request-Timestamp`. The timestamp is part of the signed
/// payload *and* checked against the clock, which is what makes replay
/// detectable — a captured request stops being accepted once it ages out.
pub fn slack(
    secret: Option<&str>,
    signature: Option<&str>,
    timestamp: Option<&str>,
    body: &[u8],
    now_unix: i64,
) -> Result<(), Rejection> {
    let Some(secret) = secret else {
        return Ok(());
    };
    let (Some(signature), Some(timestamp)) = (signature, timestamp) else {
        return Err(Rejection::MissingSignature);
    };

    let sent_at: i64 = timestamp.parse().map_err(|_| Rejection::Malformed)?;
    if (now_unix - sent_at).abs() > MAX_SKEW_SECS {
        return Err(Rejection::Stale);
    }

    let hex = signature.strip_prefix("v0=").ok_or(Rejection::Malformed)?;

    let mut signed = Vec::with_capacity(body.len() + timestamp.len() + 4);
    signed.extend_from_slice(b"v0:");
    signed.extend_from_slice(timestamp.as_bytes());
    signed.push(b':');
    signed.extend_from_slice(body);

    let expected = hex_hmac(secret.as_bytes(), &signed);

    if constant_time_eq(hex.as_bytes(), expected.as_bytes()) {
        Ok(())
    } else {
        Err(Rejection::BadSignature)
    }
}

/// Lowercase hex HMAC-SHA256.
fn hex_hmac(key: &[u8], message: &[u8]) -> String {
    // `new_from_slice` only fails for key lengths this algorithm accepts any
    // of, so the expect is unreachable in practice.
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(message);
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Byte comparison whose duration depends only on the lengths.
///
/// A short-circuiting `==` on a MAC leaks how much of a forged signature was
/// correct, which is enough to recover it a byte at a time.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Current Unix time in seconds.
pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "It's a Secret to Everybody";
    const BODY: &[u8] = b"Hello, World!";

    /// The signature GitHub's own documentation gives for this secret and body.
    /// Matching a published vector proves the construction is right, not merely
    /// self-consistent.
    const GITHUB_DOC_SIGNATURE: &str =
        "sha256=757107ea0eb2509fc211221cce984b8a37570b6d7586c22c46f4379c8b043e17";

    // ── GitHub ───────────────────────────────────────────────────────

    #[test]
    fn github_matches_the_published_test_vector() {
        assert!(github(Some(SECRET), Some(GITHUB_DOC_SIGNATURE), BODY).is_ok());
    }

    #[test]
    fn github_rejects_a_tampered_body() {
        // The whole point: a forged payload must not pass.
        assert_eq!(
            github(Some(SECRET), Some(GITHUB_DOC_SIGNATURE), b"Goodbye, World!"),
            Err(Rejection::BadSignature)
        );
    }

    #[test]
    fn github_rejects_the_wrong_secret() {
        assert_eq!(
            github(Some("not the secret"), Some(GITHUB_DOC_SIGNATURE), BODY),
            Err(Rejection::BadSignature)
        );
    }

    #[test]
    fn github_rejects_a_missing_signature_when_a_secret_is_set() {
        assert_eq!(
            github(Some(SECRET), None, BODY),
            Err(Rejection::MissingSignature)
        );
    }

    #[test]
    fn github_rejects_an_unprefixed_signature() {
        let bare = GITHUB_DOC_SIGNATURE.strip_prefix("sha256=").unwrap();
        assert_eq!(
            github(Some(SECRET), Some(bare), BODY),
            Err(Rejection::Malformed)
        );
    }

    #[test]
    fn github_rejects_a_sha1_signature() {
        // The old `X-Hub-Signature` scheme is broken; accepting it would let an
        // attacker downgrade to it.
        assert_eq!(
            github(Some(SECRET), Some("sha1=abcdef"), BODY),
            Err(Rejection::Malformed)
        );
    }

    #[test]
    fn github_accepts_anything_when_no_secret_is_configured() {
        // Documented behaviour: unconfigured means unverifiable, and rejecting
        // would break every existing install on upgrade.
        assert!(github(None, None, BODY).is_ok());
        assert!(github(None, Some("sha256=garbage"), BODY).is_ok());
    }

    // ── Slack ────────────────────────────────────────────────────────

    fn slack_signature(secret: &str, timestamp: &str, body: &[u8]) -> String {
        let mut signed = Vec::new();
        signed.extend_from_slice(b"v0:");
        signed.extend_from_slice(timestamp.as_bytes());
        signed.push(b':');
        signed.extend_from_slice(body);
        format!("v0={}", hex_hmac(secret.as_bytes(), &signed))
    }

    #[test]
    fn slack_accepts_a_correctly_signed_request() {
        let now = 1_700_000_000;
        let ts = now.to_string();
        let sig = slack_signature(SECRET, &ts, BODY);
        assert!(slack(Some(SECRET), Some(&sig), Some(&ts), BODY, now).is_ok());
    }

    #[test]
    fn slack_rejects_a_tampered_body() {
        let now = 1_700_000_000;
        let ts = now.to_string();
        let sig = slack_signature(SECRET, &ts, BODY);
        assert_eq!(
            slack(Some(SECRET), Some(&sig), Some(&ts), b"tampered", now),
            Err(Rejection::BadSignature)
        );
    }

    #[test]
    fn slack_rejects_a_replayed_request() {
        // A captured request stays correctly signed forever — only the clock
        // check stops it being replayed.
        let signed_at = 1_700_000_000;
        let ts = signed_at.to_string();
        let sig = slack_signature(SECRET, &ts, BODY);

        let much_later = signed_at + MAX_SKEW_SECS + 1;
        assert_eq!(
            slack(Some(SECRET), Some(&sig), Some(&ts), BODY, much_later),
            Err(Rejection::Stale)
        );
    }

    #[test]
    fn slack_accepts_a_request_just_inside_the_window() {
        let signed_at = 1_700_000_000;
        let ts = signed_at.to_string();
        let sig = slack_signature(SECRET, &ts, BODY);
        assert!(slack(Some(SECRET), Some(&sig), Some(&ts), BODY, signed_at + MAX_SKEW_SECS).is_ok());
    }

    #[test]
    fn slack_rejects_a_timestamp_too_far_in_the_future() {
        // Guards a clock-skew forgery as well as a stale replay.
        let signed_at = 1_700_000_000;
        let ts = signed_at.to_string();
        let sig = slack_signature(SECRET, &ts, BODY);
        assert_eq!(
            slack(Some(SECRET), Some(&sig), Some(&ts), BODY, signed_at - MAX_SKEW_SECS - 1),
            Err(Rejection::Stale)
        );
    }

    #[test]
    fn slack_rejects_a_signature_lifted_onto_a_different_timestamp() {
        // The timestamp is inside the signed payload, so moving it invalidates
        // the signature — this is what makes the replay window enforceable.
        let now = 1_700_000_000;
        let original = now.to_string();
        let sig = slack_signature(SECRET, &original, BODY);
        let forged = (now + 1).to_string();

        assert_eq!(
            slack(Some(SECRET), Some(&sig), Some(&forged), BODY, now),
            Err(Rejection::BadSignature)
        );
    }

    #[test]
    fn slack_rejects_a_missing_timestamp() {
        let now = 1_700_000_000;
        let sig = slack_signature(SECRET, &now.to_string(), BODY);
        assert_eq!(
            slack(Some(SECRET), Some(&sig), None, BODY, now),
            Err(Rejection::MissingSignature)
        );
    }

    #[test]
    fn slack_rejects_a_non_numeric_timestamp() {
        let now = 1_700_000_000;
        assert_eq!(
            slack(Some(SECRET), Some("v0=abc"), Some("not-a-number"), BODY, now),
            Err(Rejection::Malformed)
        );
    }

    #[test]
    fn slack_rejects_an_unprefixed_signature() {
        let now = 1_700_000_000;
        let ts = now.to_string();
        assert_eq!(
            slack(Some(SECRET), Some("deadbeef"), Some(&ts), BODY, now),
            Err(Rejection::Malformed)
        );
    }

    #[test]
    fn slack_accepts_anything_when_no_secret_is_configured() {
        assert!(slack(None, None, None, BODY, 0).is_ok());
    }

    // ── Primitives ───────────────────────────────────────────────────

    #[test]
    fn constant_time_eq_matches_the_semantics_of_eq() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn hex_hmac_is_lowercase_and_the_right_length() {
        let out = hex_hmac(b"k", b"m");
        assert_eq!(out.len(), 64, "SHA-256 is 32 bytes, 64 hex chars");
        assert!(out.chars().all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
    }

    // ── Reporting ────────────────────────────────────────────────────

    #[test]
    fn the_caller_facing_message_does_not_say_which_check_failed() {
        // Telling an attacker "stale" versus "mismatch" is free information.
        let messages: Vec<&str> = [
            Rejection::MissingSignature,
            Rejection::BadSignature,
            Rejection::Stale,
            Rejection::Malformed,
        ]
        .iter()
        .map(|r| r.message())
        .collect();

        assert!(messages.windows(2).all(|w| w[0] == w[1]), "{messages:?}");
    }

    #[test]
    fn unverified_providers_are_reported_for_the_startup_warning() {
        let none = WebhookSecrets::default();
        assert_eq!(none.unverified(), vec!["github", "slack"]);
        assert!(!none.any_configured());

        let partial = WebhookSecrets {
            github: Some("s".into()),
            slack: None,
        };
        assert_eq!(partial.unverified(), vec!["slack"]);
        assert!(partial.any_configured());

        let both = WebhookSecrets {
            github: Some("s".into()),
            slack: Some("s".into()),
        };
        assert!(both.unverified().is_empty());
    }
}
