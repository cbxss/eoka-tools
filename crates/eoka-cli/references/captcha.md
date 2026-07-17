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
