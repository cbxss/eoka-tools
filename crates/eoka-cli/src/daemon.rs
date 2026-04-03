//! Daemon process: listens on Unix socket, dispatches commands to Handler.

use std::time::{Duration, Instant};
use tokio::net::UnixListener;
use tokio::sync::mpsc;

use crate::handler::Handler;
use crate::protocol::{read_msg, write_msg, Request, Response};
use crate::session;

/// Default idle timeout: 30 minutes.
fn idle_timeout() -> Duration {
    let ms = std::env::var("EOKA_IDLE_TIMEOUT")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(30 * 60 * 1000);
    Duration::from_millis(ms)
}

pub async fn run(session_name: &str, headless: bool) -> anyhow::Result<()> {
    session::ensure_runtime_dir()?;

    let sock_path = session::socket_path(session_name);
    let pid_path = session::pid_path(session_name);

    // Clean up stale socket
    if sock_path.exists() {
        let _ = std::fs::remove_file(&sock_path);
    }

    let listener = UnixListener::bind(&sock_path)?;
    std::fs::write(&pid_path, std::process::id().to_string())?;

    eprintln!(
        "[eoka] daemon started (session={}, pid={})",
        session_name,
        std::process::id()
    );
    eprintln!("[eoka] socket: {}", sock_path.display());

    let mut handler = Handler::new(headless);
    let mut last_activity = Instant::now();
    let timeout = idle_timeout();

    // Signal handler sends shutdown notification via channel
    let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
    tokio::spawn(async move {
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();
        let mut sigint =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt()).unwrap();
        tokio::select! {
            _ = sigterm.recv() => eprintln!("[eoka] received SIGTERM"),
            _ = sigint.recv() => eprintln!("[eoka] received SIGINT"),
        }
        let _ = shutdown_tx.send(()).await;
    });

    loop {
        let accept = tokio::time::timeout(Duration::from_secs(10), listener.accept());

        tokio::select! {
            result = accept => {
                match result {
                    Ok(Ok((stream, _))) => {
                        last_activity = Instant::now();
                        let (mut reader, mut writer) = stream.into_split();

                        let req: Request = match read_msg(&mut reader).await {
                            Ok(r) => r,
                            Err(e) => {
                                eprintln!("[eoka] read error: {}", e);
                                continue;
                            }
                        };

                        if req.cmd == "shutdown" {
                            let _ = handler.handle("close", &serde_json::Value::Null).await;
                            let resp = Response::ok_text("Daemon shutting down");
                            let _ = write_msg(&mut writer, &resp).await;
                            break;
                        }

                        let resp = handler.handle(&req.cmd, &req.args).await;
                        if let Err(e) = write_msg(&mut writer, &resp).await {
                            eprintln!("[eoka] write error: {}", e);
                        }
                    }
                    Ok(Err(e)) => eprintln!("[eoka] accept error: {}", e),
                    Err(_) => {
                        // Accept timeout — check idle
                        if last_activity.elapsed() > timeout {
                            eprintln!("[eoka] idle timeout ({}s), shutting down", timeout.as_secs());
                            let _ = handler.handle("close", &serde_json::Value::Null).await;
                            break;
                        }
                    }
                }
            }
            _ = shutdown_rx.recv() => {
                let _ = handler.handle("close", &serde_json::Value::Null).await;
                break;
            }
        }
    }

    let _ = std::fs::remove_file(&sock_path);
    let _ = std::fs::remove_file(&pid_path);
    eprintln!("[eoka] daemon stopped");
    Ok(())
}
