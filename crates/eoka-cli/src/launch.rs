//! CLI flags → `LaunchSpec`/session-name resolution.

use crate::cli::Cli;
use crate::launch_spec::{self, LaunchSpec};

/// Map CLI flags to a `LaunchSpec`. Resolves --cdp ports to ws:// URLs eagerly.
pub(crate) fn resolve_launch_spec(cli: &Cli) -> Result<LaunchSpec, String> {
    if let Some(spec) = &cli.cdp {
        let ws_url = launch_spec::resolve_cdp_spec(spec)?;
        return Ok(LaunchSpec::Connect { ws_url });
    }
    if cli.auto_connect {
        let (_port, ws_url) = launch_spec::auto_connect()?;
        return Ok(LaunchSpec::Connect { ws_url });
    }
    Ok(LaunchSpec::Launch {
        headless: !cli.headed,
        from_profile: cli
            .from_profile
            .as_deref()
            .map(resolve_profile_spec)
            .transpose()?,
        clone_state_from: cli.clone_state_from.clone(),
        no_stealth: cli.no_stealth,
        proxy: launch_spec::resolve_proxy_spec(cli.proxy.as_deref(), cli.proxy_file.as_deref())?,
        no_js: cli.no_js,
        js_allow: cli.js_allow.clone(),
        js_block: cli.js_block.clone(),
    })
}

fn resolve_profile_spec(spec: &str) -> Result<std::path::PathBuf, String> {
    if spec == "auto" {
        return crate::handler::profile::default_profile_dir()
            .ok_or_else(|| "Could not autodetect Chrome profile dir".to_string());
    }
    let p = std::path::PathBuf::from(spec);
    if !p.exists() {
        return Err(format!("Profile path does not exist: {}", p.display()));
    }
    Ok(p)
}

pub(crate) fn effective_session(cli: &Cli, spec: &LaunchSpec) -> String {
    format!("{}{}", cli.session, launch_spec::session_suffix(spec))
}
