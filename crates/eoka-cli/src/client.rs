//! IPC client: connect to daemon, send command, receive response.

use std::process::Command as StdCommand;
use std::time::Duration;
use tokio::net::UnixStream;

use crate::launch_spec::LaunchSpec;
use crate::protocol::{read_msg, write_msg, Request, Response};
use crate::session;

/// Connect to the daemon for a session. Auto-launches if needed.
pub async fn send_command(
    session_name: &str,
    request: Request,
    spec: LaunchSpec,
) -> anyhow::Result<Response> {
    let sock = session::socket_path(session_name);

    let stream = match UnixStream::connect(&sock).await {
        Ok(s) => s,
        Err(_) => {
            launch_daemon(session_name, &spec)?;
            wait_for_socket(session_name).await?
        }
    };

    let (mut reader, mut writer) = stream.into_split();
    write_msg(&mut writer, &request).await?;
    let response: Response = read_msg(&mut reader).await?;
    Ok(response)
}

/// Wait for daemon socket to appear (up to 5s).
async fn wait_for_socket(session_name: &str) -> anyhow::Result<UnixStream> {
    let sock = session::socket_path(session_name);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if tokio::time::Instant::now() > deadline {
            anyhow::bail!(
                "Daemon failed to start (socket {} not found after 5s)",
                sock.display()
            );
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let Ok(s) = UnixStream::connect(&sock).await {
            return Ok(s);
        }
    }
}

/// Spawn daemon as a background process.
fn launch_daemon(session_name: &str, spec: &LaunchSpec) -> anyhow::Result<()> {
    session::ensure_runtime_dir()?;

    let exe = std::env::current_exe()?;
    let mut cmd = StdCommand::new(exe);
    cmd.arg("--daemon").arg("--session").arg(session_name);

    match spec {
        LaunchSpec::Launch {
            headless,
            from_profile,
            clone_state_from,
            no_stealth,
            proxy,
        } => {
            if !*headless {
                cmd.arg("--headed");
            }
            if let Some(p) = from_profile {
                cmd.arg("--from-profile").arg(p);
            }
            if let Some(s) = clone_state_from {
                cmd.arg("--clone-state-from").arg(s);
            }
            if *no_stealth {
                cmd.arg("--no-stealth");
            }
            // Already resolved (literal value or the one line picked from
            // --proxy-file) — forward as --proxy so the daemon subprocess
            // doesn't re-resolve --proxy-file and pick a different line.
            if let Some(p) = proxy {
                cmd.arg("--proxy").arg(p);
            }
        }
        LaunchSpec::Connect { ws_url } => {
            cmd.arg("--cdp").arg(ws_url);
        }
    }

    let log_path = session::socket_path(session_name).with_extension("log");
    let log_file = std::fs::File::create(&log_path)?;

    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(log_file);

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    cmd.spawn()?;
    eprintln!(
        "[eoka] daemon starting (session={}, log={})",
        session_name,
        log_path.display()
    );
    Ok(())
}

/// Check if daemon is running for a session using the socket file.
pub fn is_daemon_running(session_name: &str) -> bool {
    let sock = session::socket_path(session_name);
    // Try a synchronous connect to the socket; if it succeeds, the daemon is alive
    std::os::unix::net::UnixStream::connect(&sock).is_ok()
}

/// Kill daemon for a session by sending shutdown command, falling back to SIGTERM.
pub fn kill_daemon(session_name: &str) -> anyhow::Result<bool> {
    let sock = session::socket_path(session_name);

    // Try graceful shutdown via the protocol
    if let Ok(stream) = std::os::unix::net::UnixStream::connect(&sock) {
        // We can't easily do length-prefixed IO synchronously, so just drop and
        // fall through to PID-based kill. The daemon will clean up on disconnect.
        drop(stream);
    }

    // PID-based kill as fallback
    let pid_path = session::pid_path(session_name);
    let killed = if let Ok(pid_str) = std::fs::read_to_string(&pid_path) {
        if let Ok(pid) = pid_str.trim().parse::<u32>() {
            // Use `kill` command instead of unsafe libc
            StdCommand::new("kill")
                .arg(pid.to_string())
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        } else {
            false
        }
    } else {
        false
    };

    // Clean up stale files
    let _ = std::fs::remove_file(&sock);
    let _ = std::fs::remove_file(&pid_path);
    Ok(killed)
}
