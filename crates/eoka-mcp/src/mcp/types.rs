use serde::Deserialize;
use std::collections::HashMap;

pub type TargetRequest = eoka_protocol::TargetArgs;
pub type FillRequest = eoka_protocol::FillArgs;
pub type SelectRequest = eoka_protocol::SelectArgs;
pub type TypeKeyRequest = eoka_protocol::KeyArgs;
pub type ScrollRequest = eoka_protocol::TargetArgs;
pub type FindTextRequest = eoka_protocol::TextArgs;
pub type NewTabRequest = eoka_protocol::TabNewArgs;
pub type TabIdRequest = eoka_protocol::TabIdArgs;
pub type SpaNavigateRequest = eoka_protocol::PathStringArgs;
pub type ObserveRequest = eoka_protocol::ObserveArgs;
pub type SaveStateRequest = eoka_protocol::PathArgs;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct NavigateRequest {
    #[schemars(description = "URL to navigate to")]
    pub url: String,
    #[schemars(
        description = "Extra HTTP request headers (e.g. {\"x-middleware-subrequest\": \"middleware\"} for CVE-2025-29927)"
    )]
    pub headers: Option<HashMap<String, String>>,
    #[schemars(description = "Override User-Agent string for this navigation")]
    pub user_agent: Option<String>,
    #[schemars(description = "Disable CSP enforcement before navigating (useful for XSS testing)")]
    pub bypass_csp: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct JsRequest {
    #[schemars(description = "JavaScript code to execute (optional if 'file' is provided)")]
    pub js: Option<String>,
    #[schemars(
        description = "Path to a .js file on disk to execute instead of inline code. Saves tokens on repeated/complex scripts."
    )]
    pub file: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FetchRequest {
    #[schemars(description = "URL to fetch")]
    pub url: String,
    #[schemars(description = "HTTP method: GET, POST, PUT, DELETE, PATCH (default: GET)")]
    pub method: Option<String>,
    #[schemars(description = "Request headers as key-value pairs")]
    pub headers: Option<HashMap<String, String>>,
    #[schemars(description = "Request body string (for POST/PUT/PATCH)")]
    pub body: Option<String>,
    #[schemars(
        description = "Redirect handling: 'follow' (default, follow redirects), 'manual' (capture redirect URL without following — response type will be 'opaqueredirect'), 'error' (fail on redirect)"
    )]
    pub redirect: Option<String>,
    #[schemars(
        description = "Max response body bytes to return (default: 8192). Set to 0 for headers-only."
    )]
    pub max_body: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetCookieRequest {
    #[schemars(description = "Cookie name")]
    pub name: String,
    #[schemars(description = "Cookie value")]
    pub value: String,
    #[schemars(
        description = "Cookie domain (e.g. '.example.com'). If omitted, uses current page domain."
    )]
    pub domain: Option<String>,
    #[schemars(description = "Cookie path (default: '/')")]
    pub path: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct HistoryGoRequest {
    #[schemars(description = "History delta: -1 for back, 1 for forward, -2 for back twice, etc.")]
    pub delta: i32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SnapshotRequest {
    #[schemars(
        description = "Include all nodes (generic, presentation, StaticText). Default false for cleaner output."
    )]
    pub include_all: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BatchAction {
    #[schemars(description = "Action type: 'click', 'fill', 'type_key'")]
    pub action: String,
    #[schemars(description = "Target element (for click/fill)")]
    pub target: Option<String>,
    #[schemars(description = "Text value (for fill/type_key)")]
    pub text: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BatchRequest {
    #[schemars(description = "Array of actions to execute in sequence")]
    pub actions: Vec<BatchAction>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SolveCaptchaRequest {
    #[schemars(description = "Anti-captcha.com API key")]
    pub api_key: String,
    #[schemars(
        description = "Captcha type: 'hcaptcha', 'recaptcha_v2', 'recaptcha_v2_enterprise', 'recaptcha_v3', or 'amazon_waf'"
    )]
    pub captcha_type: String,
    #[schemars(description = "Website/page URL")]
    pub website_url: String,
    #[schemars(description = "Site key for the captcha")]
    pub website_key: String,
    #[schemars(description = "Page action (for reCAPTCHA v3)")]
    pub page_action: Option<String>,
    #[schemars(description = "Minimum score (for reCAPTCHA v3, default 0.3)")]
    pub min_score: Option<f32>,
    #[schemars(
        description = "Optional object passed to grecaptcha.enterprise.render for reCAPTCHA v2 Enterprise"
    )]
    pub enterprise_payload: Option<serde_json::Value>,
    #[schemars(
        description = "Optional reCAPTCHA script domain: www.google.com or www.recaptcha.net"
    )]
    pub api_domain: Option<String>,
    #[schemars(description = "AWS WAF iv value from window.gokuProps (required for amazon_waf)")]
    pub iv: Option<String>,
    #[schemars(
        description = "AWS WAF context value from window.gokuProps (required for amazon_waf)"
    )]
    pub context: Option<String>,
    #[schemars(description = "Optional AWS WAF captcha.js URL")]
    pub captcha_script: Option<String>,
    #[schemars(description = "Optional AWS WAF challenge.js URL")]
    pub challenge_script: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DetectCaptchaRequest {
    #[schemars(description = "Auto-detect hCaptcha or reCAPTCHA on current page")]
    pub auto_detect: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WaitMsRequest {
    #[schemars(description = "Milliseconds to wait (max 30000)")]
    pub ms: u64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ClickInterceptNavRequest {
    #[schemars(
        description = "Target element. Supports: index (0), text:Submit, placeholder:Email, role:button, css:form button, id:my-btn, or plain text search"
    )]
    pub target: String,
    #[schemars(
        description = "How long to wait after click for navigation to be intercepted (ms, default 3000)"
    )]
    pub wait_ms: Option<u64>,
    #[schemars(
        description = "If true, allow the navigation to proceed after capturing the URL (default: false — blocks navigation so you can read the page)"
    )]
    pub allow_nav: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DeleteCookieRequest {
    #[schemars(description = "Cookie name to delete")]
    pub name: String,
    #[schemars(
        description = "Cookie domain (e.g. '.example.com'). If omitted, uses current page domain."
    )]
    pub domain: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AcceptDialogRequest {
    #[schemars(
        description = "Text to type for prompt() dialogs (optional, ignored for alert/confirm)"
    )]
    pub prompt_text: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WaitForTextRequest {
    #[schemars(description = "Text substring to wait for (case-insensitive)")]
    pub text: String,
    #[schemars(description = "Timeout in milliseconds (default: 10000)")]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WaitNetworkIdleRequest {
    #[schemars(description = "Milliseconds without requests to consider idle (default: 500)")]
    pub idle_ms: Option<u64>,
    #[schemars(description = "Maximum wait time in milliseconds (default: 10000)")]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct StorageKeyRequest {
    #[schemars(description = "Storage key to read")]
    pub key: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct StorageSetRequest {
    #[schemars(description = "Storage key")]
    pub key: String,
    #[schemars(description = "Value to store")]
    pub value: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ConsoleRequest {
    #[schemars(description = "Clear the console buffer after reading (default: false)")]
    pub clear: Option<bool>,
    #[schemars(
        description = "Filter by level: 'log', 'warn', 'error', 'info', 'debug' (default: all)"
    )]
    pub level: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ErrorsRequest {
    #[schemars(description = "Clear the errors buffer after reading (default: false)")]
    pub clear: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LoadStateRequest {
    #[schemars(description = "File path to load state from (JSON). Absolute path required.")]
    pub path: String,
    #[schemars(
        description = "Navigate to the saved URL after loading state (default: true). Set false to restore state then navigate somewhere else."
    )]
    pub navigate: Option<bool>,
}

#[cfg(test)]
mod tests {
    use eoka_protocol::{input_schema_for_cmd, operation_by_cmd};

    use super::{
        FillRequest, NewTabRequest, ObserveRequest, SaveStateRequest, SelectRequest,
        SpaNavigateRequest, TabIdRequest, TargetRequest, TypeKeyRequest,
    };

    #[test]
    fn shared_mcp_protocol_operations_are_cataloged() {
        let shared_commands = [
            "click",
            "fill",
            "select",
            "key",
            "scroll",
            "find",
            "tab_new",
            "tab_switch",
            "tab_close",
            "spa_navigate",
            "observe",
            "save_state",
        ];

        for command in shared_commands {
            assert!(
                operation_by_cmd(command).is_some(),
                "MCP shared request type is not represented in protocol catalog: {command}"
            );
        }
    }

    #[test]
    fn shared_mcp_protocol_request_schemas_match_catalog() {
        assert_eq!(
            serde_json::to_value(schemars::schema_for!(TargetRequest)).unwrap(),
            input_schema_for_cmd("click")
        );
        assert_eq!(
            serde_json::to_value(schemars::schema_for!(FillRequest)).unwrap(),
            input_schema_for_cmd("fill")
        );
        assert_eq!(
            serde_json::to_value(schemars::schema_for!(SelectRequest)).unwrap(),
            input_schema_for_cmd("select")
        );
        assert_eq!(
            serde_json::to_value(schemars::schema_for!(TypeKeyRequest)).unwrap(),
            input_schema_for_cmd("key")
        );
        assert_eq!(
            serde_json::to_value(schemars::schema_for!(NewTabRequest)).unwrap(),
            input_schema_for_cmd("tab_new")
        );
        assert_eq!(
            serde_json::to_value(schemars::schema_for!(TabIdRequest)).unwrap(),
            input_schema_for_cmd("tab_switch")
        );
        assert_eq!(
            serde_json::to_value(schemars::schema_for!(SpaNavigateRequest)).unwrap(),
            input_schema_for_cmd("spa_navigate")
        );
        assert_eq!(
            serde_json::to_value(schemars::schema_for!(ObserveRequest)).unwrap(),
            input_schema_for_cmd("observe")
        );
        assert_eq!(
            serde_json::to_value(schemars::schema_for!(SaveStateRequest)).unwrap(),
            input_schema_for_cmd("save_state")
        );
    }
}
