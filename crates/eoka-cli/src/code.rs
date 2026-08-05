use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Arc;
use tack_core::{
    ExecutionEngine, RuntimeLimits, ToolCall, ToolCallError, ToolCallOutput, ToolDescriptor,
    ToolInvoker, ToolRegistry,
};
use tack_runtime_quickjs::QuickJsRuntime;
use tokio::io::AsyncReadExt;

use crate::client;
use crate::launch_spec::LaunchSpec;
use crate::protocol::{Request, Response};

const BLOCKED_TOOLS: &[&str] = &[
    "close", "shutdown", "status", "sessions", "kill", "cdp_url", "code",
];

const TOOL_ALIASES: &[(&str, &str)] = &[
    ("double_click", "dblclick"),
    ("tab.list", "tab_list"),
    ("tab.new", "tab_new"),
    ("tab.switch", "tab_switch"),
    ("tab.close", "tab_close"),
    ("tab.attach", "tab_attach"),
    ("spa.info", "spa_info"),
    ("spa.navigate", "spa_navigate"),
    ("wasm.info", "wasm_info"),
    ("wasm.read", "wasm_read"),
    ("wasm.write", "wasm_write"),
    ("wasm.find", "wasm_find"),
    ("js.mode", "js_mode"),
    ("js.allow", "js_allow"),
    ("js.block", "js_block"),
    ("js.remove", "js_remove"),
    ("js.list", "js_list"),
];

const TOOL_DESCRIPTORS: &[(&str, &str)] = &[
    ("eoka.open", "Navigate to URL"),
    ("eoka.back", "Go back"),
    ("eoka.forward", "Go forward"),
    ("eoka.reload", "Reload page"),
    ("eoka.snapshot", "Accessibility snapshot"),
    ("eoka.observe", "Observe interactive elements"),
    ("eoka.screenshot", "Take screenshot"),
    ("eoka.emulate", "Emulate viewport"),
    ("eoka.info", "Page URL and title"),
    ("eoka.text", "Visible page text"),
    ("eoka.find", "Find elements by text"),
    ("eoka.click", "Click target"),
    ("eoka.double_click", "Double click target"),
    ("eoka.fill", "Fill input"),
    ("eoka.select", "Select option"),
    ("eoka.hover", "Hover target"),
    ("eoka.key", "Press key"),
    ("eoka.scroll", "Scroll page or target"),
    ("eoka.eval", "Evaluate JavaScript"),
    ("eoka.exec", "Execute JavaScript"),
    ("eoka.fetch", "Fetch URL in page context"),
    ("eoka.cookies", "List cookies"),
    ("eoka.set_cookie", "Set cookie"),
    ("eoka.delete_cookie", "Delete cookie"),
    ("eoka.clear_cookies", "Clear cookies"),
    ("eoka.storage", "Read storage"),
    ("eoka.set_storage", "Set storage"),
    ("eoka.dump_storage", "Dump storage"),
    ("eoka.save_state", "Save browser state"),
    ("eoka.load_state", "Load browser state"),
    ("eoka.headers", "Set extra headers"),
    ("eoka.console", "Read console output"),
    ("eoka.errors", "Read JavaScript errors"),
    ("eoka.tab.list", "List tabs"),
    ("eoka.tab.new", "Open new tab"),
    ("eoka.tab.switch", "Switch tab"),
    ("eoka.tab.close", "Close tab"),
    ("eoka.tab.attach", "Attach tab"),
    ("eoka.clone_from", "Clone browser state"),
    ("eoka.wait", "Wait for page condition"),
    ("eoka.spa.info", "SPA routing info"),
    ("eoka.spa.navigate", "SPA navigation"),
    ("eoka.fake_camera", "Inject fake camera"),
    ("eoka.wasm.info", "WASM memory info"),
    ("eoka.wasm.read", "Read WASM memory"),
    ("eoka.wasm.write", "Write WASM memory"),
    ("eoka.wasm.find", "Find WASM memory pattern"),
    ("eoka.js.mode", "Set JavaScript policy mode"),
    ("eoka.js.allow", "Allow JavaScript domain"),
    ("eoka.js.block", "Block JavaScript domain"),
    ("eoka.js.remove", "Remove JavaScript domain rule"),
    ("eoka.js.list", "List JavaScript policy"),
];

pub async fn run_code_command(
    session_name: &str,
    spec: LaunchSpec,
    inline_code: Option<&str>,
    file: Option<&Path>,
    timeout_ms: Option<u64>,
    raw_json: bool,
) -> Result<Response, String> {
    let source = resolve_source(inline_code, file).await?;
    let mut limits = RuntimeLimits::default();
    if let Some(timeout_ms) = timeout_ms {
        limits.timeout_ms = timeout_ms;
    }
    let registry = ToolRegistry::from_tools(eoka_descriptors()).map_err(|e| e.to_string())?;
    let invoker = Arc::new(EokaInvoker {
        session_name: session_name.to_string(),
        spec,
    });
    let engine = ExecutionEngine::new(registry, QuickJsRuntime, invoker)
        .with_limits(limits)
        .with_alias("eoka", "eoka");
    let result = engine.execute(source).await;
    if raw_json {
        return serde_json::to_value(result)
            .map(Response::ok)
            .map_err(|e| e.to_string());
    }
    if result.ok {
        Ok(Response::ok(result.result.unwrap_or(Value::Null)))
    } else {
        Err(result
            .error
            .map(|error| error.message)
            .unwrap_or_else(|| "Code execution failed".to_string()))
    }
}

struct EokaInvoker {
    session_name: String,
    spec: LaunchSpec,
}

#[async_trait]
impl ToolInvoker for EokaInvoker {
    async fn call(&self, call: ToolCall) -> ToolCallOutput {
        let request = match request_from_tool_call(&call.path, call.input) {
            Ok(request) => request,
            Err(error) => return failed_tool(error),
        };
        match client::send_command(&self.session_name, request, self.spec.clone()).await {
            Ok(response) if response.ok => ToolCallOutput {
                ok: true,
                data: Some(response.data.unwrap_or(Value::Null)),
                text: None,
                raw: None,
                error: None,
            },
            Ok(response) => failed_tool(
                response
                    .error
                    .unwrap_or_else(|| "Command failed".to_string()),
            ),
            Err(error) => failed_tool(error.to_string()),
        }
    }
}

fn failed_tool(message: String) -> ToolCallOutput {
    ToolCallOutput {
        ok: false,
        data: None,
        text: None,
        raw: None,
        error: Some(ToolCallError {
            message,
            code: None,
            details: None,
        }),
    }
}

async fn resolve_source(inline_code: Option<&str>, file: Option<&Path>) -> Result<String, String> {
    match (inline_code, file) {
        (Some(_), Some(_)) => {
            Err("Pass code inline, with --file, or through stdin, not more than one".to_string())
        }
        (Some(code), None) => Ok(code.to_string()),
        (None, Some(path)) => std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read code file '{}': {}", path.display(), e)),
        (None, None) => {
            let mut source = String::new();
            tokio::io::stdin()
                .read_to_string(&mut source)
                .await
                .map_err(|e| e.to_string())?;
            if source.trim().is_empty() {
                Err(
                    "No code provided. Pass inline code, --file, or pipe code through stdin"
                        .to_string(),
                )
            } else {
                Ok(source)
            }
        }
    }
}

fn request_from_tool_call(path: &str, input: Value) -> Result<Request, String> {
    let cmd = path
        .strip_prefix("eoka.")
        .ok_or_else(|| format!("Unknown tool: {path}"))?;
    if BLOCKED_TOOLS.contains(&cmd) {
        return Err(format!("Tool is not available through eoka tack: {path}"));
    }
    let cmd = TOOL_ALIASES
        .iter()
        .find_map(|(tool_path, command)| (*tool_path == cmd).then_some(*command))
        .unwrap_or(cmd);
    if let Some(request) = zero_arg_request(cmd) {
        return Ok(request);
    }
    serde_json::from_value(json!({ "cmd": cmd, "args": input })).map_err(|e| e.to_string())
}

fn zero_arg_request(cmd: &str) -> Option<Request> {
    match cmd {
        "back" => Some(Request::Back),
        "forward" => Some(Request::Forward),
        "reload" => Some(Request::Reload),
        "info" => Some(Request::Info),
        "text" => Some(Request::Text),
        "cookies" => Some(Request::Cookies),
        "clear_cookies" => Some(Request::ClearCookies),
        "dump_storage" => Some(Request::DumpStorage),
        "tab_list" => Some(Request::TabList),
        "spa_info" => Some(Request::SpaInfo),
        "wasm_info" => Some(Request::WasmInfo),
        "js_list" => Some(Request::JsList),
        _ => None,
    }
}

fn eoka_descriptors() -> Vec<ToolDescriptor> {
    TOOL_DESCRIPTORS
        .iter()
        .map(|(path, description)| {
            ToolDescriptor::new(*path, path.trim_start_matches("eoka."))
                .with_description(*description)
                .with_input_schema(json!({"type":"object","additionalProperties":true}))
                .with_tag("eoka")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_nested_aliases_to_requests() {
        let request =
            request_from_tool_call("eoka.tab.new", json!({"url":"https://example.com"})).unwrap();
        assert_eq!(request.cmd(), "tab_new");
    }

    #[test]
    fn maps_zero_arg_tools_to_requests() {
        let request = request_from_tool_call("eoka.info", json!({})).unwrap();
        assert_eq!(request.cmd(), "info");
    }

    #[test]
    fn rejects_lifecycle_tools() {
        let error = request_from_tool_call("eoka.close", json!({})).unwrap_err();
        assert!(error.contains("not available"));
    }
}
