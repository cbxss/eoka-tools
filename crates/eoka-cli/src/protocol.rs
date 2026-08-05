use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", content = "args", rename_all = "snake_case")]
pub enum Request {
    Open(OpenArgs),
    Back,
    Forward,
    Reload,
    Snapshot(SnapshotArgs),
    Observe(ObserveArgs),
    Screenshot(ScreenshotArgs),
    Emulate(EmulateArgs),
    Info,
    Text,
    Find(TextArgs),
    Click(TargetArgs),
    #[serde(rename = "dblclick")]
    DblClick(TargetArgs),
    Fill(FillArgs),
    Select(SelectArgs),
    Hover(TargetArgs),
    Key(KeyArgs),
    Scroll(TargetArgs),
    Eval(ScriptArgs),
    Exec(ScriptArgs),
    Fetch(FetchArgs),
    Cookies,
    SetCookie(SetCookieArgs),
    DeleteCookie(DeleteCookieArgs),
    ClearCookies,
    Storage(StorageArgs),
    SetStorage(SetStorageArgs),
    DumpStorage,
    SaveState(PathArgs),
    LoadState(LoadStateArgs),
    Headers(HeadersArgs),
    Console(ConsoleArgs),
    Errors(ClearFlagArgs),
    TabList,
    TabNew(TabNewArgs),
    TabSwitch(TabIdArgs),
    TabClose(TabIdArgs),
    TabAttach(TabIdArgs),
    CloneFrom(CloneFromArgs),
    Wait(WaitArgs),
    SpaInfo,
    SpaNavigate(PathStringArgs),
    FakeCamera(FakeCameraArgs),
    WasmInfo,
    WasmRead(WasmReadArgs),
    WasmWrite(WasmWriteArgs),
    WasmFind(WasmFindArgs),
    InterceptAdd(InterceptAddArgs),
    InterceptList,
    InterceptRemove(IdArgs),
    InterceptLog(ClearFlagArgs),
    JsMode(ModeArgs),
    JsAllow(DomainArgs),
    JsBlock(DomainArgs),
    JsRemove(DomainArgs),
    JsList,
    NetworkRecordStart(NetworkRecordStartArgs),
    NetworkRecordStop,
    NetworkRecordStatus,
    NetworkLog(NetworkLogArgs),
    NetworkShow(NetworkShowArgs),
    NetworkWait(NetworkWaitArgs),
    NetworkSaveHar(NetworkExportArgs),
    NetworkExport(NetworkExportArgs),
    NetworkClear,
    Close,
    Shutdown,
    CaptchaInject(CaptchaInjectArgs),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpenArgs {
    pub url: String,
    pub headers: Option<serde_json::Value>,
    pub user_agent: Option<String>,
    #[serde(default)]
    pub bypass_csp: bool,
    pub inject_js: Option<String>,
    pub load_state: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SnapshotArgs {
    #[serde(default)]
    pub interactive: bool,
    #[serde(default)]
    pub all: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ObserveArgs {
    pub filter: Option<String>,
    pub max: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScreenshotArgs {
    pub output: Option<String>,
    #[serde(default)]
    pub annotate: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TextArgs {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TargetArgs {
    pub target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FillArgs {
    pub target: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SelectArgs {
    pub target: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KeyArgs {
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScriptArgs {
    pub code: Option<String>,
    pub file: Option<String>,
    pub max_size: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SetCookieArgs {
    pub name: String,
    pub value: String,
    pub domain: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeleteCookieArgs {
    pub name: String,
    pub domain: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StorageArgs {
    pub key: Option<String>,
    #[serde(default)]
    pub session_storage: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SetStorageArgs {
    pub key: String,
    pub value: String,
    #[serde(default)]
    pub session_storage: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PathArgs {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LoadStateArgs {
    pub path: String,
    #[serde(default)]
    pub no_navigate: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HeadersArgs {
    pub headers_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConsoleArgs {
    #[serde(default)]
    pub clear: bool,
    pub level: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClearFlagArgs {
    #[serde(default)]
    pub clear: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TabNewArgs {
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TabIdArgs {
    pub tab_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CloneFromArgs {
    pub source: String,
    pub to: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WaitArgs {
    pub ms: Option<u64>,
    pub text: Option<String>,
    pub url: Option<String>,
    pub load: Option<String>,
    pub timeout: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PathStringArgs {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FakeCameraArgs {
    pub file: String,
    #[serde(default)]
    pub loop_video: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WasmReadArgs {
    pub addr: String,
    pub len: usize,
    pub memory: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WasmWriteArgs {
    pub addr: String,
    pub hex: String,
    pub memory: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IdArgs {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModeArgs {
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DomainArgs {
    pub domain: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CaptchaInjectArgs {
    pub token: String,
    #[serde(default = "default_captcha_type")]
    pub captcha_type: String,
    pub callback: Option<String>,
    pub click_after: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetworkLogArgs {
    pub limit: Option<usize>,
    pub pattern: Option<String>,
    pub method: Option<String>,
    pub status: Option<u16>,
    pub since: Option<u64>,
    #[serde(default)]
    pub compact: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetworkShowArgs {
    pub id: u64,
    #[serde(default)]
    pub body: bool,
    pub max_body: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetworkWaitArgs {
    pub pattern: Option<String>,
    pub method: Option<String>,
    pub status: Option<u16>,
    pub timeout: Option<u64>,
    pub since: Option<u64>,
    #[serde(default)]
    pub include_existing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

impl Request {
    pub fn cmd(&self) -> &'static str {
        match self {
            Self::Open(_) => "open",
            Self::Back => "back",
            Self::Forward => "forward",
            Self::Reload => "reload",
            Self::Snapshot(_) => "snapshot",
            Self::Observe(_) => "observe",
            Self::Screenshot(_) => "screenshot",
            Self::Emulate(_) => "emulate",
            Self::Info => "info",
            Self::Text => "text",
            Self::Find(_) => "find",
            Self::Click(_) => "click",
            Self::DblClick(_) => "dblclick",
            Self::Fill(_) => "fill",
            Self::Select(_) => "select",
            Self::Hover(_) => "hover",
            Self::Key(_) => "key",
            Self::Scroll(_) => "scroll",
            Self::Eval(_) => "eval",
            Self::Exec(_) => "exec",
            Self::Fetch(_) => "fetch",
            Self::Cookies => "cookies",
            Self::SetCookie(_) => "set_cookie",
            Self::DeleteCookie(_) => "delete_cookie",
            Self::ClearCookies => "clear_cookies",
            Self::Storage(_) => "storage",
            Self::SetStorage(_) => "set_storage",
            Self::DumpStorage => "dump_storage",
            Self::SaveState(_) => "save_state",
            Self::LoadState(_) => "load_state",
            Self::Headers(_) => "headers",
            Self::Console(_) => "console",
            Self::Errors(_) => "errors",
            Self::TabList => "tab_list",
            Self::TabNew(_) => "tab_new",
            Self::TabSwitch(_) => "tab_switch",
            Self::TabClose(_) => "tab_close",
            Self::TabAttach(_) => "tab_attach",
            Self::CloneFrom(_) => "clone_from",
            Self::Wait(_) => "wait",
            Self::SpaInfo => "spa_info",
            Self::SpaNavigate(_) => "spa_navigate",
            Self::FakeCamera(_) => "fake_camera",
            Self::WasmInfo => "wasm_info",
            Self::WasmRead(_) => "wasm_read",
            Self::WasmWrite(_) => "wasm_write",
            Self::WasmFind(_) => "wasm_find",
            Self::InterceptAdd(_) => "intercept_add",
            Self::InterceptList => "intercept_list",
            Self::InterceptRemove(_) => "intercept_remove",
            Self::InterceptLog(_) => "intercept_log",
            Self::JsMode(_) => "js_mode",
            Self::JsAllow(_) => "js_allow",
            Self::JsBlock(_) => "js_block",
            Self::JsRemove(_) => "js_remove",
            Self::JsList => "js_list",
            Self::NetworkRecordStart(_) => "network_record_start",
            Self::NetworkRecordStop => "network_record_stop",
            Self::NetworkRecordStatus => "network_record_status",
            Self::NetworkLog(_) => "network_log",
            Self::NetworkShow(_) => "network_show",
            Self::NetworkWait(_) => "network_wait",
            Self::NetworkSaveHar(_) => "network_save_har",
            Self::NetworkExport(_) => "network_export",
            Self::NetworkClear => "network_clear",
            Self::Close => "close",
            Self::Shutdown => "shutdown",
            Self::CaptchaInject(_) => "captcha_inject",
        }
    }

    pub fn args_json(&self) -> serde_json::Value {
        match self {
            Self::Open(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::Snapshot(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::Observe(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::Screenshot(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::Emulate(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::Find(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::Click(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::DblClick(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::Fill(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::Select(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::Hover(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::Key(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::Scroll(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::Eval(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::Exec(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::Fetch(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::SetCookie(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::DeleteCookie(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::Storage(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::SetStorage(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::SaveState(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::LoadState(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::Headers(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::Console(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::Errors(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::TabNew(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::TabSwitch(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::TabClose(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::TabAttach(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::CloneFrom(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::Wait(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::SpaNavigate(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::FakeCamera(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::WasmRead(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::WasmWrite(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::WasmFind(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::InterceptAdd(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::InterceptRemove(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::InterceptLog(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::JsMode(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::JsAllow(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::JsBlock(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::JsRemove(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::CaptchaInject(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::NetworkRecordStart(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::NetworkLog(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::NetworkShow(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::NetworkWait(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::NetworkSaveHar(args) => serde_json::to_value(args).unwrap_or_default(),
            Self::NetworkExport(args) => serde_json::to_value(args).unwrap_or_default(),
            _ => serde_json::json!({}),
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

#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    pub fn ok(data: impl Into<serde_json::Value>) -> Self {
        Self {
            ok: true,
            data: Some(data.into()),
            error: None,
        }
    }

    pub fn ok_text(msg: impl Into<String>) -> Self {
        Self::ok(serde_json::Value::String(msg.into()))
    }

    pub fn err(msg: impl Into<String>) -> Self {
        Self {
            ok: false,
            data: None,
            error: Some(msg.into()),
        }
    }
}
pub async fn write_msg<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    msg: &impl Serialize,
) -> std::io::Result<()> {
    let payload = serde_json::to_vec(msg).map_err(|e| std::io::Error::other(e.to_string()))?;
    let len = (payload.len() as u32).to_be_bytes();
    writer.write_all(&len).await?;
    writer.write_all(&payload).await?;
    writer.flush().await?;
    Ok(())
}
pub async fn read_msg<R: AsyncReadExt + Unpin, T: for<'de> Deserialize<'de>>(
    reader: &mut R,
) -> std::io::Result<T> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;

    if len > 64 * 1024 * 1024 {
        return Err(std::io::Error::other("message too large (>64MB)"));
    }

    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).await?;
    serde_json::from_slice(&buf).map_err(|e| std::io::Error::other(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::{UnixListener, UnixStream};

    fn temp_socket_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "eoka-protocol-test-{}-{}.sock",
            std::process::id(),
            name
        ))
    }

    #[tokio::test]
    async fn round_trip_over_unix_socket() {
        let sock_path = temp_socket_path("roundtrip");
        let _ = std::fs::remove_file(&sock_path);
        let listener = UnixListener::bind(&sock_path).unwrap();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (mut reader, mut writer) = stream.into_split();
            let req: Request = read_msg(&mut reader).await.unwrap();
            assert_eq!(req.cmd(), "click");
            assert_eq!(req.args_json(), serde_json::json!({ "target": "@e1" }));
            write_msg(&mut writer, &Response::ok_text("pong"))
                .await
                .unwrap();
        });

        let stream = UnixStream::connect(&sock_path).await.unwrap();
        let (mut reader, mut writer) = stream.into_split();
        write_msg(
            &mut writer,
            &Request::Click(TargetArgs {
                target: "@e1".into(),
            }),
        )
        .await
        .unwrap();
        let response: Response = read_msg(&mut reader).await.unwrap();

        server.await.unwrap();
        let _ = std::fs::remove_file(&sock_path);

        assert!(response.ok);
        assert_eq!(
            response.data,
            Some(serde_json::Value::String("pong".into()))
        );
        assert_eq!(response.error, None);
    }

    #[tokio::test]
    async fn read_msg_rejects_oversized_length_prefix() {
        let sock_path = temp_socket_path("oversized");
        let _ = std::fs::remove_file(&sock_path);
        let listener = UnixListener::bind(&sock_path).unwrap();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (mut reader, _writer) = stream.into_split();
            let result: std::io::Result<Request> = read_msg(&mut reader).await;
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("too large"));
        });

        let mut stream = UnixStream::connect(&sock_path).await.unwrap();
        let oversized_len: u32 = 65 * 1024 * 1024;
        stream
            .write_all(&oversized_len.to_be_bytes())
            .await
            .unwrap();

        server.await.unwrap();
        let _ = std::fs::remove_file(&sock_path);
    }

    #[test]
    fn typed_network_request_round_trips_with_cmd_and_args() {
        let request = Request::NetworkWait(NetworkWaitArgs {
            pattern: Some("*/api/*".into()),
            method: Some("POST".into()),
            status: Some(201),
            timeout: Some(5000),
            since: Some(10),
            include_existing: false,
        });

        let value = serde_json::to_value(&request).unwrap();
        assert_eq!(value["cmd"], "network_wait");
        assert_eq!(value["args"]["pattern"], "*/api/*");

        let parsed: Request = serde_json::from_value(value).unwrap();
        assert_eq!(parsed.cmd(), "network_wait");
        assert_eq!(parsed.args_json()["status"], 201);
    }

    #[test]
    fn typed_request_rejects_unknown_command() {
        let result: Result<Request, _> =
            serde_json::from_value(serde_json::json!({ "cmd": "not_real", "args": {} }));

        assert!(result.is_err());
    }

    #[test]
    fn legacy_network_save_har_request_is_still_accepted() {
        let request: Request = serde_json::from_value(serde_json::json!({
            "cmd": "network_save_har",
            "args": { "path": "/tmp/session.har" }
        }))
        .unwrap();

        assert_eq!(request.cmd(), "network_save_har");
        assert_eq!(request.args_json()["format"], "har");
    }
}
