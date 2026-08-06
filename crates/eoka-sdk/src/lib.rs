pub mod launch_spec;
pub mod session;

mod client;

pub use client::{is_daemon_running, kill_daemon, send_command, EokaClient, SDK_HELPER_COMMANDS};
pub use eoka_protocol::{
    all_operations, default_agent_operations, exposed_operations, input_schema_for_cmd,
    input_schema_for_operation, manifest_for_operations, operation_by_cmd, operation_by_path,
    request_from_cmd, request_from_operation_path, tags_for_operation, CaptchaInjectArgs,
    ClearFlagArgs, CloneFromArgs, ConsoleArgs, DeleteCookieArgs, DomainArgs, EmulateArgs,
    ErrorDetail, FakeCameraArgs, FetchArgs, FillArgs, HeadersArgs, IdArgs, InterceptAddArgs,
    KeyArgs, LoadStateArgs, ModeArgs, NetworkExportArgs, NetworkLogArgs, NetworkRecordStartArgs,
    NetworkShowArgs, NetworkWaitArgs, ObserveArgs, OpenArgs, OperationCapability,
    OperationExposure, OperationId, OperationSpec, PathArgs, PathStringArgs, Request, Response,
    ResponseMeta, ScreenshotArgs, ScriptArgs, SelectArgs, SetCookieArgs, SetStorageArgs,
    SnapshotArgs, StorageArgs, TabIdArgs, TabNewArgs, TargetArgs, TextArgs, ToolManifestEntry,
    WaitArgs, WasmFindArgs, WasmReadArgs, WasmWriteArgs,
};
pub use launch_spec::{
    auto_connect, resolve_cdp_spec, resolve_proxy_spec, session_suffix, LaunchSpec,
};
