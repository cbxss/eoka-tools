use std::path::PathBuf;

/// Directory for all eoka runtime files (sockets, PIDs).
fn runtime_dir() -> PathBuf {
    let base = std::env::var("XDG_RUNTIME_DIR")
        .or_else(|_| std::env::var("TMPDIR"))
        .unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(base).join("eoka")
}

/// Unix socket path for a named session.
pub fn socket_path(session: &str) -> PathBuf {
    runtime_dir().join(format!("eoka-{}.sock", session))
}

/// PID file path for a named session.
pub fn pid_path(session: &str) -> PathBuf {
    runtime_dir().join(format!("eoka-{}.pid", session))
}

/// Durable Chrome profile directory for a named session. Survives daemon
/// restarts, unlike the runtime dir. Overridable via `EOKA_PROFILE_DIR`.
pub fn profile_dir(session: &str) -> PathBuf {
    if let Ok(base) = std::env::var("EOKA_PROFILE_DIR") {
        return PathBuf::from(base).join(session);
    }
    let base = std::env::var("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_default();
            if home.is_empty() {
                std::env::temp_dir()
            } else {
                PathBuf::from(home).join(".local/state")
            }
        });
    base.join("eoka/profiles").join(session)
}

/// Ensure the runtime directory exists.
pub fn ensure_runtime_dir() -> std::io::Result<()> {
    std::fs::create_dir_all(runtime_dir())
}

/// Names of all sessions that have ever left a socket file behind (live or
/// stale — callers check liveness separately via `client::is_daemon_running`).
pub fn list_sessions() -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(runtime_dir()) else {
        return Vec::new();
    };
    let mut sessions: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter_map(|name| {
            name.strip_prefix("eoka-")
                .and_then(|rest| rest.strip_suffix(".sock"))
                .map(str::to_owned)
        })
        .collect();
    sessions.sort();
    sessions
}
