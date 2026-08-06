mod args;
mod io;
mod metadata;
mod response;

pub use args::*;
pub use io::{read_msg, write_msg};
pub use metadata::{OperationCapability, OperationExposure, ToolManifestEntry};
pub use response::{ErrorDetail, Response, ResponseMeta};

use serde::{Deserialize, Serialize};

macro_rules! define_operations {
    (
        $(
            $variant:ident {
                path: $path:literal,
                cmd: $cmd:literal,
                name: $name:literal,
                description: $description:literal,
                capability: $capability:ident,
                exposure: $exposure:ident,
                read_only: $read_only:literal,
                destructive: $destructive:literal
                $(, input: $input:ty)?
                $(,)?
            }
        ),+ $(,)?
    ) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum OperationId {
            $($variant),+
        }

        #[derive(Debug, Clone, Copy)]
        pub struct OperationSpec {
            pub id: OperationId,
            pub path: &'static str,
            pub cmd: &'static str,
            pub name: &'static str,
            pub description: &'static str,
            pub capability: OperationCapability,
            pub exposure: OperationExposure,
            pub read_only: bool,
            pub destructive: bool,
        }

        #[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
        #[serde(tag = "cmd", content = "args")]
        pub enum Request {
            $(
                #[serde(rename = $cmd)]
                $variant$(($input))?,
            )+
        }

        pub const OPERATIONS: &[OperationSpec] = &[
            $(OperationSpec {
                id: OperationId::$variant,
                path: $path,
                cmd: $cmd,
                name: $name,
                description: $description,
                capability: OperationCapability::$capability,
                exposure: OperationExposure::$exposure,
                read_only: $read_only,
                destructive: $destructive,
            }),+
        ];
        pub fn all_operations() -> &'static [OperationSpec] {
            OPERATIONS
        }

        pub fn operation_by_path(path: &str) -> Option<&'static OperationSpec> {
            OPERATIONS.iter().find(|operation| operation.path == path)
        }

        pub fn operation_by_cmd(cmd: &str) -> Option<&'static OperationSpec> {
            OPERATIONS.iter().find(|operation| operation.cmd == cmd)
        }

        pub fn default_agent_operations() -> impl Iterator<Item = &'static OperationSpec> {
            OPERATIONS
                .iter()
                .filter(|operation| operation.exposure == OperationExposure::DefaultAgent)
        }

        pub fn exposed_operations(
            include_opt_in: bool,
        ) -> impl Iterator<Item = &'static OperationSpec> {
            OPERATIONS.iter().filter(move |operation| {
                operation.exposure == OperationExposure::DefaultAgent
                    || (include_opt_in && operation.exposure == OperationExposure::OptIn)
            })
        }

        pub fn request_from_operation_path(
            path: &str,
            input: serde_json::Value,
        ) -> Result<Request, String> {
            let operation = operation_by_path(path)
                .ok_or_else(|| format!("unknown eoka operation path: {path}"))?;
            request_from_cmd(operation.cmd, input)
        }

        pub fn request_from_cmd(cmd: &str, input: serde_json::Value) -> Result<Request, String> {
            match cmd {
                $(
                    $cmd => Ok(define_operations!(@request Request::$variant, input $(, $input)?)),
                )+
                _ => Err(format!("unknown eoka protocol command: {cmd}")),
            }
        }

        pub fn input_schema_for_cmd(cmd: &str) -> serde_json::Value {
            match cmd {
                $(
                    $cmd => define_operations!(@schema $( $input )?),
                )+
                _ => serde_json::json!({ "type": "object", "additionalProperties": false }),
            }
        }

        pub fn input_schema_for_operation(operation: &OperationSpec) -> serde_json::Value {
            input_schema_for_cmd(operation.cmd)
        }

        pub fn tags_for_operation(operation: &OperationSpec) -> Vec<&'static str> {
            vec!["eoka", operation.capability.as_str()]
        }

        pub fn manifest_entry_for_operation(
            namespace: &str,
            operation: &OperationSpec,
        ) -> ToolManifestEntry {
            ToolManifestEntry {
                path: format!("{}.{}", namespace, operation.path),
                cmd: operation.cmd,
                name: operation.name,
                description: operation.description,
                capability: operation.capability.as_str(),
                exposure: operation.exposure.as_str(),
                read_only: operation.read_only,
                destructive: operation.destructive,
                input_schema: input_schema_for_operation(operation),
                tags: tags_for_operation(operation),
            }
        }

        pub fn manifest_for_operations(
            namespace: &str,
            include_opt_in: bool,
        ) -> Vec<ToolManifestEntry> {
            exposed_operations(include_opt_in)
                .map(|operation| manifest_entry_for_operation(namespace, operation))
                .collect()
        }

        impl Request {
            pub fn cmd(&self) -> &'static str {
                match self {
                    $(
                        define_operations!(@match_variant Self::$variant $(, $input)?) => $cmd,
                    )+
                }
            }

            pub fn args_json(&self) -> serde_json::Value {
                serde_json::to_value(self)
                    .ok()
                    .and_then(|value| value.get("args").cloned())
                    .unwrap_or_else(|| serde_json::json!({}))
            }
        }
    };

    (@request $variant:path, $input:ident, $ty:ty) => {{
        serde_json::from_value::<$ty>($input)
            .map($variant)
            .map_err(|error| error.to_string())?
    }};

    (@request $variant:path, $input:ident) => {{
        if $input != serde_json::json!({}) && $input != serde_json::Value::Null {
            return Err("expected empty object for zero-argument command".to_string());
        }
        $variant
    }};

    (@schema $ty:ty) => {
        schema_for::<$ty>()
    };

    (@schema) => {
        serde_json::json!({ "type": "object", "additionalProperties": false })
    };

    (@match_variant $variant:path, $ty:ty) => {
        $variant(_)
    };

    (@match_variant $variant:path) => {
        $variant
    };

}

define_operations! {
    Open {
        path: "open",
        cmd: "open",
        name: "open",
        description: "Navigate to URL",
        capability: Navigation,
        exposure: DefaultAgent,
        read_only: false,
        destructive: false,
        input: OpenArgs,
    },
    Back {
        path: "back",
        cmd: "back",
        name: "back",
        description: "Go back",
        capability: Navigation,
        exposure: DefaultAgent,
        read_only: false,
        destructive: false,
    },
    Forward {
        path: "forward",
        cmd: "forward",
        name: "forward",
        description: "Go forward",
        capability: Navigation,
        exposure: DefaultAgent,
        read_only: false,
        destructive: false,
    },
    Reload {
        path: "reload",
        cmd: "reload",
        name: "reload",
        description: "Reload page",
        capability: Navigation,
        exposure: DefaultAgent,
        read_only: false,
        destructive: false,
    },
    Snapshot {
        path: "snapshot",
        cmd: "snapshot",
        name: "snapshot",
        description: "Accessibility snapshot",
        capability: Observation,
        exposure: DefaultAgent,
        read_only: true,
        destructive: false,
        input: SnapshotArgs,
    },
    Observe {
        path: "observe",
        cmd: "observe",
        name: "observe",
        description: "Observe interactive elements",
        capability: Observation,
        exposure: DefaultAgent,
        read_only: true,
        destructive: false,
        input: ObserveArgs,
    },
    Screenshot {
        path: "screenshot",
        cmd: "screenshot",
        name: "screenshot",
        description: "Take screenshot",
        capability: Observation,
        exposure: DefaultAgent,
        read_only: true,
        destructive: false,
        input: ScreenshotArgs,
    },
    Emulate {
        path: "emulate",
        cmd: "emulate",
        name: "emulate",
        description: "Emulate viewport",
        capability: Navigation,
        exposure: DefaultAgent,
        read_only: false,
        destructive: false,
        input: EmulateArgs,
    },
    Info {
        path: "info",
        cmd: "info",
        name: "info",
        description: "Page URL and title",
        capability: Observation,
        exposure: DefaultAgent,
        read_only: true,
        destructive: false,
    },
    Text {
        path: "text",
        cmd: "text",
        name: "text",
        description: "Visible page text",
        capability: Observation,
        exposure: DefaultAgent,
        read_only: true,
        destructive: false,
    },
    Find {
        path: "find",
        cmd: "find",
        name: "find",
        description: "Find elements by text",
        capability: Observation,
        exposure: DefaultAgent,
        read_only: true,
        destructive: false,
        input: TextArgs,
    },
    Click {
        path: "click",
        cmd: "click",
        name: "click",
        description: "Click target",
        capability: Interaction,
        exposure: DefaultAgent,
        read_only: false,
        destructive: true,
        input: TargetArgs,
    },
    DblClick {
        path: "double_click",
        cmd: "dblclick",
        name: "double_click",
        description: "Double click target",
        capability: Interaction,
        exposure: DefaultAgent,
        read_only: false,
        destructive: true,
        input: TargetArgs,
    },
    Fill {
        path: "fill",
        cmd: "fill",
        name: "fill",
        description: "Fill input",
        capability: Interaction,
        exposure: DefaultAgent,
        read_only: false,
        destructive: true,
        input: FillArgs,
    },
    Select {
        path: "select",
        cmd: "select",
        name: "select",
        description: "Select option",
        capability: Interaction,
        exposure: DefaultAgent,
        read_only: false,
        destructive: true,
        input: SelectArgs,
    },
    Hover {
        path: "hover",
        cmd: "hover",
        name: "hover",
        description: "Hover target",
        capability: Interaction,
        exposure: DefaultAgent,
        read_only: false,
        destructive: false,
        input: TargetArgs,
    },
    Key {
        path: "key",
        cmd: "key",
        name: "key",
        description: "Press key",
        capability: Interaction,
        exposure: DefaultAgent,
        read_only: false,
        destructive: true,
        input: KeyArgs,
    },
    Scroll {
        path: "scroll",
        cmd: "scroll",
        name: "scroll",
        description: "Scroll page or target",
        capability: Interaction,
        exposure: DefaultAgent,
        read_only: false,
        destructive: false,
        input: TargetArgs,
    },
    Eval {
        path: "eval",
        cmd: "eval",
        name: "eval",
        description: "Evaluate JavaScript",
        capability: JavaScript,
        exposure: DefaultAgent,
        read_only: false,
        destructive: false,
        input: ScriptArgs,
    },
    Exec {
        path: "exec",
        cmd: "exec",
        name: "exec",
        description: "Execute JavaScript",
        capability: JavaScript,
        exposure: DefaultAgent,
        read_only: false,
        destructive: true,
        input: ScriptArgs,
    },
    Fetch {
        path: "fetch",
        cmd: "fetch",
        name: "fetch",
        description: "Fetch URL in page context",
        capability: Network,
        exposure: DefaultAgent,
        read_only: false,
        destructive: false,
        input: FetchArgs,
    },
    Cookies {
        path: "cookies",
        cmd: "cookies",
        name: "cookies",
        description: "List cookies",
        capability: BrowserState,
        exposure: DefaultAgent,
        read_only: true,
        destructive: false,
    },
    SetCookie {
        path: "set_cookie",
        cmd: "set_cookie",
        name: "set_cookie",
        description: "Set cookie",
        capability: BrowserState,
        exposure: DefaultAgent,
        read_only: false,
        destructive: true,
        input: SetCookieArgs,
    },
    DeleteCookie {
        path: "delete_cookie",
        cmd: "delete_cookie",
        name: "delete_cookie",
        description: "Delete cookie",
        capability: BrowserState,
        exposure: DefaultAgent,
        read_only: false,
        destructive: true,
        input: DeleteCookieArgs,
    },
    ClearCookies {
        path: "clear_cookies",
        cmd: "clear_cookies",
        name: "clear_cookies",
        description: "Clear cookies",
        capability: BrowserState,
        exposure: DefaultAgent,
        read_only: false,
        destructive: true,
    },
    Storage {
        path: "storage",
        cmd: "storage",
        name: "storage",
        description: "Read storage",
        capability: BrowserState,
        exposure: DefaultAgent,
        read_only: true,
        destructive: false,
        input: StorageArgs,
    },
    SetStorage {
        path: "set_storage",
        cmd: "set_storage",
        name: "set_storage",
        description: "Set storage",
        capability: BrowserState,
        exposure: DefaultAgent,
        read_only: false,
        destructive: true,
        input: SetStorageArgs,
    },
    DumpStorage {
        path: "dump_storage",
        cmd: "dump_storage",
        name: "dump_storage",
        description: "Dump storage",
        capability: BrowserState,
        exposure: DefaultAgent,
        read_only: true,
        destructive: false,
    },
    SaveState {
        path: "save_state",
        cmd: "save_state",
        name: "save_state",
        description: "Save browser state",
        capability: BrowserState,
        exposure: DefaultAgent,
        read_only: true,
        destructive: false,
        input: PathArgs,
    },
    LoadState {
        path: "load_state",
        cmd: "load_state",
        name: "load_state",
        description: "Load browser state",
        capability: BrowserState,
        exposure: DefaultAgent,
        read_only: false,
        destructive: true,
        input: LoadStateArgs,
    },
    Headers {
        path: "headers",
        cmd: "headers",
        name: "headers",
        description: "Set extra headers",
        capability: Network,
        exposure: DefaultAgent,
        read_only: false,
        destructive: false,
        input: HeadersArgs,
    },
    Console {
        path: "console",
        cmd: "console",
        name: "console",
        description: "Read console output",
        capability: Observation,
        exposure: DefaultAgent,
        read_only: true,
        destructive: false,
        input: ConsoleArgs,
    },
    Errors {
        path: "errors",
        cmd: "errors",
        name: "errors",
        description: "Read JavaScript errors",
        capability: Observation,
        exposure: DefaultAgent,
        read_only: true,
        destructive: false,
        input: ClearFlagArgs,
    },
    TabList {
        path: "tab.list",
        cmd: "tab_list",
        name: "tab.list",
        description: "List tabs",
        capability: Tabs,
        exposure: DefaultAgent,
        read_only: true,
        destructive: false,
    },
    TabNew {
        path: "tab.new",
        cmd: "tab_new",
        name: "tab.new",
        description: "Open new tab",
        capability: Tabs,
        exposure: DefaultAgent,
        read_only: false,
        destructive: false,
        input: TabNewArgs,
    },
    TabSwitch {
        path: "tab.switch",
        cmd: "tab_switch",
        name: "tab.switch",
        description: "Switch tab",
        capability: Tabs,
        exposure: DefaultAgent,
        read_only: false,
        destructive: false,
        input: TabIdArgs,
    },
    TabClose {
        path: "tab.close",
        cmd: "tab_close",
        name: "tab.close",
        description: "Close tab",
        capability: Tabs,
        exposure: DefaultAgent,
        read_only: false,
        destructive: true,
        input: TabIdArgs,
    },
    TabAttach {
        path: "tab.attach",
        cmd: "tab_attach",
        name: "tab.attach",
        description: "Attach tab",
        capability: Tabs,
        exposure: DefaultAgent,
        read_only: false,
        destructive: false,
        input: TabIdArgs,
    },
    CloneFrom {
        path: "clone_from",
        cmd: "clone_from",
        name: "clone_from",
        description: "Clone browser state",
        capability: BrowserState,
        exposure: DefaultAgent,
        read_only: false,
        destructive: true,
        input: CloneFromArgs,
    },
    Wait {
        path: "wait",
        cmd: "wait",
        name: "wait",
        description: "Wait for page condition",
        capability: Observation,
        exposure: DefaultAgent,
        read_only: true,
        destructive: false,
        input: WaitArgs,
    },
    SpaInfo {
        path: "spa.info",
        cmd: "spa_info",
        name: "spa.info",
        description: "SPA routing info",
        capability: Spa,
        exposure: DefaultAgent,
        read_only: true,
        destructive: false,
    },
    SpaNavigate {
        path: "spa.navigate",
        cmd: "spa_navigate",
        name: "spa.navigate",
        description: "SPA navigation",
        capability: Spa,
        exposure: DefaultAgent,
        read_only: false,
        destructive: false,
        input: PathStringArgs,
    },
    FakeCamera {
        path: "fake_camera",
        cmd: "fake_camera",
        name: "fake_camera",
        description: "Inject fake camera",
        capability: Media,
        exposure: DefaultAgent,
        read_only: false,
        destructive: true,
        input: FakeCameraArgs,
    },
    WasmInfo {
        path: "wasm.info",
        cmd: "wasm_info",
        name: "wasm.info",
        description: "WASM memory info",
        capability: Wasm,
        exposure: DefaultAgent,
        read_only: true,
        destructive: false,
    },
    WasmRead {
        path: "wasm.read",
        cmd: "wasm_read",
        name: "wasm.read",
        description: "Read WASM memory",
        capability: Wasm,
        exposure: DefaultAgent,
        read_only: true,
        destructive: false,
        input: WasmReadArgs,
    },
    WasmWrite {
        path: "wasm.write",
        cmd: "wasm_write",
        name: "wasm.write",
        description: "Write WASM memory",
        capability: Wasm,
        exposure: DefaultAgent,
        read_only: false,
        destructive: true,
        input: WasmWriteArgs,
    },
    WasmFind {
        path: "wasm.find",
        cmd: "wasm_find",
        name: "wasm.find",
        description: "Find WASM memory pattern",
        capability: Wasm,
        exposure: DefaultAgent,
        read_only: true,
        destructive: false,
        input: WasmFindArgs,
    },
    InterceptAdd {
        path: "intercept.add",
        cmd: "intercept_add",
        name: "intercept.add",
        description: "Add network interception rule",
        capability: Network,
        exposure: OptIn,
        read_only: false,
        destructive: true,
        input: InterceptAddArgs,
    },
    InterceptList {
        path: "intercept.list",
        cmd: "intercept_list",
        name: "intercept.list",
        description: "List network interception rules",
        capability: Network,
        exposure: OptIn,
        read_only: true,
        destructive: false,
    },
    InterceptRemove {
        path: "intercept.remove",
        cmd: "intercept_remove",
        name: "intercept.remove",
        description: "Remove network interception rule",
        capability: Network,
        exposure: OptIn,
        read_only: false,
        destructive: true,
        input: IdArgs,
    },
    InterceptLog {
        path: "intercept.log",
        cmd: "intercept_log",
        name: "intercept.log",
        description: "Read intercepted request log",
        capability: Network,
        exposure: OptIn,
        read_only: true,
        destructive: false,
        input: ClearFlagArgs,
    },
    JsMode {
        path: "js.mode",
        cmd: "js_mode",
        name: "js.mode",
        description: "Set JavaScript policy mode",
        capability: Policy,
        exposure: DefaultAgent,
        read_only: false,
        destructive: false,
        input: ModeArgs,
    },
    JsAllow {
        path: "js.allow",
        cmd: "js_allow",
        name: "js.allow",
        description: "Allow JavaScript domain",
        capability: Policy,
        exposure: DefaultAgent,
        read_only: false,
        destructive: false,
        input: DomainArgs,
    },
    JsBlock {
        path: "js.block",
        cmd: "js_block",
        name: "js.block",
        description: "Block JavaScript domain",
        capability: Policy,
        exposure: DefaultAgent,
        read_only: false,
        destructive: false,
        input: DomainArgs,
    },
    JsRemove {
        path: "js.remove",
        cmd: "js_remove",
        name: "js.remove",
        description: "Remove JavaScript domain rule",
        capability: Policy,
        exposure: DefaultAgent,
        read_only: false,
        destructive: true,
        input: DomainArgs,
    },
    JsList {
        path: "js.list",
        cmd: "js_list",
        name: "js.list",
        description: "List JavaScript policy",
        capability: Policy,
        exposure: DefaultAgent,
        read_only: true,
        destructive: false,
    },
    NetworkRecordStart {
        path: "network.record.start",
        cmd: "network_record_start",
        name: "network.record.start",
        description: "Start network recording",
        capability: Network,
        exposure: OptIn,
        read_only: false,
        destructive: false,
        input: NetworkRecordStartArgs,
    },
    NetworkRecordStop {
        path: "network.record.stop",
        cmd: "network_record_stop",
        name: "network.record.stop",
        description: "Stop network recording",
        capability: Network,
        exposure: OptIn,
        read_only: false,
        destructive: false,
    },
    NetworkRecordStatus {
        path: "network.record.status",
        cmd: "network_record_status",
        name: "network.record.status",
        description: "Network recorder status",
        capability: Network,
        exposure: OptIn,
        read_only: true,
        destructive: false,
    },
    NetworkLog {
        path: "network.log",
        cmd: "network_log",
        name: "network.log",
        description: "Read network log",
        capability: Network,
        exposure: OptIn,
        read_only: true,
        destructive: false,
        input: NetworkLogArgs,
    },
    NetworkShow {
        path: "network.show",
        cmd: "network_show",
        name: "network.show",
        description: "Show network entry details",
        capability: Network,
        exposure: OptIn,
        read_only: true,
        destructive: false,
        input: NetworkShowArgs,
    },
    NetworkWait {
        path: "network.wait",
        cmd: "network_wait",
        name: "network.wait",
        description: "Wait for a matching network request",
        capability: Network,
        exposure: OptIn,
        read_only: true,
        destructive: false,
        input: NetworkWaitArgs,
    },
    NetworkExport {
        path: "network.export",
        cmd: "network_export",
        name: "network.export",
        description: "Export network traffic",
        capability: Network,
        exposure: OptIn,
        read_only: true,
        destructive: false,
        input: NetworkExportArgs,
    },
    NetworkClear {
        path: "network.clear",
        cmd: "network_clear",
        name: "network.clear",
        description: "Clear network log",
        capability: Network,
        exposure: OptIn,
        read_only: false,
        destructive: true,
    },
    Close {
        path: "close",
        cmd: "close",
        name: "close",
        description: "Close browser session",
        capability: Lifecycle,
        exposure: Lifecycle,
        read_only: false,
        destructive: true,
    },
    Shutdown {
        path: "shutdown",
        cmd: "shutdown",
        name: "shutdown",
        description: "Shut down daemon",
        capability: Lifecycle,
        exposure: Lifecycle,
        read_only: false,
        destructive: true,
    },
    CaptchaInject {
        path: "captcha.inject",
        cmd: "captcha_inject",
        name: "captcha.inject",
        description: "Inject a solved CAPTCHA token",
        capability: Captcha,
        exposure: OptIn,
        read_only: false,
        destructive: true,
        input: CaptchaInjectArgs,
    },
}

fn schema_for<T: schemars::JsonSchema>() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(T))
        .unwrap_or_else(|_| serde_json::json!({ "type": "object", "additionalProperties": true }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;
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
    fn operation_catalog_maps_stable_paths_to_protocol_commands() {
        let tab = operation_by_path("tab.new").unwrap();
        assert_eq!(tab.cmd, "tab_new");
        assert_eq!(tab.exposure, OperationExposure::DefaultAgent);

        let request = request_from_operation_path(
            "tab.new",
            serde_json::json!({"url":"https://example.com"}),
        )
        .unwrap();
        assert_eq!(request.cmd(), "tab_new");
        assert_eq!(request.args_json()["url"], "https://example.com");
    }

    #[test]
    fn default_catalog_excludes_opt_in_and_lifecycle_operations() {
        let paths: Vec<&str> = default_agent_operations()
            .map(|operation| operation.path)
            .collect();

        assert!(paths.contains(&"open"));
        assert!(paths.contains(&"tab.list"));
        assert!(!paths.contains(&"network.log"));
        assert!(!paths.contains(&"close"));
    }

    #[test]
    fn operation_catalog_has_unique_paths_and_commands() {
        let mut paths = std::collections::BTreeSet::new();
        let mut commands = std::collections::BTreeSet::new();
        let mut ids = std::collections::BTreeSet::new();

        for operation in all_operations() {
            assert!(ids.insert(operation.id), "duplicate id {:?}", operation.id);
            assert!(
                paths.insert(operation.path),
                "duplicate path {}",
                operation.path
            );
            assert!(
                commands.insert(operation.cmd),
                "duplicate cmd {}",
                operation.cmd
            );
            assert_eq!(operation_by_path(operation.path).unwrap().id, operation.id);
            assert_eq!(operation_by_cmd(operation.cmd).unwrap().id, operation.id);
            assert_eq!(
                request_from_operation_path(
                    operation.path,
                    operation.representative_input_for_test()
                )
                .unwrap()
                .cmd(),
                operation.cmd
            );
        }
    }

    #[test]
    fn manifest_uses_catalog_metadata_and_schemas() {
        let default_manifest = manifest_for_operations("eoka", false);
        let paths: std::collections::BTreeSet<&str> = default_manifest
            .iter()
            .map(|entry| entry.path.as_str())
            .collect();

        assert!(paths.contains("eoka.open"));
        assert!(paths.contains("eoka.tab.list"));
        assert!(!paths.contains("eoka.network.log"));
        assert!(!paths.contains("eoka.close"));

        let open = default_manifest
            .iter()
            .find(|entry| entry.path == "eoka.open")
            .unwrap();
        assert_eq!(open.capability, "navigation");
        assert_eq!(open.exposure, "defaultAgent");
        assert_eq!(open.tags, vec!["eoka", "navigation"]);
        assert!(open.input_schema.get("properties").is_some());

        let all_manifest = manifest_for_operations("eoka", true);
        let all_paths: std::collections::BTreeSet<&str> = all_manifest
            .iter()
            .map(|entry| entry.path.as_str())
            .collect();
        assert!(all_paths.contains("eoka.network.log"));
        assert!(!all_paths.contains("eoka.close"));
    }

    #[test]
    fn shared_mcp_inputs_keep_schema_descriptions() {
        let click_schema = input_schema_for_cmd("click");
        assert_eq!(
            click_schema["properties"]["target"]["description"],
            "Target element. Supports index, snapshot refs like @e1, text:Submit, placeholder:Email, role:button, css:form button, id:my-btn, or plain text search."
        );

        let tab_schema = input_schema_for_cmd("tab_switch");
        assert_eq!(
            tab_schema["properties"]["tab_id"]["description"],
            "Tab ID from list_tabs."
        );
    }

    impl OperationSpec {
        fn representative_input_for_test(&self) -> serde_json::Value {
            match self.cmd {
                "open" => serde_json::json!({"url":"about:blank"}),
                "find" => serde_json::json!({"text":"needle"}),
                "click" | "dblclick" | "hover" | "scroll" => {
                    serde_json::json!({"target":"body"})
                }
                "fill" => serde_json::json!({"target":"body","text":"value"}),
                "select" => serde_json::json!({"target":"select","value":"A"}),
                "key" => serde_json::json!({"key":"Enter"}),
                "eval" | "exec" => serde_json::json!({"code":"return 1"}),
                "emulate" => serde_json::json!({"width":390,"height":844}),
                "fetch" => serde_json::json!({"url":"https://example.com"}),
                "set_cookie" => serde_json::json!({"name":"a","value":"b"}),
                "delete_cookie" => serde_json::json!({"name":"a"}),
                "storage" => serde_json::json!({"key":"a"}),
                "set_storage" => serde_json::json!({"key":"a","value":"b"}),
                "save_state" | "load_state" | "network_har" | "network_export" => {
                    serde_json::json!({"path":"/tmp/eoka-state.json"})
                }
                "headers" => serde_json::json!({"headers_json":"{}"}),
                "console" | "errors" => serde_json::json!({"clear":false}),
                "tab_new" => serde_json::json!({"url":"about:blank"}),
                "tab_switch" | "tab_close" | "tab_attach" => serde_json::json!({"tab_id":"1"}),
                "wait" => serde_json::json!({"text":"ready"}),
                "spa_navigate" => serde_json::json!({"path":"/"}),
                "fake_camera" => serde_json::json!({"file":"/tmp/video.y4m"}),
                "wasm_read" => serde_json::json!({"addr":"0","len":1}),
                "wasm_write" => serde_json::json!({"addr":"0","hex":"00"}),
                "wasm_find" => serde_json::json!({"pattern":"00"}),
                "network_record_start" => serde_json::json!({"patterns":["*"]}),
                "network_log" => serde_json::json!({"limit":10}),
                "network_show" => serde_json::json!({"id":1}),
                "intercept_remove" => serde_json::json!({"id":"1"}),
                "network_wait" => serde_json::json!({"pattern":"*"}),
                "intercept_add" => serde_json::json!({"url_pattern":"*"}),
                "intercept_log" => serde_json::json!({"clear":false}),
                "js_mode" => serde_json::json!({"mode":"block"}),
                "js_allow" | "js_block" | "js_remove" => {
                    serde_json::json!({"domain":"example.com"})
                }
                "clone_from" => serde_json::json!({"source":"9222"}),
                "captcha_solve" => serde_json::json!({
                    "api_key":"key",
                    "captcha_type":"hcaptcha",
                    "website_url":"https://example.com",
                    "website_key":"site"
                }),
                "captcha_detect" => serde_json::json!({}),
                "captcha_inject" => serde_json::json!({"token":"token"}),
                _ => serde_json::json!({}),
            }
        }
    }
}
