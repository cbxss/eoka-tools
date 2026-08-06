use std::io::Read;

use serde_json::{json, Value};

use crate::client;
use crate::launch_spec::LaunchSpec;
use crate::protocol::{Request, Response};

pub(crate) async fn run_batch(
    session_name: &str,
    spec: LaunchSpec,
    input: Option<&str>,
    file: Option<&std::path::PathBuf>,
    bail: bool,
) -> Result<Response, String> {
    let source = read_batch_source(input, file)?;
    let requests = parse_batch_requests(&source)?;
    let mut responses = Vec::with_capacity(requests.len());
    let mut first_error = None;
    let mut shutdown_daemon = false;

    for request in requests {
        let request_cmd = request.cmd().to_string();
        let response = client::send_command(session_name, request, spec.clone())
            .await
            .map_err(|e| e.to_string())?;
        if !response.ok && first_error.is_none() {
            first_error = response.error.clone();
        }
        let step = batch_step_effect(&request_cmd, &response, bail);
        shutdown_daemon |= step.shutdown_daemon;
        responses.push(response);
        if step.stop {
            break;
        }
    }

    if shutdown_daemon {
        let _ = client::kill_daemon(session_name);
    }

    let all_ok = first_error.is_none();
    let data = serde_json::to_value(responses).map_err(|e| e.to_string())?;
    Ok(Response {
        ok: all_ok,
        data: Some(data),
        error: first_error.clone(),
        error_detail: first_error
            .map(|message| eoka_protocol::ErrorDetail::new("batch_error", message)),
        meta: None,
    })
}

fn read_batch_source(
    input: Option<&str>,
    file: Option<&std::path::PathBuf>,
) -> Result<String, String> {
    if let Some(path) = file {
        return std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read batch file '{}': {}", path.display(), e));
    }
    if let Some(input) = input {
        return Ok(input.to_string());
    }

    let mut source = String::new();
    std::io::stdin()
        .read_to_string(&mut source)
        .map_err(|e| format!("Failed to read batch JSON from stdin: {}", e))?;
    if source.trim().is_empty() {
        return Err("Batch JSON is empty. Pass JSON as an argument, via --file, or stdin.".into());
    }
    Ok(source)
}

fn parse_batch_requests(source: &str) -> Result<Vec<Request>, String> {
    let value: Value =
        serde_json::from_str(source).map_err(|e| format!("Invalid batch JSON: {}", e))?;
    let steps = match value {
        Value::Array(steps) => steps,
        other => vec![other],
    };

    steps
        .into_iter()
        .enumerate()
        .map(|(idx, step)| batch_step_to_request(idx, step))
        .collect()
}

fn batch_step_to_request(idx: usize, step: Value) -> Result<Request, String> {
    let mut obj = match step {
        Value::Object(obj) => obj,
        _ => return Err(format!("Batch step {} must be an object", idx + 1)),
    };

    if let Some(cmd) = obj.remove("cmd") {
        let cmd = cmd
            .as_str()
            .ok_or_else(|| format!("Batch step {} field 'cmd' must be a string", idx + 1))?;
        let args = obj.remove("args").unwrap_or_else(|| json!({}));
        return batch_request(idx, cmd, normalize_batch_args(idx, cmd, args)?);
    }

    if obj.len() != 1 {
        return Err(format!(
            "Batch step {} must contain either {{\"cmd\":...}} or exactly one shorthand command",
            idx + 1
        ));
    }

    let (cmd, value) = obj.into_iter().next().expect("len checked");
    let args = normalize_batch_args(idx, &cmd, value)?;
    batch_request(idx, &cmd, args)
}

fn batch_request(idx: usize, cmd: &str, args: Value) -> Result<Request, String> {
    let value = if unit_batch_command(cmd) && args.as_object().is_some_and(|obj| obj.is_empty()) {
        json!({ "cmd": cmd })
    } else {
        json!({ "cmd": cmd, "args": args })
    };
    serde_json::from_value(value)
        .map_err(|e| format!("Batch step {} invalid command '{}': {}", idx + 1, cmd, e))
}

fn unit_batch_command(cmd: &str) -> bool {
    matches!(
        cmd,
        "back"
            | "forward"
            | "reload"
            | "info"
            | "text"
            | "cookies"
            | "clear_cookies"
            | "dump_storage"
            | "tab_list"
            | "spa_info"
            | "wasm_info"
            | "intercept_list"
            | "js_list"
            | "network_record_stop"
            | "network_record_status"
            | "network_clear"
            | "close"
    )
}

fn normalize_batch_args(idx: usize, cmd: &str, value: Value) -> Result<Value, String> {
    if value.is_null() {
        return Ok(json!({}));
    }
    if value.is_object() {
        return Ok(value);
    }

    match cmd {
        "open" | "fetch" => string_arg(idx, cmd, value, "url"),
        "eval" | "exec" => string_arg(idx, cmd, value, "code"),
        "click" | "dblclick" | "hover" | "scroll" => string_arg(idx, cmd, value, "target"),
        "key" => string_arg(idx, cmd, value, "key"),
        "find" => string_arg(idx, cmd, value, "text"),
        "captcha_inject" => string_arg(idx, cmd, value, "token"),
        "spa_navigate" => string_arg(idx, cmd, value, "path"),
        "tab_new" => string_arg(idx, cmd, value, "url"),
        "tab_switch" | "tab_close" | "tab_attach" => string_arg(idx, cmd, value, "tab_id"),
        "wait" => wait_arg(idx, value),
        "fill" => pair_arg(idx, cmd, value, "target", "text"),
        "select" => pair_arg(idx, cmd, value, "target", "value"),
        _ => Err(format!(
            "Batch step {} command '{}' requires object args",
            idx + 1,
            cmd
        )),
    }
}

fn string_arg(idx: usize, cmd: &str, value: Value, key: &str) -> Result<Value, String> {
    value.as_str().map(|v| json!({ key: v })).ok_or_else(|| {
        format!(
            "Batch step {} command '{}' requires string or object args",
            idx + 1,
            cmd
        )
    })
}

fn wait_arg(idx: usize, value: Value) -> Result<Value, String> {
    if let Some(ms) = value.as_u64() {
        return Ok(json!({ "ms": ms }));
    }
    if let Some(text) = value.as_str() {
        return Ok(json!({ "text": text }));
    }
    Err(format!(
        "Batch step {} command 'wait' requires milliseconds, text, or object args",
        idx + 1
    ))
}

fn pair_arg(
    idx: usize,
    cmd: &str,
    value: Value,
    first_key: &str,
    second_key: &str,
) -> Result<Value, String> {
    let values = value.as_array().ok_or_else(|| {
        format!(
            "Batch step {} command '{}' requires [\"{}\", \"{}\"] or object args",
            idx + 1,
            cmd,
            first_key,
            second_key
        )
    })?;
    if values.len() != 2 {
        return Err(format!(
            "Batch step {} command '{}' requires exactly two array values",
            idx + 1,
            cmd
        ));
    }
    let first = values[0].as_str().ok_or_else(|| {
        format!(
            "Batch step {} command '{}' first array value must be a string",
            idx + 1,
            cmd
        )
    })?;
    let second = values[1].as_str().ok_or_else(|| {
        format!(
            "Batch step {} command '{}' second array value must be a string",
            idx + 1,
            cmd
        )
    })?;
    Ok(json!({ first_key: first, second_key: second }))
}

struct BatchStepEffect {
    stop: bool,
    shutdown_daemon: bool,
}

fn batch_step_effect(cmd: &str, response: &Response, bail: bool) -> BatchStepEffect {
    let shutdown_daemon = cmd == "close" && response.ok;
    BatchStepEffect {
        stop: shutdown_daemon || (bail && !response.ok),
        shutdown_daemon,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::test_support::parsed_command;
    use crate::cli::Command;

    #[test]
    fn batch_cli_accepts_inline_json_argument() {
        let (_cli, command) = parsed_command(&["eoka", "batch", r#"[{"info":null}]"#]);

        match command {
            Command::Batch { input, file, bail } => {
                assert_eq!(input.as_deref(), Some(r#"[{"info":null}]"#));
                assert!(file.is_none());
                assert!(!bail);
            }
            _ => panic!("expected batch command"),
        }
    }

    #[test]
    fn batch_shorthand_matches_recreation_flow_shape() {
        let requests = parse_batch_requests(
            r#"[
                {"open":"https://www.recreation.gov/camping/campsites/71576?start_date=2026-08-02&end_date=2026-08-03"},
                {"wait":3000},
                {"eval":"\"patched\""},
                {"click":"Add to Cart"},
                {"wait":5000},
                {"info":null}
            ]"#,
        )
        .unwrap();

        assert_eq!(requests.len(), 6);
        assert_eq!(requests[0].cmd(), "open");
        assert_eq!(
            requests[0].args_json()["url"],
            "https://www.recreation.gov/camping/campsites/71576?start_date=2026-08-02&end_date=2026-08-03"
        );
        assert_eq!(requests[1].cmd(), "wait");
        assert_eq!(requests[1].args_json()["ms"], 3000);
        assert_eq!(requests[2].cmd(), "eval");
        assert_eq!(requests[2].args_json()["code"], "\"patched\"");
        assert_eq!(requests[3].cmd(), "click");
        assert_eq!(requests[3].args_json()["target"], "Add to Cart");
        assert_eq!(requests[5].cmd(), "info");
        assert_eq!(requests[5].args_json(), json!({}));
    }

    #[test]
    fn batch_accepts_canonical_cmd_args_steps() {
        let requests =
            parse_batch_requests(r#"[{"cmd":"eval","args":{"code":"window.location.href"}}]"#)
                .unwrap();

        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].cmd(), "eval");
        assert_eq!(requests[0].args_json()["code"], "window.location.href");
    }

    #[test]
    fn batch_canonical_cmd_args_scalar_uses_protocol_command() {
        let requests = parse_batch_requests(r#"[{"cmd":"spa_navigate","args":"/cart"}]"#).unwrap();

        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].cmd(), "spa_navigate");
        assert_eq!(requests[0].args_json()["path"], "/cart");
    }

    #[test]
    fn batch_accepts_captcha_inject_shorthand() {
        let requests = parse_batch_requests(r#"[{"captcha_inject":"token-123"}]"#).unwrap();

        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].cmd(), "captcha_inject");
        assert_eq!(requests[0].args_json()["token"], "token-123");
    }

    #[test]
    fn batch_close_stops_and_marks_daemon_for_shutdown() {
        let effect = batch_step_effect("close", &Response::ok_text("Browser closed"), false);

        assert!(effect.stop);
        assert!(effect.shutdown_daemon);
    }

    #[test]
    fn batch_bail_still_stops_on_error_without_daemon_shutdown() {
        let effect = batch_step_effect("click", &Response::err("not found"), true);

        assert!(effect.stop);
        assert!(!effect.shutdown_daemon);
    }

    #[test]
    fn batch_rejects_ambiguous_shorthand_step() {
        let err = parse_batch_requests(r#"[{"click":"A","eval":"B"}]"#).unwrap_err();

        assert!(err.contains("exactly one shorthand command"));
    }

    #[test]
    fn batch_rejects_scalar_step() {
        let err = parse_batch_requests(r#"["click Add"]"#).unwrap_err();

        assert!(err.contains("must be an object"));
    }
}
