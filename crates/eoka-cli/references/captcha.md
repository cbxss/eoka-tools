# CAPTCHA

## Install

Install from crates.io on any supported platform.

```bash
cargo install eoka-cli
```

Ensure `$HOME/.local/bin` is on `PATH`.

## Use

```bash
export ANTI_CAPTCHA_KEY='…'
eoka captcha solve --captcha-type amazon_waf --website-url https://parks.sonomacounty.ca.gov/ --website-key <key> --iv <iv> --context <context>
```

AWS WAF values come from `window.gokuProps`. Set the returned `token` as the `aws-waf-token` cookie, use the returned `user_agent`, then reload the page.

For reCAPTCHA or hCaptcha flows, inject the token into the active browser session:

```bash
eoka captcha solve --captcha-type recaptcha_v2 --website-url https://target.com --website-key <site-key>
eoka captcha solve --captcha-type recaptcha_v2_enterprise --website-url https://target.com --website-key <site-key> --enterprise-payload '{"s":"optional-render-token"}' --api-domain www.google.com
eoka captcha solve --captcha-type recaptcha_v3 --website-url https://target.com --website-key <site-key> --page-action submit --inject
eoka captcha inject <token> --captcha-type recaptcha
eoka captcha inject <token> --captcha-type hcaptcha --callback window.onCaptchaSolved
eoka captcha inject <token> --captcha-type recaptcha --click-after "text:Continue Booking"
```

Use `recaptcha_v2` for standard checkbox or invisible reCAPTCHA. Use
`recaptcha_v2_enterprise` only when the page loads its widget from
`recaptcha/enterprise`; it submits Anti-Captcha's Enterprise v2 task type.
Pass `--enterprise-payload` only when the site's `grecaptcha.enterprise.render`
configuration includes extra parameters such as `s`.

Injection sets the common hidden response fields, dispatches form events, and calls discovered grecaptcha/hCaptcha callbacks. Use `--callback` when the page exposes a known callback. Some pages require the submit or continuation control to be clicked again after token injection; use `--click-after` for that retry click.
