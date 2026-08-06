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
        #[arg(help = "URL to navigate the active tab to")]
        url: String,
        #[arg(long, help = "JSON object of extra headers for this navigation")]
        headers: Option<String>,
        #[arg(long, help = "User-Agent value for this navigation")]
        user_agent: Option<String>,
        #[arg(long, help = "Disable Content Security Policy for this navigation")]
        bypass_csp: bool,
        #[arg(
            long,
            value_name = "FILE",
            help = "JavaScript source or file to inject before page scripts run"
        )]
        inject_js: Option<String>,
        #[arg(
            long,
            value_name = "FILE",
            help = "Load cookies and storage from a state JSON file before navigation"
        )]
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
        #[arg(short, long, help = "Limit output to interactive elements")]
        interactive: bool,
        #[arg(long, help = "Include hidden and non-interactive accessibility nodes")]
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
        #[arg(short, long, help = "Write screenshot PNG to this path")]
        output: Option<PathBuf>,
        #[arg(long, help = "Overlay observed element indices on the screenshot")]
        annotate: bool,
    },
    #[command(about = "Solve or inject CAPTCHA tokens")]
    Captcha {
        #[command(subcommand)]
        action: CaptchaAction,
    },
    #[command(about = "Emulate a device viewport")]
    Emulate {
        #[arg(long, default_value = "390", help = "Viewport width in CSS pixels")]
        width: u32,
        #[arg(long, default_value = "844", help = "Viewport height in CSS pixels")]
        height: u32,
        #[arg(long, default_value = "2", help = "Device pixel ratio")]
        dpr: f64,
        #[arg(long, help = "Use desktop viewport behavior")]
        desktop: bool,
        #[arg(long, help = "Reset viewport emulation")]
        reset: bool,
    },
    #[command(about = "Get page URL and title")]
    Info,
    #[command(about = "Get all visible text on page")]
    Text,
    #[command(about = "Find elements by text substring")]
    Find {
        #[arg(help = "Text substring to search for")]
        text: String,
    },
    #[command(about = "Click element")]
    Click {
        #[arg(help = "Target by index, ref, selector, id, role, placeholder, or text")]
        target: String,
    },
    #[command(name = "dblclick", about = "Double-click element")]
    DoubleClick {
        #[arg(help = "Target by index, ref, selector, id, role, placeholder, or text")]
        target: String,
    },
    #[command(about = "Clear and fill input")]
    Fill {
        #[arg(help = "Input target by index, selector, id, placeholder, or text")]
        target: String,
        #[arg(help = "Text to type into the input")]
        text: String,
    },
    #[command(about = "Select dropdown option")]
    Select {
        #[arg(help = "Select element target by index, selector, id, role, or text")]
        target: String,
        #[arg(help = "Option value or visible text to select")]
        value: String,
    },
    #[command(about = "Hover over element")]
    Hover {
        #[arg(help = "Target by index, ref, selector, id, role, placeholder, or text")]
        target: String,
    },
    #[command(about = "Press keyboard key")]
    Key {
        #[arg(help = "Keyboard key such as Enter, Tab, Escape, or ArrowDown")]
        key: String,
    },
    #[command(about = "Scroll page or element into view")]
    Scroll {
        #[arg(help = "Target element to scroll into view")]
        target: String,
    },
    #[command(about = "Execute JavaScript and return result")]
    Eval {
        #[arg(help = "JavaScript code to evaluate")]
        code: Option<String>,
        #[arg(short, long, help = "Read JavaScript from a file")]
        file: Option<PathBuf>,
        #[arg(long, help = "Execute without returning the evaluated value")]
        no_return: bool,
        #[arg(long, help = "Maximum serialized result size in bytes")]
        max_size: Option<usize>,
        #[arg(long, help = "Do not await returned promises")]
        no_await: bool,
    },
    #[command(about = "Execute JavaScript without returning a value")]
    Exec {
        #[arg(help = "JavaScript code to execute")]
        code: Option<String>,
        #[arg(short, long, help = "Read JavaScript from a file")]
        file: Option<PathBuf>,
        #[arg(long, help = "Do not await returned promises")]
        no_await: bool,
    },
    #[command(about = "Run Tack TypeScript against the active eoka browser session")]
    Tack {
        #[arg(help = "Tack TypeScript code to execute")]
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
        #[arg(help = "URL to fetch from the browser context")]
        url: String,
        #[arg(short, long, help = "HTTP method to use")]
        method: Option<String>,
        #[arg(long, help = "JSON object of request headers")]
        headers: Option<String>,
        #[arg(short, long, help = "Request body")]
        body: Option<String>,
        #[arg(long, help = "Fetch redirect mode")]
        redirect: Option<String>,
        #[arg(long, help = "Print only the response body")]
        body_only: bool,
        #[arg(long, help = "Maximum response body bytes to return")]
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
        #[arg(help = "Cookie name")]
        name: String,
        #[arg(help = "Cookie value")]
        value: String,
        #[arg(long, help = "Cookie domain")]
        domain: Option<String>,
        #[arg(long, help = "Cookie path")]
        path: Option<String>,
    },
    #[command(about = "Delete a cookie by name")]
    DeleteCookie {
        #[arg(help = "Cookie name")]
        name: String,
        #[arg(long, help = "Cookie domain")]
        domain: Option<String>,
    },
    #[command(about = "Clear all cookies")]
    ClearCookies,
    #[command(about = "Get localStorage")]
    Storage {
        #[arg(help = "Storage key to read; omit to list all keys")]
        key: Option<String>,
        #[arg(long, help = "Use sessionStorage instead of localStorage")]
        session_storage: bool,
    },
    #[command(about = "Set storage value")]
    SetStorage {
        #[arg(help = "Storage key")]
        key: String,
        #[arg(help = "Storage value")]
        value: String,
        #[arg(long, help = "Use sessionStorage instead of localStorage")]
        session_storage: bool,
    },
    #[command(about = "Dump localStorage and sessionStorage")]
    DumpStorage,
    #[command(about = "Save cookies and storage to JSON")]
    SaveState {
        #[arg(help = "Path to write state JSON")]
        path: PathBuf,
    },
    #[command(about = "Load cookies and storage from JSON")]
    LoadState {
        #[arg(help = "Path to read state JSON")]
        path: PathBuf,
        #[arg(long, help = "Restore state without navigating to the saved URL")]
        no_navigate: bool,
    },
    #[command(about = "Set persistent extra HTTP headers")]
    Headers {
        #[arg(help = "JSON object of persistent extra HTTP headers")]
        headers_json: String,
    },
    #[command(about = "Read browser console output")]
    Console {
        #[arg(long, help = "Clear console entries after reading")]
        clear: bool,
        #[arg(long, help = "Filter by console level")]
        level: Option<String>,
    },
    #[command(about = "Read JavaScript errors")]
    Errors {
        #[arg(long, help = "Clear JavaScript errors after reading")]
        clear: bool,
    },
    #[command(about = "Tab management")]
    Tab {
        #[command(subcommand)]
        action: TabAction,
    },
    #[command(about = "Wait for time, text, URL, or load state")]
    Wait {
        #[arg(help = "Milliseconds to wait")]
        ms: Option<u64>,
        #[arg(long, help = "Wait until visible text contains this substring")]
        text: Option<String>,
        #[arg(long, help = "Wait until the URL contains this substring")]
        url: Option<String>,
        #[arg(long, help = "Wait for load state")]
        load: Option<String>,
        #[arg(short, long, help = "Maximum wait time in milliseconds")]
        timeout: Option<u64>,
    },
    #[command(about = "Execute multiple commands in sequence")]
    Batch {
        #[arg(help = "Batch JSON, or omit to read JSON from stdin")]
        input: Option<String>,
        #[arg(short, long, help = "Read batch JSON from a file")]
        file: Option<PathBuf>,
        #[arg(long, help = "Stop after the first failed step")]
        bail: bool,
    },
    #[command(about = "Inject fake camera from a video file")]
    FakeCamera {
        #[arg(help = "Video file path to use as the fake camera")]
        file: PathBuf,
        #[arg(long, help = "Loop the video file")]
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
    SpaNavigate {
        #[arg(help = "SPA route path")]
        path: String,
    },
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
        #[arg(long, help = "Chrome DevTools port to query")]
        port: Option<u16>,
    },
    #[command(
        name = "clone-from",
        about = "Snapshot cookies and storage from a running Chrome"
    )]
    CloneFrom {
        #[arg(help = "Source Chrome DevTools port or WebSocket URL")]
        source: String,
        #[arg(long, help = "Path to write cloned state JSON")]
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
        #[arg(long, help = "Maximum log entries to return")]
        limit: Option<usize>,
        #[arg(long, help = "Only include URLs matching this pattern")]
        pattern: Option<String>,
        #[arg(long, help = "Only include this HTTP method")]
        method: Option<String>,
        #[arg(long, help = "Only include this HTTP status code")]
        status: Option<u16>,
        #[arg(long, help = "Only include entries after this timestamp")]
        since: Option<u64>,
        #[arg(long, help = "Return compact log entries")]
        compact: bool,
    },
    Show {
        #[arg(help = "Network entry ID")]
        id: u64,
        #[arg(long, help = "Include response body")]
        body: bool,
        #[arg(long, help = "Maximum response body bytes to include")]
        max_body: Option<usize>,
    },
    Har {
        #[arg(help = "Path to write HAR JSON")]
        path: PathBuf,
        #[arg(long, help = "Milliseconds to wait for in-flight requests to settle")]
        settle_ms: Option<u64>,
    },
    Export {
        #[arg(help = "Path to write exported network data")]
        path: PathBuf,
        #[arg(long, default_value = "har", help = "Export format: har or json")]
        format: String,
        #[arg(long, help = "Milliseconds to wait for in-flight requests to settle")]
        settle_ms: Option<u64>,
    },
    Wait {
        #[arg(long, help = "URL pattern to wait for")]
        pattern: Option<String>,
        #[arg(long, help = "HTTP method to wait for")]
        method: Option<String>,
        #[arg(long, help = "HTTP status code to wait for")]
        status: Option<u16>,
        #[arg(long, help = "Maximum wait time in milliseconds")]
        timeout: Option<u64>,
        #[arg(long, help = "Only consider entries after this timestamp")]
        since: Option<u64>,
        #[arg(long, help = "Match existing entries before waiting for new ones")]
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
        #[arg(long, help = "Include opt-in tools")]
        all: bool,
        #[arg(long, help = "Print manifest as JSON")]
        json: bool,
    },
}

#[derive(Subcommand)]
pub enum NetworkRecordAction {
    Start {
        #[arg(long = "pattern", help = "URL pattern to record")]
        patterns: Vec<String>,
        #[arg(long, help = "Do not capture response bodies")]
        no_bodies: bool,
        #[arg(
            long,
            default_value = "10485760",
            help = "Maximum response body bytes to capture"
        )]
        max_body_bytes: usize,
        #[arg(long, help = "Clear existing entries before recording")]
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
        #[arg(help = "CAPTCHA token to inject")]
        token: String,
        #[arg(long, default_value = "auto", help = "CAPTCHA type to inject")]
        captcha_type: String,
        #[arg(long, help = "Callback function name to invoke with the token")]
        callback: Option<String>,
        #[arg(long, help = "Selector or target to click after injection")]
        click_after: Option<String>,
    },
}

#[derive(Args)]
pub struct SolveArgs {
    #[arg(long, help = "CAPTCHA task type")]
    pub captcha_type: String,
    #[arg(long, help = "Website URL where the CAPTCHA appears")]
    pub website_url: String,
    #[arg(long, help = "Website CAPTCHA key")]
    pub website_key: String,
    #[arg(long, env = "ANTI_CAPTCHA_KEY", help = "Anti-Captcha API key")]
    pub api_key: Option<String>,
    #[arg(long, help = "reCAPTCHA action name")]
    pub page_action: Option<String>,
    #[arg(long, help = "Minimum reCAPTCHA v3 score")]
    pub min_score: Option<f32>,
    #[arg(long, help = "Enterprise payload JSON")]
    pub enterprise_payload: Option<String>,
    #[arg(long, help = "CAPTCHA API domain")]
    pub api_domain: Option<String>,
    #[arg(long, help = "GeeTest initialization vector")]
    pub iv: Option<String>,
    #[arg(long, help = "CAPTCHA context JSON")]
    pub context: Option<String>,
    #[arg(long, help = "CAPTCHA script URL")]
    pub captcha_script: Option<String>,
    #[arg(long, help = "Challenge script URL")]
    pub challenge_script: Option<String>,
    #[arg(long, help = "Inject the solved token into the current page")]
    pub inject: bool,
    #[arg(long, help = "Callback function name to invoke after solving")]
    pub inject_callback: Option<String>,
    #[arg(long, help = "Selector or target to click after solving")]
    pub click_after: Option<String>,
}

#[derive(Subcommand)]
pub enum WasmAction {
    Read {
        #[arg(help = "Memory address to read")]
        addr: String,
        #[arg(help = "Number of bytes to read")]
        len: usize,
        #[arg(long, help = "WebAssembly.Memory global name")]
        memory: Option<String>,
    },
    Write {
        #[arg(help = "Memory address to write")]
        addr: String,
        #[arg(help = "Hex bytes to write")]
        hex: String,
        #[arg(long, help = "WebAssembly.Memory global name")]
        memory: Option<String>,
    },
    Find {
        #[arg(help = "Hex or text pattern to search for")]
        pattern: String,
        #[arg(long, help = "Start address")]
        start: Option<String>,
        #[arg(long, help = "End address")]
        end: Option<String>,
        #[arg(long, default_value = "20", help = "Maximum matches to return")]
        max: usize,
        #[arg(long, help = "WebAssembly.Memory global name")]
        memory: Option<String>,
    },
    Info,
}

#[derive(Subcommand)]
pub enum InterceptAction {
    Add {
        #[arg(help = "URL pattern to intercept")]
        url_pattern: String,
        #[arg(long, help = "Path to capture matching requests")]
        capture: Option<PathBuf>,
        #[arg(long, help = "Path to a response body to serve")]
        respond: Option<PathBuf>,
        #[arg(
            long,
            default_value = "200",
            help = "HTTP status for synthetic responses"
        )]
        status: u16,
    },
    List,
    Remove {
        #[arg(help = "Intercept rule ID")]
        id: String,
    },
    Log {
        #[arg(long, help = "Clear intercept log after reading")]
        clear: bool,
    },
}

#[derive(Subcommand)]
pub enum JsAction {
    Mode {
        #[arg(help = "JavaScript policy mode")]
        mode: String,
    },
    Allow {
        #[arg(help = "Domain to allow JavaScript on")]
        domain: String,
    },
    Block {
        #[arg(help = "Domain to block JavaScript on")]
        domain: String,
    },
    Remove {
        #[arg(help = "Domain policy entry to remove")]
        domain: String,
    },
    List,
}

#[derive(Subcommand)]
pub enum TabAction {
    List,
    New {
        #[arg(help = "Optional URL for the new tab")]
        url: Option<String>,
    },
    Switch {
        #[arg(help = "Tab ID from tab list")]
        tab_id: String,
    },
    Close {
        #[arg(help = "Tab ID from tab list")]
        tab_id: String,
    },
    Attach {
        #[arg(help = "Tab ID from tab list")]
        tab_id: String,
    },
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

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::{Command, CommandFactory};

    #[test]
    fn visible_cli_arguments_have_help() {
        let command = Cli::command();
        assert_command_args_have_help(&command, "eoka");
    }

    fn assert_command_args_have_help(command: &Command, path: &str) {
        for arg in command.get_arguments() {
            if arg.is_hide_set() {
                continue;
            }
            let has_help = arg
                .get_help()
                .map(|help| !help.to_string().trim().is_empty())
                .unwrap_or(false);
            assert!(
                has_help,
                "{path} argument '{}' has empty help",
                arg.get_id()
            );
        }
        for subcommand in command.get_subcommands() {
            let next = format!("{path} {}", subcommand.get_name());
            assert_command_args_have_help(subcommand, &next);
        }
    }
}
