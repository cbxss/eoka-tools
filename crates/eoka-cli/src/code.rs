use serde_json::Value;
use std::path::Path;
use tack_core::{ExecutionEngine, RuntimeLimits};
use tack_runtime_quickjs::QuickJsRuntime;
use tokio::io::AsyncReadExt;

use eoka_sdk::EokaClient;
use eoka_tack::{EokaToolFilter, EokaToolSet};

use crate::launch_spec::LaunchSpec;
use crate::protocol::Response;

pub async fn run_code_command(
    session_name: &str,
    spec: LaunchSpec,
    inline_code: Option<&str>,
    file: Option<&Path>,
    timeout_ms: Option<u64>,
    raw_json: bool,
    filter: EokaToolFilter,
) -> Result<Response, String> {
    let source = resolve_source(inline_code, file).await?;
    let mut limits = RuntimeLimits::default();
    if let Some(timeout_ms) = timeout_ms {
        limits.timeout_ms = timeout_ms;
    }
    let client = EokaClient::new(session_name, spec);
    let catalog = EokaToolSet::new(client)
        .with_filter(filter)
        .catalog("eoka")
        .map_err(|error| error.to_string())?;
    let engine = ExecutionEngine::from_catalog(catalog, QuickJsRuntime).with_limits(limits);
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
