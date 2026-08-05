//! How the daemon should obtain its `Browser` instance.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub enum LaunchSpec {
    /// Spawn a fresh Chrome via `Browser::launch_with_config`.
    Launch {
        headless: bool,
        /// Optional: copy this profile dir before launching, point Chrome at the copy.
        from_profile: Option<PathBuf>,
        /// Optional: after launch, hydrate from a running Chrome (port or ws:// URL).
        clone_state_from: Option<String>,
        /// Disable stealth (filter_cdp + evasion script).
        no_stealth: bool,
        /// Resolved proxy URL (already picked from --proxy-file if that was used).
        proxy: Option<String>,
        /// Start in block-all JS mode (NoScript "Safest"-style).
        no_js: bool,
        /// Domains to always run JS on, regardless of `no_js`.
        js_allow: Vec<String>,
        /// Domains to always block JS on, even without `no_js`.
        js_block: Vec<String>,
    },
    /// Attach to a Chrome already running, via `Browser::connect_with_config`.
    Connect { ws_url: String },
}

/// Resolve `--proxy`/`--proxy-file` into a single proxy URL. `--proxy` wins
/// if both are somehow set (clap already rejects that combination via
/// `conflicts_with`, so this is just a defensive precedence rule).
/// A `--proxy-file` with multiple lines picks one at random per call.
pub fn resolve_proxy_spec(
    proxy: Option<&str>,
    proxy_file: Option<&Path>,
) -> Result<Option<String>, String> {
    if let Some(proxy) = proxy {
        return Ok(Some(proxy.to_owned()));
    }
    let Some(path) = proxy_file else {
        return Ok(None);
    };
    let contents = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read --proxy-file '{}': {}", path.display(), e))?;
    let lines: Vec<&str> = contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    if lines.is_empty() {
        return Err(format!(
            "--proxy-file '{}' does not contain a proxy",
            path.display()
        ));
    }
    use std::time::{SystemTime, UNIX_EPOCH};
    let idx = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as usize % lines.len())
        .unwrap_or(0);
    Ok(Some(lines[idx].to_owned()))
}

impl LaunchSpec {
    pub fn is_live(&self) -> bool {
        matches!(self, LaunchSpec::Connect { .. })
    }
}

/// Resolve a `--cdp <spec>` argument (port, `ws://` URL, or `auto`) into a
/// WebSocket URL. `auto` scans 9222-9229 for a responsive Chrome.
pub fn resolve_cdp_spec(spec: &str) -> Result<String, String> {
    let trimmed = spec.trim();
    if trimmed.eq_ignore_ascii_case("auto") {
        return auto_connect().map(|(_, url)| url);
    }
    if trimmed.starts_with("ws://") || trimmed.starts_with("wss://") {
        return Ok(trimmed.to_string());
    }
    let port: u16 = trimmed.parse().map_err(|_| {
        format!(
            "Invalid --cdp spec '{}': expected port, ws:// URL, or 'auto'",
            spec
        )
    })?;
    eoka::cdp::discover::discover_browser_ws("127.0.0.1", port)
        .map_err(|e| format!("Failed to discover Chrome on 127.0.0.1:{}: {}", port, e))
}

/// Find the first port in 9222..=9229 with a responsive Chrome DevTools endpoint.
pub fn auto_connect() -> Result<(u16, String), String> {
    eoka::cdp::discover::auto_connect(9222, 9229)
        .ok_or_else(|| "No running Chrome found on ports 9222-9229".to_string())
}

/// Internal session-name suffix so daemons in different modes don't collide.
/// Headed and headless launches are kept separate so `--headed` always
/// produces a visible window even if a headless daemon was running.
pub fn session_suffix(spec: &LaunchSpec) -> &'static str {
    match spec {
        LaunchSpec::Launch { headless: true, .. } => "",
        LaunchSpec::Launch {
            headless: false, ..
        } => "-headed",
        LaunchSpec::Connect { .. } => "-live",
    }
}
