use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "eoka", about = "Browser automation CLI", version)]
pub struct Cli {
    /// Session name for isolated browser instances
    #[arg(long, default_value = "default", global = true)]
    pub session: String,

    /// JSON output mode (for agents)
    #[arg(long, global = true)]
    pub json: bool,

    /// Run browser in headed mode (visible window)
    #[arg(long, global = true)]
    pub headed: bool,

    /// Connect to a Chrome already running with --remote-debugging-port=N.
    /// Accepts a port (e.g. `--cdp 9222`) or a full ws:// URL.
    /// Implies no stealth injection on attached tabs.
    #[arg(long, value_name = "PORT|URL", global = true, env = "EOKA_CDP")]
    pub cdp: Option<String>,

    /// Auto-discover a running Chrome on ports 9222-9229. Implies --cdp.
    #[arg(long, global = true, env = "EOKA_AUTO_CONNECT")]
    pub auto_connect: bool,

    /// Launch a fresh Chrome and pre-load cookies/storage from a running
    /// Chrome (port or ws:// URL) before running commands.
    #[arg(long, value_name = "PORT|URL", global = true)]
    pub clone_state_from: Option<String>,

    /// Launch with a copy of an existing Chrome profile directory.
    /// Pass `auto` to autodetect the default profile, or an absolute path.
    #[arg(
        long,
        value_name = "auto|PATH",
        global = true,
        env = "EOKA_FROM_PROFILE"
    )]
    pub from_profile: Option<String>,

    /// Disable the stealth CDP-command filter and skip evasion-script injection.
    /// Useful for debugging or when targeting cooperative pages. Default in
    /// connect/CDP mode; opt-in for launch mode.
    #[arg(long, global = true, env = "EOKA_NO_STEALTH")]
    pub no_stealth: bool,

    /// Internal: run as daemon (hidden)
    #[arg(long, hide = true)]
    pub daemon: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    // ── Navigation ──────────────────────────────────────────────────
    /// Navigate to URL
    #[command(alias = "goto", alias = "navigate")]
    Open {
        url: String,
        /// Extra HTTP headers as JSON (e.g. '{"Authorization": "Bearer ..."}')
        #[arg(long)]
        headers: Option<String>,
        /// Override User-Agent string
        #[arg(long)]
        user_agent: Option<String>,
        /// Disable CSP enforcement (useful for XSS testing)
        #[arg(long)]
        bypass_csp: bool,
        /// Inject JS file via addScriptToEvaluateOnNewDocument (runs before any page script)
        #[arg(long, value_name = "FILE")]
        inject_js: Option<String>,
        /// Restore cookies and seed storage before this navigation.
        #[arg(long, value_name = "FILE")]
        load_state: Option<PathBuf>,
    },

    /// Go back in history
    Back,

    /// Go forward in history
    Forward,

    /// Reload current page
    Reload,

    // ── Observation ─────────────────────────────────────────────────
    /// Accessibility tree with @eN refs (best for AI)
    Snapshot {
        /// Interactive elements only
        #[arg(short, long)]
        interactive: bool,
        /// Include all nodes (generic, presentation, etc.)
        #[arg(long)]
        all: bool,
    },

    /// List interactive elements with indices
    Observe {
        /// Filter: "inputs", "buttons", or "all"
        #[arg(long)]
        filter: Option<String>,
        /// Max elements to return
        #[arg(long)]
        max: Option<usize>,
    },

    /// Take screenshot
    Screenshot {
        /// Output file path (default: temp file)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Annotate with numbered element labels
        #[arg(long)]
        annotate: bool,
    },

    /// Solve or inject CAPTCHA tokens.
    Captcha {
        #[command(subcommand)]
        action: CaptchaAction,
    },

    /// Emulate a device viewport (CDP Emulation.setDeviceMetricsOverride).
    /// Defaults to an iPhone-class 390x844 @2x mobile viewport.
    #[command(alias = "viewport", alias = "device")]
    Emulate {
        /// Viewport width in CSS pixels
        #[arg(long, default_value = "390")]
        width: u32,
        /// Viewport height in CSS pixels
        #[arg(long, default_value = "844")]
        height: u32,
        /// Device pixel ratio (retina = 2 or 3)
        #[arg(long, default_value = "2")]
        dpr: f64,
        /// Emulate desktop instead of mobile (disables the mobile flag + touch)
        #[arg(long)]
        desktop: bool,
        /// Clear the device emulation override and return to the window size
        #[arg(long)]
        reset: bool,
    },

    /// Get page URL and title
    Info,

    /// Get all visible text on page
    Text,

    /// Find elements by text substring
    Find {
        /// Text to search for (case-insensitive)
        text: String,
    },

    // ── Actions ─────────────────────────────────────────────────────
    /// Click element
    Click {
        /// Target: [0], 0, index:0, @e1, text:Submit, css:#btn, id:login, role:button
        target: String,
    },

    /// Double-click element
    #[command(alias = "dblclick")]
    DoubleClick { target: String },

    /// Clear and fill input
    Fill {
        /// Target input element
        target: String,
        /// Text to type
        text: String,
    },

    /// Select dropdown option
    Select {
        /// Target select element
        target: String,
        /// Option value or text
        value: String,
    },

    /// Hover over element
    Hover { target: String },

    /// Press keyboard key (Enter, Tab, Escape, ArrowDown, etc.)
    #[command(alias = "press")]
    Key { key: String },

    /// Scroll page or element into view
    Scroll {
        /// Direction (up/down/top/bottom) or element target
        target: String,
    },

    // ── JavaScript ──────────────────────────────────────────────────
    /// Execute JavaScript and return result
    Eval {
        /// JS code (or --file path)
        code: Option<String>,
        /// Read JS from file
        #[arg(short, long)]
        file: Option<PathBuf>,
        /// Don't return the result (avoids CDP crashes on large values)
        #[arg(long)]
        no_return: bool,
        /// Truncate result to N chars (default: unlimited)
        #[arg(long)]
        max_size: Option<usize>,
    },

    /// Execute JavaScript (no return value)
    Exec {
        code: Option<String>,
        #[arg(short, long)]
        file: Option<PathBuf>,
    },

    // ── Network ─────────────────────────────────────────────────────
    /// Fetch URL from browser context (with cookies/TLS)
    Fetch {
        url: String,
        /// HTTP method (default: GET)
        #[arg(short, long)]
        method: Option<String>,
        /// Request headers as JSON
        #[arg(long)]
        headers: Option<String>,
        /// Request body
        #[arg(short, long)]
        body: Option<String>,
        /// Redirect handling: follow, manual, error
        #[arg(long)]
        redirect: Option<String>,
        /// Print only the response body. Alias: --raw
        #[arg(long, alias = "raw")]
        body_only: bool,
        /// Max response body chars (default: 8192, 0 for unlimited)
        #[arg(long)]
        max_body: Option<usize>,
    },

    // ── Cookies ─────────────────────────────────────────────────────
    /// Get all cookies
    Cookies,

    /// Set a cookie
    SetCookie {
        name: String,
        value: String,
        #[arg(long)]
        domain: Option<String>,
        #[arg(long)]
        path: Option<String>,
    },

    /// Delete a cookie by name
    DeleteCookie {
        name: String,
        #[arg(long)]
        domain: Option<String>,
    },

    /// Clear all cookies
    ClearCookies,

    // ── Storage ─────────────────────────────────────────────────────
    /// Get localStorage (all or by key)
    Storage {
        /// Key to get (omit for all)
        key: Option<String>,
        /// Use sessionStorage instead
        #[arg(long)]
        session_storage: bool,
    },

    /// Set storage value
    SetStorage {
        key: String,
        value: String,
        /// Use sessionStorage instead
        #[arg(long)]
        session_storage: bool,
    },

    /// Dump all localStorage and sessionStorage
    DumpStorage,

    // ── State persistence ───────────────────────────────────────────
    /// Save cookies + storage to JSON file
    SaveState { path: PathBuf },

    /// Load cookies + storage from JSON file
    LoadState {
        path: PathBuf,
        /// Skip navigating to saved URL after loading
        #[arg(long)]
        no_navigate: bool,
    },

    // ── Headers ─────────────────────────────────────────────────────
    /// Set persistent extra HTTP headers
    Headers {
        /// JSON object of headers (e.g. '{"Authorization": "Bearer ..."}')
        headers_json: String,
    },

    // ── Console / Errors ────────────────────────────────────────────
    /// Read browser console output
    Console {
        /// Clear buffer after reading
        #[arg(long)]
        clear: bool,
        /// Filter by level: log, warn, error, info, debug
        #[arg(long)]
        level: Option<String>,
    },

    /// Read JavaScript errors
    Errors {
        /// Clear buffer after reading
        #[arg(long)]
        clear: bool,
    },

    // ── Tabs ────────────────────────────────────────────────────────
    /// Tab management
    Tab {
        #[command(subcommand)]
        action: TabAction,
    },

    // ── Wait ────────────────────────────────────────────────────────
    /// Wait for time, text, or URL pattern
    Wait {
        /// Milliseconds to wait
        ms: Option<u64>,
        /// Wait for text to appear
        #[arg(long)]
        text: Option<String>,
        /// Wait for URL pattern
        #[arg(long)]
        url: Option<String>,
        /// Wait for network idle
        #[arg(long)]
        load: Option<String>,
        /// Timeout in ms (default: 10000)
        #[arg(short, long)]
        timeout: Option<u64>,
    },

    /// Execute multiple commands in sequence
    Batch {
        /// JSON batch. If omitted, commands are read from stdin.
        input: Option<String>,
        /// Read JSON batch from file instead of argv/stdin.
        #[arg(short, long)]
        file: Option<PathBuf>,
        /// Stop on first error
        #[arg(long)]
        bail: bool,
    },

    // ── Fake Camera ───────────────────────────────────────────────
    /// Inject fake camera (getUserMedia override) from video file
    FakeCamera {
        /// Path to video file (mp4/webm)
        file: PathBuf,
        /// Loop the video
        #[arg(long, alias = "loop")]
        loop_video: bool,
    },

    // ── WASM Memory ─────────────────────────────────────────────────
    /// WASM linear memory operations
    Wasm {
        #[command(subcommand)]
        action: WasmAction,
    },

    // ── Network Interception ────────────────────────────────────────
    /// Intercept and modify network requests (CDP Fetch domain)
    Intercept {
        #[command(subcommand)]
        action: InterceptAction,
    },

    // ── SPA ─────────────────────────────────────────────────────────
    /// Detect SPA router type
    SpaInfo,

    /// Navigate SPA without page reload
    SpaNavigate { path: String },

    // ── Session management ──────────────────────────────────────────
    /// Show daemon status
    Status,

    /// Force-kill daemon
    Kill,

    /// Close browser and daemon. In connect mode, only disconnects.
    Close,

    /// Print the discovered DevTools WebSocket URL for a running Chrome.
    /// Defaults to the port from --cdp / --auto-connect, else 9222.
    #[command(name = "cdp-url")]
    CdpUrl {
        /// Port to query (overrides --cdp)
        #[arg(long)]
        port: Option<u16>,
    },

    /// Snapshot cookies + storage from a running Chrome (without binding the
    /// daemon to it). Pass `--cdp PORT|URL` to choose the source.
    #[command(name = "clone-from")]
    CloneFrom {
        /// Port or ws:// URL of the running Chrome
        source: String,
        /// Write the snapshot to a JSON file (otherwise loaded into the daemon's session)
        #[arg(long)]
        to: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
pub enum CaptchaAction {
    /// Solve hCaptcha, reCAPTCHA, or an AWS WAF challenge with Anti-Captcha.
    Solve {
        /// hcaptcha, recaptcha_v2, recaptcha_v3, or amazon_waf
        #[arg(long)]
        captcha_type: String,
        #[arg(long)]
        website_url: String,
        #[arg(long)]
        website_key: String,
        /// Anti-Captcha key; defaults to ANTI_CAPTCHA_KEY.
        #[arg(long, env = "ANTI_CAPTCHA_KEY")]
        api_key: Option<String>,
        #[arg(long)]
        page_action: Option<String>,
        #[arg(long)]
        min_score: Option<f32>,
        /// Required for amazon_waf; obtain from window.gokuProps.
        #[arg(long)]
        iv: Option<String>,
        /// Required for amazon_waf; obtain from window.gokuProps.
        #[arg(long)]
        context: Option<String>,
        #[arg(long)]
        captcha_script: Option<String>,
        #[arg(long)]
        challenge_script: Option<String>,
        /// Inject the solved token into the current browser session.
        #[arg(long)]
        inject: bool,
        /// Optional JavaScript callback expression to call with the solved token.
        #[arg(long)]
        inject_callback: Option<String>,
        /// Target to click after injecting the solved token.
        #[arg(long)]
        click_after: Option<String>,
    },

    /// Inject an already-solved CAPTCHA token into the current browser session.
    Inject {
        token: String,
        /// auto, recaptcha, hcaptcha, or turnstile
        #[arg(long, default_value = "auto")]
        captcha_type: String,
        /// Optional JavaScript callback expression to call with the token.
        #[arg(long)]
        callback: Option<String>,
        /// Target to click after injecting the token.
        #[arg(long)]
        click_after: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum WasmAction {
    /// Read bytes from WASM linear memory
    Read {
        /// Memory address (hex with 0x prefix or decimal)
        addr: String,
        /// Number of bytes to read
        len: usize,
        /// JS path to WASM memory (e.g. "window.__ft.mem"). Auto-detects if omitted.
        #[arg(long)]
        memory: Option<String>,
    },
    /// Write bytes to WASM linear memory
    Write {
        /// Memory address (hex with 0x prefix or decimal)
        addr: String,
        /// Hex bytes to write (e.g. "deadbeef" or "ff d8 ff e0")
        hex: String,
        #[arg(long)]
        memory: Option<String>,
    },
    /// Search WASM memory for byte pattern
    Find {
        /// Hex pattern to search for (e.g. "ff d8 ff e0")
        pattern: String,
        /// Start address
        #[arg(long)]
        start: Option<String>,
        /// End address
        #[arg(long)]
        end: Option<String>,
        /// Max results (default: 20)
        #[arg(long, default_value = "20")]
        max: usize,
        #[arg(long)]
        memory: Option<String>,
    },
    /// List detected WASM memory instances
    Info,
}

#[derive(Subcommand)]
pub enum InterceptAction {
    /// Add an intercept rule for URL pattern
    Add {
        /// URL pattern to match (glob, e.g. "*/api/data*")
        url_pattern: String,
        /// Save request body to file
        #[arg(long)]
        capture: Option<PathBuf>,
        /// Respond with file contents instead of forwarding
        #[arg(long)]
        respond: Option<PathBuf>,
        /// Response status code (default: 200, used with --respond)
        #[arg(long, default_value = "200")]
        status: u16,
    },
    /// List active intercept rules
    List,
    /// Remove intercept rule by ID or "all"
    Remove {
        /// Rule ID number or "all"
        id: String,
    },
    /// Show intercepted request log
    Log {
        #[arg(long)]
        clear: bool,
    },
}

#[derive(Subcommand)]
pub enum TabAction {
    /// List all tabs
    List,
    /// Open new tab
    New { url: Option<String> },
    /// Switch to tab by ID
    Switch { tab_id: String },
    /// Close tab by ID
    Close { tab_id: String },
    /// Attach to an existing tab by ID without injecting any scripts.
    /// Useful in --cdp mode to drive a tab the user already has open.
    Attach { tab_id: String },
}
