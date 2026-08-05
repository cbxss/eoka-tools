//! `eoka sessions` — list every session with a socket in the runtime dir.

use serde_json::{json, Value};

use crate::{client, output, protocol, session};

/// List every session with a socket file in the runtime dir, live or stale,
/// like `tmux ls`. Names are the fully-qualified session ids (e.g.
/// `default-headed`), matching what `--session`/`session_suffix` produce.
pub(crate) fn print_sessions(json_mode: bool) {
    let sessions = session::list_sessions();

    if json_mode {
        let rows: Vec<Value> = sessions
            .iter()
            .map(|name| {
                json!({
                    "session": name,
                    "running": client::is_daemon_running(name),
                    "socket": session::socket_path(name).to_string_lossy(),
                })
            })
            .collect();
        output::print_response(&protocol::Response::ok(json!({ "sessions": rows })), true);
        return;
    }

    if sessions.is_empty() {
        println!("No sessions found");
        return;
    }

    let name_width = sessions.iter().map(String::len).max().unwrap_or(0).max(7);
    println!("{:<name_width$}  RUNNING  SOCKET", "SESSION");
    for name in &sessions {
        let running = client::is_daemon_running(name);
        println!(
            "{:<name_width$}  {:<7}  {}",
            name,
            if running { "yes" } else { "no" },
            session::socket_path(name).display()
        );
    }
}
