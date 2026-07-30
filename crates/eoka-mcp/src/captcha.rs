#[derive(Debug, Clone)]
pub struct CaptchaInfo {
    pub captcha_type: String,
    pub sitekey: String,
}

pub async fn detect_captcha_on_page(page: &eoka_server::eoka::Page) -> Option<CaptchaInfo> {
    let hcaptcha_script = r#"
        (() => document.querySelector('[data-sitekey]')?.getAttribute('data-sitekey') || null)()
    "#;
    if let Ok(result) = page.evaluate::<serde_json::Value>(hcaptcha_script).await {
        if let Some(sitekey) = result.as_str().filter(|key| !key.is_empty()) {
            return Some(CaptchaInfo {
                captcha_type: "hcaptcha".into(),
                sitekey: sitekey.into(),
            });
        }
    }

    let recaptcha_script = r#"
        (() => {
            if (![...document.scripts].some(s => s.src.includes('recaptcha'))) return null;
            return document.documentElement.innerHTML.match(/"sitekey"\s*:\s*"([^"]+)"/)?.[1] || null;
        })()
    "#;
    if let Ok(result) = page.evaluate::<serde_json::Value>(recaptcha_script).await {
        if let Some(sitekey) = result.as_str().filter(|key| !key.is_empty()) {
            return Some(CaptchaInfo {
                captcha_type: "recaptcha".into(),
                sitekey: sitekey.into(),
            });
        }
    }
    None
}
