use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct OpenArgs {
    pub url: String,
    pub headers: Option<serde_json::Value>,
    pub user_agent: Option<String>,
    #[serde(default)]
    pub bypass_csp: bool,
    pub inject_js: Option<String>,
    pub load_state: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct SnapshotArgs {
    #[serde(default)]
    pub interactive: bool,
    #[serde(default)]
    pub all: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct ObserveArgs {
    #[schemars(
        description = "Filter: 'inputs' for form elements, 'buttons' for buttons and links, or 'all' for all observed elements."
    )]
    pub filter: Option<String>,
    #[schemars(description = "Maximum elements to return.")]
    pub max: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct ScreenshotArgs {
    pub output: Option<String>,
    #[serde(default)]
    pub annotate: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EmulateArgs {
    #[serde(default = "default_viewport_width")]
    pub width: u32,
    #[serde(default = "default_viewport_height")]
    pub height: u32,
    #[serde(default = "default_device_pixel_ratio")]
    pub dpr: f64,
    #[serde(default)]
    pub desktop: bool,
    #[serde(default)]
    pub reset: bool,
}

impl Default for EmulateArgs {
    fn default() -> Self {
        Self {
            width: default_viewport_width(),
            height: default_viewport_height(),
            dpr: default_device_pixel_ratio(),
            desktop: false,
            reset: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct TextArgs {
    #[schemars(description = "Text substring to search for, matched case-insensitively.")]
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct TargetArgs {
    #[schemars(
        description = "Target element. Supports index, snapshot refs like @e1, text:Submit, placeholder:Email, role:button, css:form button, id:my-btn, or plain text search."
    )]
    pub target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct FillArgs {
    #[schemars(
        description = "Target input. Supports index, text:Email, placeholder:Enter code, css:input.search, id:email-field, or plain text search."
    )]
    pub target: String,
    #[schemars(description = "Text to type into the element.")]
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct SelectArgs {
    #[schemars(description = "Target select element by index, text, CSS selector, id, or role.")]
    pub target: String,
    #[schemars(description = "Option value or visible text to select.")]
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct KeyArgs {
    #[schemars(
        description = "Key to press, for example Enter, Tab, Escape, ArrowDown, or Backspace."
    )]
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct ScriptArgs {
    pub code: Option<String>,
    pub file: Option<String>,
    pub max_size: Option<usize>,
    #[serde(default)]
    pub no_await: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct FetchArgs {
    pub url: String,
    pub method: Option<String>,
    pub headers: Option<serde_json::Value>,
    pub body: Option<String>,
    pub redirect: Option<String>,
    #[serde(default)]
    pub body_only: bool,
    pub max_body: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct SetCookieArgs {
    pub name: String,
    pub value: String,
    pub domain: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct DeleteCookieArgs {
    pub name: String,
    pub domain: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct StorageArgs {
    pub key: Option<String>,
    #[serde(default)]
    pub session_storage: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct SetStorageArgs {
    pub key: String,
    pub value: String,
    #[serde(default)]
    pub session_storage: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct PathArgs {
    #[schemars(description = "File path.")]
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct LoadStateArgs {
    pub path: String,
    #[serde(default)]
    pub no_navigate: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct HeadersArgs {
    pub headers_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct ConsoleArgs {
    #[serde(default)]
    pub clear: bool,
    pub level: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct ClearFlagArgs {
    #[serde(default)]
    pub clear: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct TabNewArgs {
    #[schemars(description = "Optional URL to navigate to. If omitted, opens a blank tab.")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct TabIdArgs {
    #[schemars(description = "Tab ID from list_tabs.")]
    pub tab_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct CloneFromArgs {
    pub source: String,
    pub to: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct WaitArgs {
    pub ms: Option<u64>,
    pub text: Option<String>,
    pub url: Option<String>,
    pub load: Option<String>,
    pub timeout: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct PathStringArgs {
    #[schemars(description = "Target path, for example /docs or /about.")]
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct FakeCameraArgs {
    pub file: String,
    #[serde(default)]
    pub loop_video: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct WasmReadArgs {
    pub addr: String,
    pub len: usize,
    pub memory: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct WasmWriteArgs {
    pub addr: String,
    pub hex: String,
    pub memory: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WasmFindArgs {
    pub pattern: String,
    pub start: Option<String>,
    pub end: Option<String>,
    #[serde(default = "default_wasm_find_max")]
    pub max: usize,
    pub memory: Option<String>,
}

impl Default for WasmFindArgs {
    fn default() -> Self {
        Self {
            pattern: String::new(),
            start: None,
            end: None,
            max: default_wasm_find_max(),
            memory: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct InterceptAddArgs {
    pub url_pattern: String,
    pub capture: Option<String>,
    pub respond: Option<String>,
    #[serde(default = "default_intercept_status")]
    pub status: u16,
}

impl Default for InterceptAddArgs {
    fn default() -> Self {
        Self {
            url_pattern: String::new(),
            capture: None,
            respond: None,
            status: default_intercept_status(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct IdArgs {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct ModeArgs {
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct DomainArgs {
    pub domain: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct CaptchaInjectArgs {
    pub token: String,
    #[serde(default = "default_captcha_type")]
    pub captcha_type: String,
    pub callback: Option<String>,
    pub click_after: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct NetworkRecordStartArgs {
    #[serde(default)]
    pub patterns: Vec<String>,
    #[serde(default)]
    pub no_bodies: bool,
    #[serde(default = "default_max_body_bytes")]
    pub max_body_bytes: usize,
    #[serde(default)]
    pub clear: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct NetworkLogArgs {
    pub limit: Option<usize>,
    pub pattern: Option<String>,
    pub method: Option<String>,
    pub status: Option<u16>,
    pub since: Option<u64>,
    #[serde(default)]
    pub compact: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct NetworkShowArgs {
    pub id: u64,
    #[serde(default)]
    pub body: bool,
    pub max_body: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct NetworkWaitArgs {
    pub pattern: Option<String>,
    pub method: Option<String>,
    pub status: Option<u16>,
    pub timeout: Option<u64>,
    pub since: Option<u64>,
    #[serde(default)]
    pub include_existing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NetworkExportArgs {
    pub path: String,
    #[serde(default = "default_network_export_format")]
    pub format: String,
    pub settle_ms: Option<u64>,
}

impl Default for NetworkExportArgs {
    fn default() -> Self {
        Self {
            path: String::new(),
            format: default_network_export_format(),
            settle_ms: None,
        }
    }
}

fn default_max_body_bytes() -> usize {
    10 * 1024 * 1024
}

fn default_network_export_format() -> String {
    "har".to_string()
}

fn default_viewport_width() -> u32 {
    390
}

fn default_viewport_height() -> u32 {
    844
}

fn default_device_pixel_ratio() -> f64 {
    2.0
}

fn default_wasm_find_max() -> usize {
    20
}

fn default_intercept_status() -> u16 {
    200
}

fn default_captcha_type() -> String {
    "auto".to_string()
}
