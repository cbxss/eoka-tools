//! Black-box end-to-end test against the real `eoka` binary.
//!
//! This has to shell out to the compiled binary (via `CARGO_BIN_EXE_eoka`)
//! rather than call `client`/`daemon` internals directly: the daemon
//! auto-spawn path re-execs `std::env::current_exe()`, which inside a
//! `cargo test` unit test resolves to the *test harness* binary, not `eoka`,
//! so the spawned "daemon" process is nonsense. Running through the actual
//! binary is what makes this path testable at all.

use std::process::Command;

fn eoka_bin() -> &'static str {
    env!("CARGO_BIN_EXE_eoka")
}

fn run(session: &str, args: &[&str]) -> std::process::Output {
    Command::new(eoka_bin())
        .arg("--session")
        .arg(session)
        .args(args)
        .output()
        .expect("failed to spawn eoka binary")
}

#[test]
#[ignore = "requires Chrome"]
fn spawns_daemon_and_completes_a_real_command() {
    let session = format!("eoka-e2e-test-{}", std::process::id());

    // Best-effort clean slate in case a previous run left a daemon behind.
    let _ = run(&session, &["kill"]);

    let open = run(&session, &["--json", "open", "about:blank"]);
    assert!(
        open.status.success(),
        "open failed: stdout={} stderr={}",
        String::from_utf8_lossy(&open.stdout),
        String::from_utf8_lossy(&open.stderr)
    );
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
