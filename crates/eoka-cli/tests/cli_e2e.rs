use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

const AUTH_PROXY: &str = "http://14a48de81a808:9b2c74eae3@217.156.58.217:12323";

fn eoka_bin() -> &'static str {
    env!("CARGO_BIN_EXE_eoka")
}

fn run(session: &str, args: &[&str]) -> std::process::Output {
    command(session, args)
        .output()
        .expect("failed to spawn eoka binary")
}

fn command(session: &str, args: &[&str]) -> Command {
    let mut command = Command::new(eoka_bin());
    command.arg("--session").arg(session).args(args);
    command
}

fn runtime_dir() -> PathBuf {
    let base = std::env::var("XDG_RUNTIME_DIR")
        .or_else(|_| std::env::var("TMPDIR"))
        .unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(base).join("eoka")
}

fn pid_path(session: &str) -> PathBuf {
    runtime_dir().join(format!("eoka-{session}.pid"))
}

fn cmdline(pid: u32) -> Vec<String> {
    let bytes = std::fs::read(format!("/proc/{pid}/cmdline")).expect("process cmdline readable");
    bytes
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
        .map(|part| String::from_utf8_lossy(part).into_owned())
        .collect()
}

fn daemon_pid(session: &str) -> u32 {
    let pid = std::fs::read_to_string(pid_path(session)).expect("daemon pid file readable");
    pid.trim().parse().expect("daemon pid is numeric")
}

fn child_pids(pid: u32) -> Vec<u32> {
    let path = format!("/proc/{pid}/task/{pid}/children");
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .split_whitespace()
        .filter_map(|value| value.parse().ok())
        .collect()
}

fn chrome_cmdline_for_daemon(pid: u32) -> Vec<String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        for child in child_pids(pid) {
            let args = cmdline(child);
            if args.iter().any(|arg| arg.contains("chrome")) {
                return args;
            }
        }
        if Instant::now() > deadline {
            panic!("chrome child process was not found for daemon {pid}");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn assert_authenticated_proxy_is_normalized(chrome_args: &[String]) {
    assert!(
        chrome_args
            .iter()
            .any(|arg| arg.starts_with("--proxy-server=http://127.0.0.1:")),
        "chrome args missing local proxy forwarder: {chrome_args:?}"
    );
    assert!(
        chrome_args.iter().all(|arg| !arg.contains("14a48de81a808")
            && !arg.contains("9b2c74eae3")
            && !arg.contains("217.156.58.217")
            && !arg.contains("http://http")),
        "chrome args leaked upstream proxy details or malformed proxy: {chrome_args:?}"
    );
}

fn clean_session(session: &str) {
    let _ = run(session, &["kill"]);
}

fn temp_proxy_file(session: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("{session}-proxies.txt"));
    std::fs::write(&path, format!("\n# ignored\n{AUTH_PROXY}\n\n"))
        .expect("proxy file should be writable");
    path
}

fn assert_success(output: &std::process::Output, context: &str) {
    assert!(
        output.status.success(),
        "{context} failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "requires Chrome"]
fn spawns_daemon_and_completes_a_real_command() {
    let session = format!("eoka-e2e-test-{}", std::process::id());

    clean_session(&session);

    let open = run(&session, &["--json", "open", "about:blank"]);
    assert_success(&open, "open");
    let open_json: serde_json::Value =
        serde_json::from_slice(&open.stdout).expect("open output should be JSON");
    assert_eq!(open_json["ok"], serde_json::json!(true));

    let status = run(&session, &["--json", "status"]);
    assert!(status.status.success());
    let status_json: serde_json::Value =
        serde_json::from_slice(&status.stdout).expect("status output should be JSON");
    assert_eq!(status_json["data"]["running"], serde_json::json!(true));

    let kill = run(&session, &["--json", "kill"]);
    assert!(kill.status.success());
    let kill_json: serde_json::Value =
        serde_json::from_slice(&kill.stdout).expect("kill output should be JSON");
    assert_eq!(kill_json["data"]["killed"], serde_json::json!(true));

    let status_after = run(&session, &["--json", "status"]);
    let status_after_json: serde_json::Value =
        serde_json::from_slice(&status_after.stdout).expect("status output should be JSON");
    assert_eq!(
        status_after_json["data"]["running"],
        serde_json::json!(false)
    );
}

#[test]
#[ignore = "requires Chrome and /proc"]
fn env_authenticated_proxy_launches_chrome_with_local_proxy_forwarder() {
    let session = format!("eoka-proxy-env-test-{}", std::process::id());

    clean_session(&session);

    let open = command(&session, &["--json", "open", "about:blank"])
        .env("EOKA_PROXY", AUTH_PROXY)
        .output()
        .expect("failed to spawn eoka binary");
    assert_success(&open, "open");

    let daemon_pid = daemon_pid(&session);
    let chrome_args = chrome_cmdline_for_daemon(daemon_pid);
    assert_authenticated_proxy_is_normalized(&chrome_args);

    clean_session(&session);
}

#[test]
#[ignore = "requires Chrome and /proc"]
fn proxy_file_user_flow_forwards_resolved_proxy_to_daemon_and_chrome() {
    let session = format!("eoka-proxy-file-test-{}", std::process::id());
    let proxy_file = temp_proxy_file(&session);

    clean_session(&session);

    let proxy_file_arg = proxy_file.to_string_lossy().into_owned();
    let open = run(
        &session,
        &[
            "--proxy-file",
            &proxy_file_arg,
            "--json",
            "open",
            "about:blank",
        ],
    );
    assert_success(&open, "open");

    let daemon_pid = daemon_pid(&session);
    let daemon_args = cmdline(daemon_pid);
    assert!(
        daemon_args.iter().any(|arg| arg == "--proxy"),
        "daemon args did not receive resolved --proxy: {daemon_args:?}"
    );
    assert!(
        daemon_args.iter().any(|arg| arg == AUTH_PROXY),
        "daemon args did not receive the proxy file entry: {daemon_args:?}"
    );
    assert!(
        daemon_args.iter().all(|arg| arg != "--proxy-file"),
        "daemon should not re-read proxy files: {daemon_args:?}"
    );

    let chrome_args = chrome_cmdline_for_daemon(daemon_pid);
    assert_authenticated_proxy_is_normalized(&chrome_args);

    clean_session(&session);
    let _ = std::fs::remove_file(proxy_file);
}
