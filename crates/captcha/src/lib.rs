//! Optional CAPTCHA solver integrations for the eoka ecosystem.
//!
//! This crate deliberately contains no browser automation. Consumers provide
//! challenge parameters and decide how to apply the returned token.

#[cfg(feature = "anti-captcha")]
mod anti_captcha;

#[cfg(feature = "anti-captcha")]
pub use anti_captcha::{AntiCaptcha, CaptchaSolution};
