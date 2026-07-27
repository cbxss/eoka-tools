//! CAPTCHA solving and token-delivery helpers for the eoka ecosystem.
//!
//! This crate deliberately contains no browser automation. Consumers provide
//! challenge parameters, then execute the shared token-delivery script in the
//! browser context they own.

#[cfg(feature = "anti-captcha")]
mod anti_captcha;
mod injection;

#[cfg(feature = "anti-captcha")]
pub use anti_captcha::{AntiCaptcha, CaptchaSolution};
pub use injection::{build_captcha_inject_js, parse_captcha_inject_kind, CaptchaInjectionKind};
