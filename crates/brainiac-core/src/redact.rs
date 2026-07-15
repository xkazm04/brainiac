//! Secret redaction for anything lifted out of a raw LLM session.
//!
//! Brainiac ingests real transcripts and both stores distilled memory *content*
//! and serves a verbatim source *excerpt* through `memory_provenance`. Neither
//! had any scrubbing (UAT run 2026-07-13, finding H4): a credential pasted into a
//! session became a team-visible memory body and was handed, verbatim, to any
//! agent whose RLS admitted it. This is the firewall — applied where a raw string
//! crosses into a stored memory or an agent-facing payload.
//!
//! It is deliberately **recall-biased**: a false redaction (masking a non-secret)
//! is cheap; a missed credential is a breach. Patterns cover the high-value
//! shapes — private-key blocks, provider key prefixes, bearer/connection-string
//! secrets, and `key = value` where the key names a secret. It is not a
//! guarantee (no scanner is); it is the difference between "verbatim by default"
//! and "scrubbed by default", which is the finding.

use std::sync::LazyLock;

use regex::Regex;

const MASK: &str = "[REDACTED]";

/// Ordered redaction patterns. Each replaces its match (or a capture group) with
/// [`MASK`]. Compiled once. Order matters only for readability; matches are
/// applied in sequence over the accumulating string.
struct Rule {
    re: Regex,
    /// When `Some(n)`, only capture group `n` is masked (keeps the surrounding
    /// key/label so the redaction is legible); when `None`, the whole match is.
    group: Option<usize>,
}

static RULES: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    let r = |p: &str, group: Option<usize>| Rule {
        re: Regex::new(p).expect("static redaction regex"),
        group,
    };
    vec![
        // PEM private-key blocks (any type: RSA/EC/OPENSSH/PGP…), across lines.
        r(
            r"(?s)-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----.*?-----END [A-Z0-9 ]*PRIVATE KEY-----",
            None,
        ),
        // Connection strings with an inline password: scheme://user:PASSWORD@host.
        r(
            r"([a-zA-Z][a-zA-Z0-9+.\-]*://[^\s:/@]+:)([^\s@/]+)(@)",
            Some(2),
        ),
        // Well-known provider key/token shapes (prefix + body).
        r(r"\bsk-[A-Za-z0-9_\-]{16,}\b", None), // OpenAI-style
        r(r"\bbrk_[A-Za-z0-9_\-]{16,}\b", None), // Brainiac API tokens
        r(r"\bAKIA[0-9A-Z]{16}\b", None),       // AWS access key id
        r(r"\bgh[pousr]_[A-Za-z0-9]{20,}\b", None), // GitHub tokens
        r(r"\bxox[baprs]-[A-Za-z0-9\-]{10,}\b", None), // Slack tokens
        r(r"\bAIza[0-9A-Za-z_\-]{30,}\b", None), // Google API key
        // `Authorization: Bearer <token>` — the module doc promised bearer
        // coverage but no rule implemented it, so live bearer credentials pasted
        // into a session survived verbatim into a memory body. Masks only the
        // token so the `Bearer` label stays legible.
        r(r"(?i)\bbearer\s+([A-Za-z0-9._\-]{20,})", Some(1)),
        // A raw JWT anywhere (header.payload.signature), with or without a Bearer
        // prefix — these carry identity and are routinely pasted into transcripts.
        r(
            r"\beyJ[A-Za-z0-9_\-]+\.[A-Za-z0-9_\-]+\.[A-Za-z0-9_\-]*",
            None,
        ),
        // `key = value` / `key: value` where the key names a secret. Captures the
        // value (quoted or bare) and masks only it, keeping the label.
        //
        // The optional `(?:[a-z0-9]+[_-])?` prefix is load-bearing: `\btoken\b`
        // cannot match the `token` inside `access_token` / `refresh_token` /
        // `auth_token`, because `_` is a word character so there is no boundary —
        // the most common OAuth key names slipped through entirely.
        r(
            r#"(?i)\b(?:[a-z0-9]+[_-])?(?:api[_-]?key|secret|password|passwd|token|client[_-]?secret|access[_-]?key)\b\s*[:=]\s*(['"]?)([^\s'"]{6,})(['"]?)"#,
            Some(2),
        ),
    ]
});

/// Redact secrets from `input`, returning the scrubbed string. Idempotent — the
/// mask itself contains no secret shape, so re-running never changes a scrubbed
/// string. Cheap on clean text (each rule is a single scan; most miss).
pub fn redact(input: &str) -> String {
    let mut out = input.to_string();
    for rule in RULES.iter() {
        out = match rule.group {
            None => rule.re.replace_all(&out, MASK).into_owned(),
            Some(g) => rule
                .re
                .replace_all(&out, |caps: &regex::Captures| {
                    let whole = &caps[0];
                    let secret = &caps[g];
                    // Reconstruct the match with only the secret group masked.
                    whole.replacen(secret, MASK, 1)
                })
                .into_owned(),
        };
    }
    out
}

/// True if `redact` would change `input` — i.e. it contains at least one detected
/// secret. Useful for flagging/alerting without mutating.
pub fn contains_secret(input: &str) -> bool {
    RULES.iter().any(|rule| rule.re.is_match(input))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_the_common_shapes() {
        let cases = [
            "here is my key sk-abcdef0123456789ABCDEF please use it",
            "token brk_0123456789abcdefABCD is the api token",
            "aws key AKIAIOSFODNN7EXAMPLE in the log",
            "db is postgres://admin:sup3rs3cret@db.internal:5432/app",
            "PASSWORD=hunter2hunter2",
            "api_key: \"aBcD1234EfGh5678\"",
            "-----BEGIN RSA PRIVATE KEY-----\nMIIabc\n-----END RSA PRIVATE KEY-----",
        ];
        for c in cases {
            let out = redact(c);
            assert!(out.contains(MASK), "no redaction in: {c} -> {out}");
            assert!(contains_secret(c), "not detected: {c}");
        }
        // The password is masked; the host and scheme survive (legible redaction).
        let db = redact("postgres://admin:sup3rs3cret@db.internal:5432/app");
        assert!(db.contains("db.internal") && db.contains(MASK) && !db.contains("sup3rs3cret"));
    }

    #[test]
    fn masks_bearer_jwt_and_compound_oauth_keys() {
        // The module doc claimed bearer coverage that no rule implemented, and
        // `\btoken\b` could never match inside `access_token` (`_` is a word char,
        // so there's no boundary) — so live OAuth credentials survived redact()
        // into memory bodies and out through memory_provenance.
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dBjftJeZ4CVPmB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let cases = [
            format!("Authorization: Bearer {jwt}"),
            format!("curl -H 'authorization: bearer {jwt}'"),
            format!("the raw token is {jwt} btw"),
            "access_token= 9f8c7b6a5e4d3c2b1a09".to_string(),
            "refresh_token: \"r1_aBcD1234EfGh5678\"".to_string(),
            "auth_token = zzzz9999yyyy8888".to_string(),
        ];
        for c in &cases {
            let out = redact(c);
            assert!(out.contains(MASK), "no redaction in: {c} -> {out}");
            assert!(contains_secret(c), "not detected: {c}");
            assert!(!out.contains(jwt), "jwt survived: {out}");
            assert!(
                !out.contains("9f8c7b6a5e4d3c2b1a09"),
                "value survived: {out}"
            );
        }
        // The label survives so the redaction stays legible.
        let b = redact(&format!("Authorization: Bearer {jwt}"));
        assert!(
            b.to_lowercase().contains("bearer") && b.contains(MASK),
            "{b}"
        );
        // Still idempotent with the new rules.
        assert_eq!(redact(&b), b);
    }

    #[test]
    fn leaves_clean_text_untouched_and_is_idempotent() {
        let clean = "raise the refund-worker retry cap to 30s with jitter";
        assert_eq!(redact(clean), clean);
        assert!(!contains_secret(clean));
        let once = redact("PASSWORD=hunter2hunter2");
        assert_eq!(redact(&once), once, "redaction must be idempotent");
    }
}
