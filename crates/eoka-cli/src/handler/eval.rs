//! `eval`/`exec` commands and Runtime.evaluate result formatting.

use eoka::cdp::Session as CdpSession;
use serde_json::{json, Value};

use super::target::json_str;
use super::Handler;
use crate::protocol::Response;

impl Handler {
    pub(super) async fn cmd_eval(&mut self, args: &Value) -> Result<Response, String> {
        let code = resolve_js(args)?;
        let max_size = args["max_size"].as_u64().map(|v| v as usize);
        let await_promise = !args["no_await"].as_bool().unwrap_or(false);
        let session = self.require_tab()?.page.session().clone();

        if let Some(max) = max_size {
            let result: Value = session
                .send(
                    "Runtime.evaluate",
                    &json!({
                        "expression": build_limited_eval_js(&code, max),
                        "returnByValue": true,
                        "awaitPromise": await_promise,
                    }),
                )
                .await
                .map_err(|e| e.to_string())?;
            return Ok(Response::ok_text(runtime_eval_string_value(&result)?));
        }

        let result: Value = session
            .send(
                "Runtime.evaluate",
                &json!({
                    "expression": code,
                    "returnByValue": false,
                    "awaitPromise": await_promise,
                }),
            )
            .await
            .map_err(|e| e.to_string())?;
        Ok(Response::ok_text(
            format_runtime_eval_result(&session, &result).await?,
        ))
    }

    pub(super) async fn cmd_exec(&mut self, args: &Value) -> Result<Response, String> {
        let code = resolve_js(args)?;
        let tab = self.require_tab()?;
        if args["no_await"].as_bool().unwrap_or(false) {
            let _: String = tab.page.evaluate_sync(&code).await.unwrap_or_default();
        } else {
            let _ = tab.page.execute(&code).await;
        }
        Ok(Response::ok_text("Executed successfully"))
    }
}

async fn format_runtime_eval_result(
    session: &CdpSession,
    result: &Value,
) -> Result<String, String> {
    if let Some(exception) = result.get("exceptionDetails").filter(|v| !v.is_null()) {
        return Err(format_runtime_exception(exception));
    }

    let remote = result
        .get("result")
        .ok_or_else(|| "Runtime.evaluate response missing result".to_string())?;
    format_runtime_remote_object(session, remote).await
}

fn runtime_eval_string_value(result: &Value) -> Result<String, String> {
    if let Some(exception) = result.get("exceptionDetails").filter(|v| !v.is_null()) {
        return Err(format_runtime_exception(exception));
    }

    let remote = result
        .get("result")
        .ok_or_else(|| "Runtime.evaluate response missing result".to_string())?;
    if remote.get("type").and_then(Value::as_str) == Some("undefined") {
        return Ok("undefined".into());
    }
    remote
        .get("value")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "Runtime.evaluate did not return a string".to_string())
}

fn format_runtime_exception(exception: &Value) -> String {
    let text = exception
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or("JavaScript error");
    let line = exception
        .get("lineNumber")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let column = exception
        .get("columnNumber")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    format!("JavaScript error: {} at {}:{}", text, line, column)
}

async fn format_runtime_remote_object(
    session: &CdpSession,
    remote: &Value,
) -> Result<String, String> {
    if let Some(text) = runtime_primitive_object_text(remote) {
        return Ok(text);
    }

    if let Some(object_id) = remote.get("objectId").and_then(Value::as_str) {
        let text = stringify_remote_object(session, object_id)
            .await?
            .unwrap_or_else(|| runtime_remote_description(remote));
        let _ = session
            .send::<_, Value>("Runtime.releaseObject", &json!({ "objectId": object_id }))
            .await;
        return Ok(text);
    }

    Ok(runtime_remote_description(remote))
}

fn runtime_primitive_object_text(remote: &Value) -> Option<String> {
    let remote_type = remote.get("type").and_then(Value::as_str).unwrap_or("");
    if remote_type == "undefined" || remote_type == "function" || remote_type == "symbol" {
        return Some("undefined".into());
    }

    if let Some(value) = remote.get("value") {
        return Some(serde_json::to_string(value).unwrap_or_else(|_| value.to_string()));
    }

    if let Some(value) = remote.get("unserializableValue").and_then(Value::as_str) {
        return Some(unserializable_runtime_value_text(value));
    }

    None
}

fn runtime_remote_description(remote: &Value) -> String {
    let remote_type = remote.get("type").and_then(Value::as_str).unwrap_or("");
    remote
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or(remote_type)
        .to_string()
}

async fn stringify_remote_object(
    session: &CdpSession,
    object_id: &str,
) -> Result<Option<String>, String> {
    let result: Value = session
        .send(
            "Runtime.callFunctionOn",
            &json!({
                "objectId": object_id,
                "functionDeclaration": RUNTIME_OBJECT_TO_TEXT_JS,
                "returnByValue": true,
                "awaitPromise": true,
            }),
        )
        .await
        .map_err(|e| e.to_string())?;

    if result.get("exceptionDetails").is_some_and(|v| !v.is_null()) {
        return Ok(None);
    }
    Ok(runtime_eval_string_value(&result).ok())
}

const RUNTIME_OBJECT_TO_TEXT_JS: &str = r#"function() {
  let text;
  try {
    text = JSON.stringify(this);
  } catch (error) {
    text = String(this);
  }
  if (text === undefined) {
    text = String(this);
  }
  if (text === undefined) {
    text = "undefined";
  }
  return text;
}"#;

fn unserializable_runtime_value_text(value: &str) -> String {
    match value {
        "NaN" | "Infinity" | "-Infinity" => "null".into(),
        "-0" => "0".into(),
        bigint if bigint.ends_with('n') => bigint.trim_end_matches('n').into(),
        other => other.into(),
    }
}

fn build_limited_eval_js(code: &str, max_size: usize) -> String {
    format!(
        r#"(async () => {{
  const source = {source};
  const value = await Function("return eval(arguments[0])")(source);
  let text;
  try {{
    text = JSON.stringify(value);
  }} catch (error) {{
    text = String(value);
  }}
  if (text === undefined) {{
    text = "undefined";
  }}
  if (text.length <= {max}) {{
    return text;
  }}
  return text.slice(0, {max}) + "...(truncated " + text.length + " chars)";
}})()"#,
        source = json_str(code),
        max = max_size,
    )
}

fn resolve_js(args: &Value) -> Result<String, String> {
    if let Some(path) = args["file"].as_str() {
        std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read JS file '{}': {}", path, e))
    } else if let Some(code) = args["code"].as_str() {
        Ok(code.to_string())
    } else {
        Err("Either 'code' or '--file' must be provided".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_eval_formats_undefined() {
        let text = runtime_primitive_object_text(&json!({ "type": "undefined" })).unwrap();

        assert_eq!(text, "undefined");
    }

    #[test]
    fn runtime_eval_formats_string_as_json_string() {
        let text = runtime_primitive_object_text(&json!({ "type": "string", "value": "patched" }))
            .unwrap();

        assert_eq!(text, "\"patched\"");
    }

    #[test]
    fn runtime_eval_formats_remote_object_descriptions() {
        let text = runtime_remote_description(&json!({
            "type": "object",
            "description": "Object"
        }));

        assert_eq!(text, "Object");
    }

    #[test]
    fn runtime_eval_formats_exception_details() {
        let err = format_runtime_exception(&json!({
            "text": "Uncaught",
            "lineNumber": 1,
            "columnNumber": 2
        }));

        assert_eq!(err, "JavaScript error: Uncaught at 1:2");
    }

    #[test]
    fn runtime_eval_result_detects_exception_details() {
        let result = json!({
            "exceptionDetails": {
                "text": "Uncaught",
                "lineNumber": 1,
                "columnNumber": 2
            },
            "result": { "type": "object" }
        });
        let err = result
            .get("exceptionDetails")
            .filter(|v| !v.is_null())
            .map(format_runtime_exception)
            .unwrap();

        assert_eq!(err, "JavaScript error: Uncaught at 1:2");
    }

    #[test]
    fn runtime_eval_formats_unserializable_values_like_json_stringify() {
        assert_eq!(
            runtime_primitive_object_text(
                &json!({ "type": "number", "unserializableValue": "NaN" })
            ),
            Some("null".to_string())
        );
        assert_eq!(
            runtime_primitive_object_text(
                &json!({ "type": "bigint", "unserializableValue": "123n" })
            ),
            Some("123".to_string())
        );
    }

    #[test]
    fn limited_eval_js_truncates_in_browser() {
        let js = build_limited_eval_js("document.body.innerText", 128);

        assert!(js.contains("Function(\"return eval(arguments[0])\")"));
        assert!(js.contains("text.slice(0, 128)"));
        assert!(js.contains("...(truncated "));
    }

    #[test]
    fn runtime_eval_string_value_extracts_limited_helper_result() {
        let text = runtime_eval_string_value(
            &json!({ "result": { "type": "string", "value": "abc...(truncated 20 chars)" } }),
        )
        .unwrap();

        assert_eq!(text, "abc...(truncated 20 chars)");
    }
}
