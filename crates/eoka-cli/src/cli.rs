use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "eoka", about = "Browser automation CLI", version)]
pub struct Cli {
    #[arg(
        long,
        default_value = "default",
        global = true,
        help = "Session name for isolated browser instances"
    )]
    pub session: String,
    #[arg(long, global = true, help = "JSON output mode")]
    pub json: bool,
    #[arg(
        long,
        global = true,
        help = "JSON output plus structured metadata for agent callers"
    )]
    pub agent: bool,
    #[arg(long, global = true, help = "Run browser in headed mode")]
    pub headed: bool,
    #[arg(
        long,
        value_name = "PORT|URL",
        global = true,
        env = "EOKA_CDP",
        help = "Connect to Chrome by DevTools port or WebSocket URL"
    )]
    pub cdp: Option<String>,
    #[arg(
        long,
        global = true,
        env = "EOKA_AUTO_CONNECT",
        help = "Auto-discover a running Chrome on ports 9222-9229"
    )]
    pub auto_connect: bool,
    #[arg(
        long,
        value_name = "PORT|URL",
        global = true,
        help = "Launch fresh Chrome and pre-load cookies/storage from a running Chrome"
    )]
    pub clone_state_from: Option<String>,
    #[arg(
        long,
        value_name = "auto|PATH",
        global = true,
        env = "EOKA_FROM_PROFILE",
        help = "Launch with a copy of an existing Chrome profile directory"
    )]
    pub from_profile: Option<String>,
    #[arg(
        long,
        global = true,
        env = "EOKA_NO_STEALTH",
        help = "Disable stealth CDP filtering and evasion script injection"
    )]
    pub no_stealth: bool,
    #[arg(
        long,
        global = true,
        value_name = "URL",
        env = "EOKA_PROXY",
        conflicts_with = "proxy_file",
        help = "Proxy URL for launched browser"
    )]
    pub proxy: Option<String>,
    #[arg(
        long,
        global = true,
        value_name = "FILE",
        env = "EOKA_PROXY_FILE",
        conflicts_with = "proxy",
        help = "Read proxy URL from a file"
    )]
    pub proxy_file: Option<PathBuf>,
    #[arg(
        long,
        global = true,
        env = "EOKA_NO_JS",
        help = "Start with JavaScript blocked by default"
    )]
    pub no_js: bool,
    #[arg(
        long,
        global = true,
        value_name = "DOMAIN",
        help = "Domain to always run JavaScript on"
    )]
    pub js_allow: Vec<String>,
    #[arg(
        long,
        global = true,
        value_name = "DOMAIN",
        help = "Domain to always block JavaScript on"
    )]
    pub js_block: Vec<String>,
    #[arg(long, hide = true)]
    pub daemon: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    #[command(about = "Navigate to URL")]
    Open {
        url: String,
        #[arg(long)]
        headers: Option<String>,
        #[arg(long)]
        user_agent: Option<String>,
        #[arg(long)]
        bypass_csp: bool,
        #[arg(long, value_name = "FILE")]
        inject_js: Option<String>,
        #[arg(long, value_name = "FILE")]
        load_state: Option<PathBuf>,
    },
    #[command(about = "Go back in history")]
    Back,
    #[command(about = "Go forward in history")]
    Forward,
    #[command(about = "Reload current page")]
    Reload,
    #[command(about = "Accessibility tree with @eN refs")]
    Snapshot {
        #[arg(short, long)]
        interactive: bool,
        #[arg(long)]
        all: bool,
    },
    #[command(about = "List interactive elements with indices")]
    Observe {
        #[arg(long, help = "Filter elements: inputs, buttons, or all")]
        filter: Option<String>,
        #[arg(long, help = "Maximum elements to return")]
        max: Option<usize>,
        #[arg(long, help = "Return structured element objects instead of text")]
        structured: bool,
    },
    #[command(about = "Take screenshot")]
    Screenshot {
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(long)]
        annotate: bool,
    },
    #[command(about = "Solve or inject CAPTCHA tokens")]
    Captcha {
        #[command(subcommand)]
        action: CaptchaAction,
    },
    #[command(about = "Emulate a device viewport")]
    Emulate {
        #[arg(long, default_value = "390")]
        width: u32,
        #[arg(long, default_value = "844")]
        height: u32,
        #[arg(long, default_value = "2")]
        dpr: f64,
        #[arg(long)]
        desktop: bool,
        #[arg(long)]
        reset: bool,
    },
    #[command(about = "Get page URL and title")]
    Info,
    #[command(about = "Get all visible text on page")]
    Text,
    #[command(about = "Find elements by text substring")]
    Find { text: String },
    #[command(about = "Click element")]
    Click { target: String },
    #[command(name = "dblclick", about = "Double-click element")]
    DoubleClick { target: String },
    #[command(about = "Clear and fill input")]
    Fill { target: String, text: String },
    #[command(about = "Select dropdown option")]
    Select { target: String, value: String },
    #[command(about = "Hover over element")]
    Hover { target: String },
    #[command(about = "Press keyboard key")]
    Key { key: String },
    #[command(about = "Scroll page or element into view")]
    Scroll { target: String },
    #[command(about = "Execute JavaScript and return result")]
    Eval {
        code: Option<String>,
        #[arg(short, long)]
        file: Option<PathBuf>,
        #[arg(long)]
        no_return: bool,
        #[arg(long)]
        max_size: Option<usize>,
        #[arg(long)]
        no_await: bool,
    },
    #[command(about = "Execute JavaScript without returning a value")]
    Exec {
        code: Option<String>,
        #[arg(short, long)]
        file: Option<PathBuf>,
        #[arg(long)]
        no_await: bool,
    },
    #[command(about = "Run Tack TypeScript against the active eoka browser session")]
    Tack {
        code: Option<String>,
        #[arg(short, long, help = "Read Tack TypeScript from a file")]
        file: Option<PathBuf>,
        #[arg(long, help = "Execution timeout in milliseconds")]
        timeout_ms: Option<u64>,
        #[arg(long, help = "Print raw Tack execution JSON")]
        raw_json: bool,
        #[arg(long, help = "Expose all non-lifecycle eoka tools to Tack")]
        all_tools: bool,
        #[arg(long = "capability", help = "Expose opt-in tools for a capability")]
        capabilities: Vec<String>,
    },
    #[command(about = "Inspect the generated eoka tool catalog")]
    Tools {
        #[command(subcommand)]
        action: ToolsAction,
    },
    #[command(about = "Fetch URL from browser context")]
    Fetch {
        url: String,
        #[arg(short, long)]
        method: Option<String>,
        #[arg(long)]
        headers: Option<String>,
        #[arg(short, long)]
        body: Option<String>,
        #[arg(long)]
        redirect: Option<String>,
        #[arg(long)]
        body_only: bool,
        #[arg(long)]
        max_body: Option<usize>,
    },

    #[command(about = "Network recording, inspection, export, and interception")]
    Network {
        #[command(subcommand)]
        action: NetworkAction,
    },
    #[command(about = "Get all cookies")]
    Cookies,
    #[command(about = "Set a cookie")]
    SetCookie {
        name: String,
        value: String,
        #[arg(long)]
        domain: Option<String>,
        #[arg(long)]
        path: Option<String>,
    },
    #[command(about = "Delete a cookie by name")]
    DeleteCookie {
        name: String,
        #[arg(long)]
        domain: Option<String>,
    },
    #[command(about = "Clear all cookies")]
    ClearCookies,
    #[command(about = "Get localStorage")]
    Storage {
        key: Option<String>,
        #[arg(long)]
        session_storage: bool,
    },
    #[command(about = "Set storage value")]
    SetStorage {
        key: String,
        value: String,
        #[arg(long)]
        session_storage: bool,
    },
    #[command(about = "Dump localStorage and sessionStorage")]
    DumpStorage,
    #[command(about = "Save cookies and storage to JSON")]
    SaveState { path: PathBuf },
    #[command(about = "Load cookies and storage from JSON")]
    LoadState {
        path: PathBuf,
        #[arg(long)]
        no_navigate: bool,
    },
    #[command(about = "Set persistent extra HTTP headers")]
    Headers { headers_json: String },
    #[command(about = "Read browser console output")]
    Console {
        #[arg(long)]
        clear: bool,
        #[arg(long)]
        level: Option<String>,
    },
    #[command(about = "Read JavaScript errors")]
    Errors {
        #[arg(long)]
        clear: bool,
    },
    #[command(about = "Tab management")]
    Tab {
        #[command(subcommand)]
        action: TabAction,
    },
    #[command(about = "Wait for time, text, URL, or load state")]
    Wait {
        ms: Option<u64>,
        #[arg(long)]
        text: Option<String>,
        #[arg(long)]
        url: Option<String>,
        #[arg(long)]
        load: Option<String>,
        #[arg(short, long)]
        timeout: Option<u64>,
    },
    #[command(about = "Execute multiple commands in sequence")]
    Batch {
        input: Option<String>,
        #[arg(short, long)]
        file: Option<PathBuf>,
        #[arg(long)]
        bail: bool,
    },
    #[command(about = "Inject fake camera from a video file")]
    FakeCamera {
        file: PathBuf,
        #[arg(long)]
        loop_video: bool,
    },
    #[command(about = "WASM linear memory operations")]
    Wasm {
        #[command(subcommand)]
        action: WasmAction,
    },
    #[command(about = "Manage the per-domain JavaScript policy")]
    Js {
        #[command(subcommand)]
        action: JsAction,
    },
    #[command(about = "Detect SPA router type")]
    SpaInfo,
    #[command(about = "Navigate SPA without page reload")]
    SpaNavigate { path: String },
    #[command(about = "List all sessions")]
    Sessions,
    #[command(about = "Show daemon status")]
    Status,
    #[command(about = "Diagnose local eoka runtime state")]
    Doctor,
    #[command(about = "Force-kill daemon")]
    Kill,
    #[command(about = "Close browser and daemon")]
    Close,
    #[command(
        name = "cdp-url",
        about = "Print the discovered DevTools WebSocket URL"
    )]
    CdpUrl {
        #[arg(long)]
        port: Option<u16>,
    },
    #[command(
        name = "clone-from",
        about = "Snapshot cookies and storage from a running Chrome"
    )]
    CloneFrom {
        source: String,
        #[arg(long)]
        to: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
pub enum NetworkAction {
    Record {
        #[command(subcommand)]
        action: NetworkRecordAction,
    },
    Log {
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long)]
        pattern: Option<String>,
        #[arg(long)]
        method: Option<String>,
        #[arg(long)]
        status: Option<u16>,
        #[arg(long)]
        since: Option<u64>,
        #[arg(long)]
        compact: bool,
    },
    Show {
        id: u64,
        #[arg(long)]
        body: bool,
        #[arg(long)]
        max_body: Option<usize>,
    },
    Har {
        path: PathBuf,
        #[arg(long)]
        settle_ms: Option<u64>,
    },
    Export {
        path: PathBuf,
        #[arg(long, default_value = "har")]
        format: String,
        #[arg(long)]
        settle_ms: Option<u64>,
    },
    Wait {
        #[arg(long)]
        pattern: Option<String>,
        #[arg(long)]
        method: Option<String>,
        #[arg(long)]
        status: Option<u16>,
        #[arg(long)]
        timeout: Option<u64>,
        #[arg(long)]
        since: Option<u64>,
        #[arg(long)]
        include_existing: bool,
    },
    Clear,
    Intercept {
        #[command(subcommand)]
        action: InterceptAction,
    },
}

#[derive(Subcommand)]
pub enum ToolsAction {
    Manifest {
        #[arg(long)]
        all: bool,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub enum NetworkRecordAction {
    Start {
        #[arg(long = "pattern")]
        patterns: Vec<String>,
        #[arg(long)]
        no_bodies: bool,
        #[arg(long, default_value = "10485760")]
        max_body_bytes: usize,
        #[arg(long)]
        clear: bool,
    },
    Stop,
    Status,
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
pub enum CaptchaAction {
    Solve(Box<SolveArgs>),
    Inject {
        token: String,
        #[arg(long, default_value = "auto")]
        captcha_type: String,
        #[arg(long)]
        callback: Option<String>,
        #[arg(long)]
        click_after: Option<String>,
    },
}

#[derive(Args)]
pub struct SolveArgs {
    #[arg(long)]
    pub captcha_type: String,
    #[arg(long)]
    pub website_url: String,
    #[arg(long)]
    pub website_key: String,
    #[arg(long, env = "ANTI_CAPTCHA_KEY")]
    pub api_key: Option<String>,
    #[arg(long)]
    pub page_action: Option<String>,
    #[arg(long)]
    pub min_score: Option<f32>,
    #[arg(long)]
    pub enterprise_payload: Option<String>,
    #[arg(long)]
    pub api_domain: Option<String>,
    #[arg(long)]
    pub iv: Option<String>,
    #[arg(long)]
    pub context: Option<String>,
    #[arg(long)]
    pub captcha_script: Option<String>,
    #[arg(long)]
    pub challenge_script: Option<String>,
    #[arg(long)]
    pub inject: bool,
    #[arg(long)]
    pub inject_callback: Option<String>,
    #[arg(long)]
    pub click_after: Option<String>,
}

#[derive(Subcommand)]
pub enum WasmAction {
    Read {
        addr: String,
        len: usize,
        #[arg(long)]
        memory: Option<String>,
    },
    Write {
        addr: String,
        hex: String,
        #[arg(long)]
        memory: Option<String>,
    },
    Find {
        pattern: String,
        #[arg(long)]
        start: Option<String>,
        #[arg(long)]
        end: Option<String>,
        #[arg(long, default_value = "20")]
        max: usize,
        #[arg(long)]
        memory: Option<String>,
    },
    Info,
}

#[derive(Subcommand)]
pub enum InterceptAction {
    Add {
        url_pattern: String,
        #[arg(long)]
        capture: Option<PathBuf>,
        #[arg(long)]
        respond: Option<PathBuf>,
        #[arg(long, default_value = "200")]
        status: u16,
    },
    List,
    Remove {
        id: String,
    },
    Log {
        #[arg(long)]
        clear: bool,
    },
}

#[derive(Subcommand)]
pub enum JsAction {
    Mode { mode: String },
    Allow { domain: String },
    Block { domain: String },
    Remove { domain: String },
    List,
}

#[derive(Subcommand)]
pub enum TabAction {
    List,
    New { url: Option<String> },
    Switch { tab_id: String },
    Close { tab_id: String },
    Attach { tab_id: String },
}
#[cfg(test)]
pub(crate) mod test_support {
    use super::{Cli, Command};
    use clap::Parser;

    pub(crate) fn parsed_command(args: &[&str]) -> (Cli, Command) {
        let mut cli = Cli::try_parse_from(args).unwrap();
        let command = cli.command.take().unwrap();
        (cli, command)
    }
}
