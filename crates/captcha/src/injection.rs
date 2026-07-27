//! Shared browser-side delivery for solved CAPTCHA tokens.
//!
//! The script is intentionally browser-library agnostic: callers execute the
//! returned JavaScript through the browser abstraction they use.

use std::fmt;

const CAPTCHA_INJECT_JS: &str = include_str!("captcha_inject.js");

/// The widget family that should receive a solved CAPTCHA token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptchaInjectionKind {
    Auto,
    Recaptcha,
    Hcaptcha,
    Turnstile,
}

impl CaptchaInjectionKind {
    /// Returns the value understood by the shared browser-side script.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Recaptcha => "recaptcha",
            Self::Hcaptcha => "hcaptcha",
            Self::Turnstile => "turnstile",
        }
    }
}

/// A validation error for an unsupported CAPTCHA injection kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptchaInjectionKindError {
    kind: String,
}

impl fmt::Display for CaptchaInjectionKindError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Unsupported captcha injection type '{}'. Use auto, recaptcha, hcaptcha, or turnstile.",
            self.kind
        )
    }
}

impl std::error::Error for CaptchaInjectionKindError {}

/// Normalizes public solver names to the relevant widget family.
pub fn parse_captcha_inject_kind(
    kind: &str,
) -> Result<CaptchaInjectionKind, CaptchaInjectionKindError> {
    match kind.to_lowercase().as_str() {
        "" | "auto" => Ok(CaptchaInjectionKind::Auto),
        "recaptcha"
        | "recaptcha_v2"
        | "recaptcha_v2_enterprise"
        | "recaptcha_v3"
        | "recaptcha_enterprise" => Ok(CaptchaInjectionKind::Recaptcha),
        "hcaptcha" => Ok(CaptchaInjectionKind::Hcaptcha),
        "turnstile" => Ok(CaptchaInjectionKind::Turnstile),
        _ => Err(CaptchaInjectionKindError { kind: kind.into() }),
    }
}

/// Builds JavaScript that delivers `token` to the selected CAPTCHA widget.
///
/// The token and optional callback are JSON encoded so untrusted values cannot
/// alter the generated script.
pub fn build_captcha_inject_js(
    token: &str,
    kind: CaptchaInjectionKind,
    callback: Option<&str>,
) -> String {
    let token = serde_json::to_string(token).expect("serializing a string cannot fail");
    let kind = serde_json::to_string(kind.as_str()).expect("serializing a string cannot fail");
    let callback = serde_json::to_string(callback.unwrap_or_default())
        .expect("serializing a string cannot fail");
    format!("{CAPTCHA_INJECT_JS}({token},{kind},{callback})")
}

#[cfg(test)]
mod tests {
    use super::{build_captcha_inject_js, parse_captcha_inject_kind, CaptchaInjectionKind};

    #[test]
    fn normalizes_supported_kinds() {
        assert_eq!(
            parse_captcha_inject_kind("recaptcha_v2_enterprise").unwrap(),
            CaptchaInjectionKind::Recaptcha
        );
        assert_eq!(
            parse_captcha_inject_kind("hcaptcha").unwrap(),
            CaptchaInjectionKind::Hcaptcha
        );
        assert!(parse_captcha_inject_kind("amazon_waf").is_err());
    }

    #[test]
    fn script_escapes_dynamic_values() {
        let script = build_captcha_inject_js(
            "tok\"en",
            CaptchaInjectionKind::Recaptcha,
            Some("window.onCaptcha"),
        );

        assert!(script.contains("\"tok\\\"en\""));
        assert!(script.contains("\"recaptcha\""));
        assert!(script.contains("\"window.onCaptcha\""));
        assert!(script.contains("textarea[name=\"g-recaptcha-response\"]"));
    }
}
