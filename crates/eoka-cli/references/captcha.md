# CAPTCHA

## Install

Install from crates.io on any supported platform, or use the macOS ARM64 GitHub release installer.

```bash
# Any platform
cargo install eoka-cli

# macOS Apple Silicon (no Rust toolchain)
curl -fsSL https://raw.githubusercontent.com/cbxss/eoka-tools/main/crates/eoka-cli/scripts/install.sh | sh
```

Ensure `$HOME/.local/bin` is on `PATH`.

## Use

```bash
export ANTI_CAPTCHA_KEY='…'
eoka captcha solve --captcha-type amazon_waf --website-url https://parks.sonomacounty.ca.gov/ --website-key <key> --iv <iv> --context <context>
```

AWS WAF values come from `window.gokuProps`. Set the returned `token` as the `aws-waf-token` cookie, use the returned `user_agent`, then reload the page.
