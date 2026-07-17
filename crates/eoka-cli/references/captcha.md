# CAPTCHA

## Install

Install from crates.io on any supported platform, or use the macOS ARM64 GitHub release binary.

```bash
# Any platform
cargo install eoka-cli

# macOS Apple Silicon (no Rust toolchain)
curl -LO https://github.com/cbxss/eoka-tools/releases/download/eoka-cli-v0.1.1/eoka-cli-v0.1.1-aarch64-apple-darwin.tar.gz
tar -xzf eoka-cli-v0.1.1-aarch64-apple-darwin.tar.gz --strip-components=1 '*/eoka'
mkdir -p "$HOME/.local/bin" && install -m 755 eoka "$HOME/.local/bin/eoka"
```

Ensure `$HOME/.local/bin` is on `PATH`.

## Use

```bash
export ANTI_CAPTCHA_KEY='…'
eoka captcha solve --captcha-type amazon_waf --website-url https://parks.sonomacounty.ca.gov/ --website-key <key> --iv <iv> --context <context>
```

AWS WAF values come from `window.gokuProps`. Set the returned `token` as the `aws-waf-token` cookie, use the returned `user_agent`, then reload the page.
