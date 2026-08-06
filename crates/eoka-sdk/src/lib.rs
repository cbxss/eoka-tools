pub mod launch_spec;
pub mod session;

mod client;

pub use client::{is_daemon_running, kill_daemon, send_command, EokaClient};
pub use eoka_protocol::*;
pub use launch_spec::{
    auto_connect, resolve_cdp_spec, resolve_proxy_spec, session_suffix, LaunchSpec,
};
