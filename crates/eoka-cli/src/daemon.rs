use std::time::{Duration, Instant};
use tokio::net::UnixListener;
use tokio::sync::mpsc;

use crate::handler::Handler;
use crate::launch_spec::LaunchSpec;
use crate::protocol::{read_msg, write_msg, Request, Response};
use crate::session;

fn idle_timeout() -> Duration {
    let ms = std::env::var("EOKA_IDLE_TIMEOUT")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(30 * 60 * 1000);
    Duration::from_millis(ms)
}

pub async fn run(session_name: &str, spec: LaunchSpec) -> anyhow::Result<()> {
    session::ensure_runtime_dir()?;

    let sock_path = session::socket_path(session_name);
    let pid_path = session::pid_path(session_name);

    if sock_path.exists() {
        let _ = std::fs::remove_file(&sock_path);
    }

    let listener = UnixListener::bind(&sock_path)?;
    std::fs::write(&pid_path, std::process::id().to_string())?;

    eprintln!(
        "[eoka] daemon started (session={}, pid={}, mode={})",
        session_name,
        std::process::id(),
        if spec.is_live() { "connect" } else { "launch" }
    );
    eprintln!("[eoka] socket: {}", sock_path.display());

    let mut handler = Handler::new(session_name, spec);
    let mut last_activity = Instant::now();
    let timeout = idle_timeout();

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

                        let raw: serde_json::Value = match read_msg(&mut reader).await {
                            Ok(r) => r,
                            Err(e) => {
                                eprintln!("[eoka] read error: {}", e);
                                continue;
                            }
                        };
                        let req: Request = match serde_json::from_value(raw) {
                            Ok(req) => req,
                            Err(e) => {
                                let resp = Response::err(format!("Invalid command request: {}", e));
                                let _ = write_msg(&mut writer, &resp).await;
                                continue;
                            }
                        };

                        if matches!(req, Request::Shutdown) {
                            let _ = handler.handle("close", &serde_json::Value::Null).await;
                            let resp = Response::ok_text("Daemon shutting down");
                            let _ = write_msg(&mut writer, &resp).await;
                            break;
                        }

                        let resp = handler.handle_request(req).await;
                        if let Err(e) = write_msg(&mut writer, &resp).await {
                            eprintln!("[eoka] write error: {}", e);
                        }
                    }
                    Ok(Err(e)) => eprintln!("[eoka] accept error: {}", e),
                    Err(_) => {
                        if last_activity.elapsed() > timeout && !handler.is_network_recording().await {
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

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn idle_timeout_shuts_daemon_down_with_no_clients() {
        unsafe {
            std::env::set_var("EOKA_IDLE_TIMEOUT", "50");
        }
        let session_name = format!("eoka-idle-test-{}", std::process::id());
        let spec = LaunchSpec::Launch {
            headless: true,
            from_profile: None,
            clone_state_from: None,
            no_stealth: false,
            proxy: None,
            no_js: false,
            js_allow: Vec::new(),
            js_block: Vec::new(),
            persist: false,
            geo_align: false,
        };

        let result = tokio::time::timeout(Duration::from_secs(15), run(&session_name, spec)).await;

        unsafe {
            std::env::remove_var("EOKA_IDLE_TIMEOUT");
        }

        assert!(
            result.is_ok(),
            "daemon did not shut itself down within 15s of being idle"
        );
        assert!(result.unwrap().is_ok(), "daemon run() returned an error");
        assert!(!session::socket_path(&session_name).exists());
        assert!(!session::pid_path(&session_name).exists());
    }
}
