//! How the daemon should obtain its `Browser` instance.

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum LaunchSpec {
    /// Spawn a fresh Chrome via `Browser::launch_with_config`.
    Launch {
        headless: bool,
        /// Optional: copy this profile dir before launching, point Chrome at the copy.
        from_profile: Option<PathBuf>,
        /// Optional: after launch, hydrate from a running Chrome (port or ws:// URL).
        clone_state_from: Option<String>,
    },
    /// Attach to a Chrome already running, via `Browser::connect_with_config`.
    Connect { ws_url: String },
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

/// Internal session-name suffix so connect-mode daemons don't collide with
/// launch-mode daemons.
pub fn session_suffix(spec: &LaunchSpec) -> &'static str {
    match spec {
        LaunchSpec::Launch { .. } => "",
        LaunchSpec::Connect { .. } => "-live",
    }
}
