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

/// Ensure the runtime directory exists.
pub fn ensure_runtime_dir() -> std::io::Result<()> {
    std::fs::create_dir_all(runtime_dir())
}
