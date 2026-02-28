use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rmcp::{
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::*,
    tool, tool_handler, tool_router, ServerHandler,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use eoka::{Browser, Page, StealthConfig, TabInfo};
use eoka_agent::{
    annotate, captcha, observe, spa, target, InteractiveElement, ObserveConfig, Target,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const ERR_NO_BROWSER: &str = "No browser open. Use navigate first.";
const ERR_NO_TAB: &str = "No tab open. Use navigate first.";

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

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
pub struct TargetRequest {
    #[schemars(
        description = "Target element. Supports: index (0), text:Submit, placeholder:Email, role:button, css:form button, id:my-btn, or plain text search"
    )]
    pub target: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FillRequest {
    #[schemars(
        description = "Target input. Supports: index (0), text:Email, placeholder:Enter code, css:input.search, id:email-field, or plain text search"
    )]
    pub target: String,
    #[schemars(description = "Text to type into the element")]
    pub text: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SelectRequest {
    #[schemars(description = "Element index (number) OR text to find")]
    pub target: String,
    #[schemars(description = "Option value or visible text to select")]
    pub value: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TypeKeyRequest {
    #[schemars(description = "Key to press (e.g. Enter, Tab, Escape, ArrowDown, Backspace)")]
    pub key: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ScrollRequest {
    #[schemars(
        description = "Direction: up, down, top, bottom, or element index/text to scroll into view"
    )]
    pub target: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FindTextRequest {
    #[schemars(description = "Text substring to search for (case-insensitive)")]
    pub text: String,
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
pub struct NewTabRequest {
    #[schemars(description = "Optional URL to navigate to. If omitted, opens about:blank.")]
    pub url: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TabIdRequest {
    #[schemars(description = "Tab ID (from list_tabs)")]
    pub tab_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SpaNavigateRequest {
    #[schemars(description = "Target path to navigate to (e.g. '/docs', '/about')")]
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct HistoryGoRequest {
    #[schemars(description = "History delta: -1 for back, 1 for forward, -2 for back twice, etc.")]
    pub delta: i32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ObserveRequest {
    #[schemars(
        description = "Filter: 'inputs' (form elements), 'buttons' (buttons/links), 'all' (default)"
    )]
    pub filter: Option<String>,
    #[schemars(description = "Maximum elements to return (default: unlimited)")]
    pub max: Option<usize>,
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
    #[schemars(description = "Captcha type: 'hcaptcha', 'recaptcha_v2', or 'recaptcha_v3'")]
    pub captcha_type: String,
    #[schemars(description = "Website/page URL")]
    pub website_url: String,
    #[schemars(description = "Site key for the captcha")]
    pub website_key: String,
    #[schemars(description = "Page action (for reCAPTCHA v3)")]
    pub page_action: Option<String>,
    #[schemars(description = "Minimum score (for reCAPTCHA v3, default 0.3)")]
    pub min_score: Option<f32>,
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

// ---------------------------------------------------------------------------
// Tab State
// ---------------------------------------------------------------------------

/// State for a single tab
struct TabState {
    page: Page,
    elements: Vec<InteractiveElement>,
}

impl TabState {
    fn new(page: Page) -> Self {
        Self {
            page,
            elements: Vec::new(),
        }
    }
}

/// Multi-tab browser state
struct BrowserState {
    browser: Browser,
    tabs: HashMap<String, TabState>,
    current_tab_id: Option<String>,
    config: ObserveConfig,
    /// Set to true when a transport error is detected; triggers relaunch on next call
    unhealthy: bool,
}

impl BrowserState {
    async fn new(headless: bool) -> eoka::Result<Self> {
        let patch_binary = std::env::var("EOKA_PATCH_BINARY")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);

        // Parse proxy: EOKA_PROXY_FILE (one per line, random) > EOKA_PROXY (single)
        // Format: host:port:username:password
        let proxy_line = if let Ok(path) = std::env::var("EOKA_PROXY_FILE") {
            match std::fs::read_to_string(&path) {
                Ok(contents) => {
                    let lines: Vec<&str> = contents
                        .lines()
                        .map(|l| l.trim())
                        .filter(|l| !l.is_empty() && !l.starts_with('#'))
                        .collect();
                    if lines.is_empty() {
                        eprintln!("[eoka-agent] proxy file is empty: {}", path);
                        None
                    } else {
                        use std::time::SystemTime;
                        let idx = SystemTime::now()
                            .duration_since(SystemTime::UNIX_EPOCH)
                            .map(|d| d.as_nanos() as usize % lines.len())
                            .unwrap_or(0);
                        Some(lines[idx].to_string())
                    }
                }
                Err(e) => {
                    eprintln!("[eoka-agent] failed to read proxy file {}: {}", path, e);
                    None
                }
            }
        } else {
            std::env::var("EOKA_PROXY").ok()
        };

        let (proxy, proxy_username, proxy_password) = if let Some(proxy_str) = proxy_line {
            let parts: Vec<&str> = proxy_str.splitn(4, ':').collect();
            if parts.len() == 4 {
                (
                    Some(format!("http://{}:{}", parts[0], parts[1])),
                    Some(parts[2].to_string()),
                    Some(parts[3].to_string()),
                )
            } else if parts.len() == 2 {
                (
                    Some(format!("http://{}:{}", parts[0], parts[1])),
                    None,
                    None,
                )
            } else {
                eprintln!("[eoka-agent] invalid proxy format, expected host:port:user:pass");
                (None, None, None)
            }
        } else {
            (None, None, None)
        };

        if let Some(ref p) = proxy {
            eprintln!("[eoka-agent] using proxy: {}", p);
        }

        // When using a proxy, increase CDP timeout (proxies are slower)
        let cdp_timeout = if proxy.is_some() { 90 } else { 30 };

        eprintln!(
            "[eoka-agent] launching browser (headless={}, cdp_timeout={}s, proxy={})",
            headless,
            cdp_timeout,
            proxy.is_some()
        );
        let config = StealthConfig {
            headless,
            patch_binary,
            proxy,
            proxy_username,
            proxy_password,
            cdp_timeout,
            ..Default::default()
        };
        let browser = Browser::launch_with_config(config).await?;
        Ok(Self {
            browser,
            tabs: HashMap::new(),
            current_tab_id: None,
            config: ObserveConfig::default(),
            unhealthy: false,
        })
    }

    /// Get or create the current tab, navigating to URL
    async fn ensure_tab(&mut self, url: &str) -> eoka::Result<&mut TabState> {
        let tab_id = if let Some(existing_id) = &self.current_tab_id {
            // Navigate current tab
            if let Some(tab) = self.tabs.get_mut(existing_id) {
                tab.elements.clear();
                tab.page.goto(url).await?;
            }
            existing_id.clone()
        } else {
            // Create first tab
            let page = self.browser.new_page(url).await?;
            let new_id = page.target_id().to_string();
            self.tabs.insert(new_id.clone(), TabState::new(page));
            self.current_tab_id = Some(new_id.clone());
            new_id
        };
        Ok(self.tabs.get_mut(&tab_id).unwrap())
    }

    /// Get current tab or error
    fn current_tab(&self) -> Option<&TabState> {
        self.current_tab_id
            .as_ref()
            .and_then(|id| self.tabs.get(id))
    }

    fn current_tab_mut(&mut self) -> Option<&mut TabState> {
        self.current_tab_id
            .as_ref()
            .and_then(|id| self.tabs.get_mut(id))
    }

    /// Create a new tab
    async fn new_tab(&mut self, url: Option<&str>) -> eoka::Result<(String, &mut TabState)> {
        let page = match url {
            Some(u) => self.browser.new_page(u).await?,
            None => self.browser.new_blank_page().await?,
        };
        let tab_id = page.target_id().to_string();
        self.tabs.insert(tab_id.clone(), TabState::new(page));
        self.browser.activate_tab(&tab_id).await?;
        self.current_tab_id = Some(tab_id.clone());
        Ok((
            tab_id,
            self.tabs
                .get_mut(self.current_tab_id.as_ref().unwrap())
                .unwrap(),
        ))
    }

    /// Switch to a tab by ID. If the tab is not yet tracked (e.g., a popup opened by
    /// window.open()), automatically attaches to it and adds it to tracked tabs.
    async fn switch_tab(&mut self, tab_id: &str) -> eoka::Result<()> {
        if !self.tabs.contains_key(tab_id) {
            // Try to attach to an unmanaged target (popup, new window, etc.)
            let page = self.browser.attach_page(tab_id).await?;
            self.tabs.insert(tab_id.to_string(), TabState::new(page));
        }
        self.browser.activate_tab(tab_id).await?;
        self.current_tab_id = Some(tab_id.to_string());
        Ok(())
    }

    /// Close a tab
    async fn close_tab(&mut self, tab_id: &str) -> eoka::Result<()> {
        if self.tabs.len() <= 1 {
            return Err(eoka::Error::CdpSimple("Cannot close the last tab".into()));
        }
        if !self.tabs.contains_key(tab_id) {
            return Err(eoka::Error::ElementNotFound(format!(
                "Tab {} not found",
                tab_id
            )));
        }

        self.browser.close_tab(tab_id).await?;
        self.tabs.remove(tab_id);

        // If we closed the current tab, switch to another
        if self.current_tab_id.as_deref() == Some(tab_id) {
            if let Some(new_id) = self.tabs.keys().next().cloned() {
                self.current_tab_id = Some(new_id.clone());
                self.browser.activate_tab(&new_id).await?;
            } else {
                self.current_tab_id = None;
            }
        }
        Ok(())
    }

    /// List all tabs
    async fn list_tabs(&self) -> eoka::Result<Vec<TabInfo>> {
        self.browser.tabs().await
    }

    /// Close browser
    async fn close(self) -> eoka::Result<()> {
        self.browser.close().await
    }
}

// ---------------------------------------------------------------------------
// Server
// ---------------------------------------------------------------------------

fn err(e: impl std::fmt::Display) -> ErrorData {
    let msg = e.to_string();
    if is_transport_error(&e) {
        eprintln!("[eoka-agent] transport error detected: {}", msg);
    }
    ErrorData::internal_error(msg, None::<Value>)
}

/// Check if an error indicates a broken connection that requires session reset
fn is_transport_error(e: &impl std::fmt::Display) -> bool {
    let msg = e.to_string().to_lowercase();
    msg.contains("websocket")
        || msg.contains("transport")
        || msg.contains("timed out")
        || msg.contains("connection")
        || msg.contains("broken pipe")
        || msg.contains("reset by peer")
}

/// Resolve JS code from a JsRequest: prefer `file` (read from disk), fall back to `js` inline.
fn resolve_js(req: &JsRequest) -> Result<String, ErrorData> {
    if let Some(path) = &req.file {
        std::fs::read_to_string(path).map_err(|e| {
            ErrorData::invalid_params(
                format!("Failed to read JS file '{}': {}", path, e),
                None::<Value>,
            )
        })
    } else if let Some(js) = &req.js {
        Ok(js.clone())
    } else {
        Err(ErrorData::invalid_params(
            "Either 'js' or 'file' must be provided",
            None::<Value>,
        ))
    }
}

fn text_ok(s: impl Into<String>) -> Result<CallToolResult, ErrorData> {
    Ok(CallToolResult::success(vec![Content::text(s.into())]))
}

/// Resolved target ready for action.
struct ResolvedTarget {
    selector: String,
    desc: String,
    bbox: target::BBox,
}

/// Resolve target to selector + bbox. Index uses cache, everything else is live.
async fn resolve_target(
    page: &Page,
    elements: &[InteractiveElement],
    target_str: &str,
) -> Result<ResolvedTarget, ErrorData> {
    match Target::parse(target_str) {
        Target::Index(idx) => {
            let el = elements.get(idx).ok_or_else(|| {
                ErrorData::invalid_params(
                    format!("Index {} out of range (have {})", idx, elements.len()),
                    None::<Value>,
                )
            })?;
            Ok(ResolvedTarget {
                selector: el.selector.clone(),
                desc: el.to_string(),
                bbox: target::BBox {
                    x: el.bbox.x,
                    y: el.bbox.y,
                    width: el.bbox.width,
                    height: el.bbox.height,
                },
            })
        }
        Target::Live(pattern) => {
            let r = target::resolve(page, &pattern).await.map_err(err)?;
            if !r.found {
                return Err(ErrorData::invalid_params(
                    r.error
                        .unwrap_or_else(|| format!("{} not found", target_str)),
                    None::<Value>,
                ));
            }
            Ok(ResolvedTarget {
                selector: r.selector,
                desc: format!("<{}> \"{}\"", r.tag, r.text),
                bbox: r.bbox,
            })
        }
    }
}

/// Get page title without blocking on busy JS thread.
/// page.title() uses evaluate() with await_promise=true which hangs on heavy SPAs.
async fn title_nonblocking(page: &Page) -> String {
    page.evaluate_sync("document.title || ''")
        .await
        .unwrap_or_default()
}

/// Wait for page stability after navigation or action.
///
/// Uses CDP Page.getFrameTree (non-JS, never blocks on busy JS thread) to confirm
/// navigation committed, then evaluate_sync (await_promise=false) to check readyState.
/// This avoids the core hang: evaluate() with await_promise=true blocks on heavy SPAs
/// because Chrome queues the evaluation behind the page's synchronous JS execution
/// (React hydration, bundle eval, etc.) which can take 5-30+ seconds.
async fn wait_for_stable(page: &Page) -> eoka::Result<()> {
    // Phase 1: Wait for document.readyState to be at least "interactive"
    // using evaluate_sync (await_promise=false) so it returns immediately
    // even when the JS thread is busy with heavy synchronous work.
    let start = std::time::Instant::now();
    let max_wait = std::time::Duration::from_secs(10);

    loop {
        eprintln!(
            "[wait_for_stable] polling readyState (elapsed: {:?})...",
            start.elapsed()
        );
        let ready: String = page
            .evaluate_sync("document.readyState || 'loading'")
            .await
            .unwrap_or_else(|_| "loading".to_string());
        eprintln!("[wait_for_stable] readyState={}", &ready);

        if ready == "interactive" || ready == "complete" {
            break;
        }

        if start.elapsed() > max_wait {
            eprintln!("[wait_for_stable] timeout, breaking");
            break;
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    page.wait(100).await;
    eprintln!("[wait_for_stable] done");
    Ok(())
}

#[derive(Clone)]
pub struct EokaServer {
    state: Arc<Mutex<Option<BrowserState>>>,
    tool_router: ToolRouter<Self>,
    headless: bool,
}

impl EokaServer {
    async fn ensure_browser(&self) -> Result<(), ErrorData> {
        let mut guard = self.state.lock().await;
        // If browser is unhealthy (previous transport error), kill and relaunch
        if guard.as_ref().map(|s| s.unhealthy).unwrap_or(false) {
            eprintln!("[eoka-agent] browser unhealthy, relaunching...");
            if let Some(state) = guard.take() {
                let _ = state.close().await;
            }
        }
        if guard.is_none() {
            let state = BrowserState::new(self.headless).await.map_err(err)?;
            *guard = Some(state);
        }
        Ok(())
    }

    /// Check error and mark state unhealthy if it's a transport error.
    /// The unhealthy state will trigger a relaunch on the next ensure_browser() call.
    async fn check_transport_err<E: std::fmt::Display>(&self, e: E) -> ErrorData {
        let msg = e.to_string();
        if is_transport_error(&e) {
            eprintln!("[eoka-agent] connection lost, marking unhealthy: {}", msg);
            let mut guard = self.state.lock().await;
            if let Some(state) = guard.as_mut() {
                state.unhealthy = true;
            }
            ErrorData::internal_error(
                format!("{} (connection lost - will relaunch on next call)", msg),
                None::<Value>,
            )
        } else {
            err(e)
        }
    }
}

#[tool_router]
impl EokaServer {
    pub fn new() -> Self {
        let headless = std::env::var("EOKA_HEADLESS")
            .map(|v| v != "false" && v != "0")
            .unwrap_or(true);

        Self {
            state: Arc::new(Mutex::new(None)),
            tool_router: Self::tool_router(),
            headless,
        }
    }

    // =========================================================================
    // Tab Management
    // =========================================================================

    #[tool(description = "List all open browser tabs. Returns tab IDs, titles, and URLs.")]
    async fn list_tabs(&self) -> Result<CallToolResult, ErrorData> {
        self.ensure_browser().await?;
        let guard = self.state.lock().await;
        let state = guard.as_ref().unwrap();

        let tabs = state.list_tabs().await.map_err(err)?;
        let current_id = state.current_tab_id.as_deref();

        let mut out = String::new();
        for tab in tabs {
            let marker = if Some(tab.id.as_str()) == current_id {
                " *"
            } else {
                ""
            };
            out.push_str(&format!(
                "[{}]{} {}\n  {}\n",
                tab.id, marker, tab.title, tab.url
            ));
        }
        if out.is_empty() {
            out = "No tabs open.".into();
        }
        text_ok(out)
    }

    #[tool(description = "Open a new browser tab. Optionally navigate to URL. Returns new tab ID.")]
    async fn new_tab(&self, req: Parameters<NewTabRequest>) -> Result<CallToolResult, ErrorData> {
        self.ensure_browser().await?;
        let mut guard = self.state.lock().await;
        let state = guard.as_mut().unwrap();

        let (tab_id, tab) = state.new_tab(req.0.url.as_deref()).await.map_err(err)?;

        let url = tab.page.url().await.map_err(err)?;
        let title = title_nonblocking(&tab.page).await;
        text_ok(format!(
            "Opened new tab [{}]\nURL: {}\nTitle: {}",
            tab_id, url, title
        ))
    }

    #[tool(description = "Switch to a different browser tab by ID. Get IDs from list_tabs.")]
    async fn switch_tab(&self, req: Parameters<TabIdRequest>) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.state.lock().await;
        let state = guard.as_mut().ok_or_else(|| err(ERR_NO_BROWSER))?;

        state.switch_tab(&req.0.tab_id).await.map_err(err)?;

        let tab = state.current_tab().unwrap();
        let url = tab.page.url().await.map_err(err)?;
        let title = title_nonblocking(&tab.page).await;
        text_ok(format!(
            "Switched to tab [{}]\nURL: {}\nTitle: {}",
            req.0.tab_id, url, title
        ))
    }

    #[tool(description = "Close a browser tab by ID. Cannot close the last remaining tab.")]
    async fn close_tab(&self, req: Parameters<TabIdRequest>) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.state.lock().await;
        let state = guard.as_mut().ok_or_else(|| err(ERR_NO_BROWSER))?;

        state.close_tab(&req.0.tab_id).await.map_err(err)?;
        text_ok(format!("Closed tab [{}]", req.0.tab_id))
    }

    // =========================================================================
    // Navigation
    // =========================================================================

    #[tool(
        description = "Navigate to a URL. Launches browser on first call. Returns page title. \
        Optional: headers (e.g. CVE-2025-29927 bypass), user_agent override, bypass_csp."
    )]
    async fn navigate(
        &self,
        req: Parameters<NavigateRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        eprintln!("[navigate] start: {}", &req.0.url);
        self.ensure_browser().await?;
        eprintln!("[navigate] browser ready, acquiring lock...");
        let mut guard = self.state.lock().await;
        let state = guard.as_mut().unwrap();
        eprintln!("[navigate] lock acquired, calling goto...");

        let tab = match state.ensure_tab(&req.0.url).await {
            Ok(t) => t,
            Err(e) => {
                eprintln!("[navigate] goto FAILED: {}", e);
                drop(guard);
                return Err(self.check_transport_err(e).await);
            }
        };

        // Apply optional nav modifiers then navigate again with them active.
        let has_extras = req.0.headers.is_some()
            || req.0.user_agent.is_some()
            || req.0.bypass_csp.unwrap_or(false);

        if has_extras {
            if let Some(ua) = &req.0.user_agent {
                tab.page.set_user_agent(ua).await.map_err(err)?;
            }
            if req.0.bypass_csp.unwrap_or(false) {
                tab.page.set_bypass_csp(true).await.map_err(err)?;
            }
            if let Some(h) = req.0.headers.clone() {
                tab.page.set_extra_headers(h).await.map_err(err)?;
            }
            let nav_result = tab.page.goto(&req.0.url).await;
            let _ = tab.page.clear_extra_headers().await;
            nav_result.map_err(err)?;
        }

        eprintln!("[navigate] goto done, waiting for stable...");
        wait_for_stable(&tab.page).await.map_err(err)?;
        eprintln!("[navigate] stable, getting url...");
        let url = tab.page.url().await.map_err(err)?;
        eprintln!("[navigate] url={}, getting title...", &url);
        let title = title_nonblocking(&tab.page).await;
        eprintln!("[navigate] done: {} - {}", &url, &title);
        text_ok(format!("Navigated to: {}\nTitle: {}", url, title))
    }

    #[tool(
        description = "List interactive elements. Optional filter: 'inputs' (form elements), 'buttons' (clickables), 'all'. Optional max limit. Use live targeting (text:, css:) to skip observe."
    )]
    async fn observe(&self, req: Parameters<ObserveRequest>) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.state.lock().await;
        let state = guard.as_mut().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let viewport_only = state.config.viewport_only;
        let tab = state.current_tab_mut().ok_or_else(|| err(ERR_NO_TAB))?;

        tab.elements = match observe::observe(&tab.page, viewport_only).await {
            Ok(e) => e,
            Err(e) => {
                drop(guard);
                return Err(self.check_transport_err(e).await);
            }
        };

        // Apply filter
        let filtered: Vec<&InteractiveElement> = match req.0.filter.as_deref() {
            Some("inputs") => tab
                .elements
                .iter()
                .filter(|e| {
                    matches!(
                        e.tag.as_str(),
                        "input" | "select" | "textarea" | "contenteditable"
                    )
                })
                .collect(),
            Some("buttons") => tab
                .elements
                .iter()
                .filter(|e| {
                    matches!(e.tag.as_str(), "button" | "a") || e.role.as_deref() == Some("button")
                })
                .collect(),
            _ => tab.elements.iter().collect(),
        };

        // Apply max limit
        let limited: Vec<&InteractiveElement> = match req.0.max {
            Some(max) => filtered.into_iter().take(max).collect(),
            None => filtered,
        };

        let list: String = limited.iter().map(|e| format!("{}\n", e)).collect();
        text_ok(if list.is_empty() {
            "No interactive elements found.".into()
        } else {
            list
        })
    }

    #[tool(
        description = "Take annotated screenshot with numbered element boxes. Returns PNG image AND element list. Best way to see the page."
    )]
    async fn screenshot(&self) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.state.lock().await;
        let state = guard.as_mut().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let config_viewport_only = state.config.viewport_only;
        let tab = state.current_tab_mut().ok_or_else(|| err(ERR_NO_TAB))?;

        // Auto-observe if needed
        if tab.elements.is_empty() {
            tab.elements = observe::observe(&tab.page, config_viewport_only)
                .await
                .map_err(err)?;
        }

        let png = match annotate::annotated_screenshot(&tab.page, &tab.elements).await {
            Ok(p) => p,
            Err(e) => {
                drop(guard);
                return Err(self.check_transport_err(e).await);
            }
        };
        let list = element_list(&tab.elements);
        let b64 = BASE64.encode(&png);
        Ok(CallToolResult::success(vec![
            Content::image(b64, "image/png"),
            Content::text(if list.is_empty() {
                "No interactive elements found.".into()
            } else {
                list
            }),
        ]))
    }

    #[tool(
        description = "Click an element. Target: index (0), text:Submit, placeholder:Search, role:button, css:selector, id:my-btn, or plain text. Auto-retries once on stale element."
    )]
    async fn click(&self, req: Parameters<TargetRequest>) -> Result<CallToolResult, ErrorData> {
        self.ensure_browser().await?;
        let mut guard = self.state.lock().await;
        let state = guard.as_mut().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let config_viewport_only = state.config.viewport_only;
        let tab = state.current_tab_mut().ok_or_else(|| err(ERR_NO_TAB))?;

        // Only auto-observe for cached targets (index or plain text)
        let target = Target::parse(&req.0.target);
        if matches!(target, Target::Index(_)) && tab.elements.is_empty() {
            match observe::observe(&tab.page, config_viewport_only).await {
                Ok(e) => tab.elements = e,
                Err(e) => {
                    drop(guard);
                    return Err(self.check_transport_err(e).await);
                }
            }
        }

        let resolved = resolve_target(&tab.page, &tab.elements, &req.0.target).await?;

        // Try click with auto-retry on element not found
        match tab.page.click(&resolved.selector).await {
            Ok(_) => {}
            Err(e)
                if e.to_string().contains("not found") || e.to_string().contains("not visible") =>
            {
                match observe::observe(&tab.page, config_viewport_only).await {
                    Ok(e) => tab.elements = e,
                    Err(e) => {
                        drop(guard);
                        return Err(self.check_transport_err(e).await);
                    }
                }
                let resolved2 = resolve_target(&tab.page, &tab.elements, &req.0.target).await?;
                if let Err(e) = tab.page.click(&resolved2.selector).await {
                    drop(guard);
                    return Err(self.check_transport_err(e).await);
                }
            }
            Err(e) => {
                drop(guard);
                return Err(self.check_transport_err(e).await);
            }
        }

        let _ = wait_for_stable(&tab.page).await;
        tab.elements.clear();
        text_ok(format!("Clicked {}", resolved.desc))
    }

    #[tool(
        description = "Type text into an input. Target: index, text:Label, placeholder:Enter code, css:input, id:field. Auto-retries once on stale element. Clears existing text."
    )]
    async fn fill(&self, req: Parameters<FillRequest>) -> Result<CallToolResult, ErrorData> {
        self.ensure_browser().await?;
        let mut guard = self.state.lock().await;
        let state = guard.as_mut().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let config_viewport_only = state.config.viewport_only;
        let tab = state.current_tab_mut().ok_or_else(|| err(ERR_NO_TAB))?;

        let target = Target::parse(&req.0.target);
        if matches!(target, Target::Index(_)) && tab.elements.is_empty() {
            match observe::observe(&tab.page, config_viewport_only).await {
                Ok(e) => tab.elements = e,
                Err(e) => {
                    drop(guard);
                    return Err(self.check_transport_err(e).await);
                }
            }
        }

        let resolved = resolve_target(&tab.page, &tab.elements, &req.0.target).await?;

        // Try fill with auto-retry on element not found
        match tab.page.fill(&resolved.selector, &req.0.text).await {
            Ok(_) => {}
            Err(e)
                if e.to_string().contains("not found") || e.to_string().contains("not visible") =>
            {
                match observe::observe(&tab.page, config_viewport_only).await {
                    Ok(e) => tab.elements = e,
                    Err(e) => {
                        drop(guard);
                        return Err(self.check_transport_err(e).await);
                    }
                }
                let resolved2 = resolve_target(&tab.page, &tab.elements, &req.0.target).await?;
                if let Err(e) = tab.page.fill(&resolved2.selector, &req.0.text).await {
                    drop(guard);
                    return Err(self.check_transport_err(e).await);
                }
            }
            Err(e) => {
                drop(guard);
                return Err(self.check_transport_err(e).await);
            }
        }

        let _ = wait_for_stable(&tab.page).await;
        tab.elements.clear();
        text_ok(format!("Filled {} with \"{}\"", resolved.desc, req.0.text))
    }

    #[tool(
        description = "Select dropdown option. Target: index, text:Label, css:select, id:dropdown. Value matches option value or visible text."
    )]
    async fn select(&self, req: Parameters<SelectRequest>) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.state.lock().await;
        let state = guard.as_mut().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let config_viewport_only = state.config.viewport_only;
        let tab = state.current_tab_mut().ok_or_else(|| err(ERR_NO_TAB))?;

        let target = Target::parse(&req.0.target);
        if matches!(target, Target::Index(_)) && tab.elements.is_empty() {
            tab.elements = observe::observe(&tab.page, config_viewport_only)
                .await
                .map_err(err)?;
        }

        let resolved = resolve_target(&tab.page, &tab.elements, &req.0.target).await?;
        let arg = serde_json::json!({ "sel": resolved.selector, "val": req.0.value });
        let js = format!(
            r#"(() => {{
                const arg = {arg};
                const sel = document.querySelector(arg.sel);
                if (!sel) return false;
                const opt = Array.from(sel.options).find(o => o.value === arg.val || o.text === arg.val);
                if (!opt) return false;
                sel.value = opt.value;
                sel.dispatchEvent(new Event('change', {{ bubbles: true }}));
                return true;
            }})()"#,
            arg = serde_json::to_string(&arg).unwrap()
        );
        let selected: bool = tab.page.evaluate(&js).await.map_err(err)?;
        if !selected {
            return Err(ErrorData::invalid_params(
                format!("Option \"{}\" not found in {}", req.0.value, resolved.desc),
                None::<Value>,
            ));
        }
        wait_for_stable(&tab.page).await.map_err(err)?;
        tab.elements.clear();
        text_ok(format!("Selected \"{}\" in {}", req.0.value, resolved.desc))
    }

    #[tool(
        description = "Hover over element to trigger tooltips, menus, or hover states. Target: index, text:Label, css:selector, etc."
    )]
    async fn hover(&self, req: Parameters<TargetRequest>) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.state.lock().await;
        let state = guard.as_mut().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let config_viewport_only = state.config.viewport_only;
        let tab = state.current_tab_mut().ok_or_else(|| err(ERR_NO_TAB))?;

        let target = Target::parse(&req.0.target);
        if matches!(target, Target::Index(_)) && tab.elements.is_empty() {
            tab.elements = observe::observe(&tab.page, config_viewport_only)
                .await
                .map_err(err)?;
        }

        let resolved = resolve_target(&tab.page, &tab.elements, &req.0.target).await?;
        let cx = resolved.bbox.x + resolved.bbox.width / 2.0;
        let cy = resolved.bbox.y + resolved.bbox.height / 2.0;
        tab.page
            .session()
            .dispatch_mouse_event(eoka::cdp::MouseEventType::MouseMoved, cx, cy, None, None)
            .await
            .map_err(err)?;
        text_ok(format!("Hovered {}", resolved.desc))
    }

    #[tool(
        description = "Press keyboard key. Common: Enter, Tab, Escape, ArrowDown, ArrowUp, Backspace, Space."
    )]
    async fn type_key(&self, req: Parameters<TypeKeyRequest>) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard.as_ref().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let tab = state.current_tab().ok_or_else(|| err(ERR_NO_TAB))?;
        tab.page.human().press_key(&req.0.key).await.map_err(err)?;
        text_ok(format!("Pressed {}", req.0.key))
    }

    #[tool(
        description = "Execute multiple actions in sequence. Reduces round trips. Actions: click, fill, type_key. Uses live targeting."
    )]
    async fn batch(&self, req: Parameters<BatchRequest>) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.state.lock().await;
        let state = guard.as_mut().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let tab = state.current_tab_mut().ok_or_else(|| err(ERR_NO_TAB))?;

        let mut results = Vec::new();

        for (i, action) in req.0.actions.iter().enumerate() {
            let result = match action.action.as_str() {
                "click" => {
                    let target = action.target.as_ref().ok_or_else(|| {
                        ErrorData::invalid_params(
                            format!("Action {} (click): missing target", i),
                            None::<Value>,
                        )
                    })?;
                    let resolved = resolve_target(&tab.page, &tab.elements, target).await?;
                    tab.page.click(&resolved.selector).await.map_err(err)?;
                    format!("click {}", resolved.desc)
                }
                "fill" => {
                    let target = action.target.as_ref().ok_or_else(|| {
                        ErrorData::invalid_params(
                            format!("Action {} (fill): missing target", i),
                            None::<Value>,
                        )
                    })?;
                    let text = action.text.as_ref().ok_or_else(|| {
                        ErrorData::invalid_params(
                            format!("Action {} (fill): missing text", i),
                            None::<Value>,
                        )
                    })?;
                    let resolved = resolve_target(&tab.page, &tab.elements, target).await?;
                    tab.page.fill(&resolved.selector, text).await.map_err(err)?;
                    format!("fill {} with \"{}\"", resolved.desc, text)
                }
                "type_key" => {
                    let key = action.text.as_ref().ok_or_else(|| {
                        ErrorData::invalid_params(
                            format!("Action {} (type_key): missing text (key name)", i),
                            None::<Value>,
                        )
                    })?;
                    tab.page.human().press_key(key).await.map_err(err)?;
                    format!("press {}", key)
                }
                other => {
                    return Err(ErrorData::invalid_params(
                        format!("Action {} unknown action type: {}", i, other),
                        None::<Value>,
                    ));
                }
            };
            results.push(result);
        }

        wait_for_stable(&tab.page).await.map_err(err)?;
        tab.elements.clear();
        text_ok(format!(
            "Executed {} actions:\n{}",
            results.len(),
            results.join("\n")
        ))
    }

    #[tool(
        description = "Scroll page. Target: 'up', 'down', 'top', 'bottom', or element target (index, text:Label, css:selector) to scroll into view."
    )]
    async fn scroll(&self, req: Parameters<ScrollRequest>) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.state.lock().await;
        let state = guard.as_mut().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let config_viewport_only = state.config.viewport_only;
        let tab = state.current_tab_mut().ok_or_else(|| err(ERR_NO_TAB))?;

        match req.0.target.as_str() {
            "up" => tab
                .page
                .execute_sync("window.scrollBy(0, -window.innerHeight * 0.8)")
                .await
                .map_err(err)?,
            "down" => tab
                .page
                .execute_sync("window.scrollBy(0, window.innerHeight * 0.8)")
                .await
                .map_err(err)?,
            "top" => tab
                .page
                .execute_sync("window.scrollTo(0, 0)")
                .await
                .map_err(err)?,
            "bottom" => tab
                .page
                .execute_sync("window.scrollTo(0, document.body.scrollHeight)")
                .await
                .map_err(err)?,
            target_str => {
                let target = Target::parse(target_str);
                if matches!(target, Target::Index(_)) && tab.elements.is_empty() {
                    tab.elements = observe::observe(&tab.page, config_viewport_only)
                        .await
                        .map_err(err)?;
                }
                let resolved = resolve_target(&tab.page, &tab.elements, target_str).await?;
                let js = format!(
                    "document.querySelector({})?.scrollIntoView({{behavior:'smooth',block:'center'}})",
                    serde_json::to_string(&resolved.selector).unwrap()
                );
                tab.page.execute_sync(&js).await.map_err(err)?;
            }
        }
        text_ok(format!("Scrolled {}", req.0.target))
    }

    #[tool(
        description = "Find elements by text content (case-insensitive). Returns matching elements with indices."
    )]
    async fn find_text(
        &self,
        req: Parameters<FindTextRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.state.lock().await;
        let state = guard.as_mut().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let config_viewport_only = state.config.viewport_only;
        let tab = state.current_tab_mut().ok_or_else(|| err(ERR_NO_TAB))?;

        if tab.elements.is_empty() {
            tab.elements = observe::observe(&tab.page, config_viewport_only)
                .await
                .map_err(err)?;
        }

        let needle = req.0.text.to_lowercase();
        let matches: Vec<_> = tab
            .elements
            .iter()
            .filter(|e| {
                e.text.to_lowercase().contains(&needle)
                    || e.placeholder
                        .as_ref()
                        .map(|p| p.to_lowercase().contains(&needle))
                        .unwrap_or(false)
            })
            .collect();

        if matches.is_empty() {
            text_ok(format!("No elements found matching \"{}\"", req.0.text))
        } else {
            let out: String = matches.iter().map(|e| format!("{}\n", e)).collect();
            text_ok(out)
        }
    }

    #[tool(
        description = "Run JavaScript and return result. Supports multi-statement code; the last expression's value is returned as JSON. \
        Pass js= for inline code, or file= with an absolute path to load from disk (saves tokens for repeated/complex scripts)."
    )]
    async fn extract(&self, req: Parameters<JsRequest>) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard.as_ref().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let tab = state.current_tab().ok_or_else(|| err(ERR_NO_TAB))?;
        let code = resolve_js(&req.0)?;
        // Use eval() to handle multi-statement code - returns value of last expression
        // Safely escape the JS code as a JSON string to prevent injection
        let escaped_js = serde_json::to_string(&code).map_err(err)?;
        let js = format!("JSON.stringify(eval({}))", escaped_js);
        // Use evaluate_sync (awaitPromise=false) so heavy React SPAs don't block
        // the JS thread waiting for microtask queue to drain (hydration, analytics, etc.)
        let json_str: String = tab.page.evaluate_sync(&js).await.map_err(err)?;
        text_ok(json_str)
    }

    #[tool(
        description = "Execute JavaScript without expecting a return value. Use for side effects like clicking elements via JS. \
        Pass js= for inline code, or file= with an absolute path to load from disk (saves tokens for repeated/complex scripts)."
    )]
    async fn exec(&self, req: Parameters<JsRequest>) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard.as_ref().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let tab = state.current_tab().ok_or_else(|| err(ERR_NO_TAB))?;
        let code = resolve_js(&req.0)?;
        // Use evaluate_sync to avoid blocking on heavy SPAs
        let _: String = tab.page.evaluate_sync(&code).await.unwrap_or_default();
        text_ok("Executed successfully")
    }

    #[tool(
        description = "Make an HTTP request from inside the browser session. Uses the browser's real cookies, TLS fingerprint, and Cloudflare clearance — bypasses bot detection that blocks curl. \
        Returns status, headers, and body. Use instead of Bash(curl) for any target protected by Cloudflare or requiring authentication. \
        Set redirect='manual' to capture redirect URLs without following (useful for OAuth redirect_uri testing)."
    )]
    async fn fetch(&self, req: Parameters<FetchRequest>) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard.as_ref().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let tab = state.current_tab().ok_or_else(|| err(ERR_NO_TAB))?;

        let method = serde_json::to_string(req.0.method.as_deref().unwrap_or("GET")).unwrap();
        let redirect =
            serde_json::to_string(req.0.redirect.as_deref().unwrap_or("follow")).unwrap();
        let max_body = req.0.max_body.unwrap_or(8192);
        let url = serde_json::to_string(&req.0.url).unwrap();
        let headers = match &req.0.headers {
            Some(h) => serde_json::to_string(h).unwrap_or_else(|_| "null".into()),
            None => "null".into(),
        };
        let body = match &req.0.body {
            Some(b) => serde_json::to_string(b).unwrap_or_else(|_| "null".into()),
            None => "null".into(),
        };

        let js = format!(
            r#"(async () => {{
                try {{
                    const opts = {{ method: {method}, credentials: 'include', redirect: {redirect} }};
                    const h = {headers};
                    if (h) opts.headers = h;
                    const b = {body};
                    if (b !== null) opts.body = b;
                    const r = await fetch({url}, opts);
                    const rh = {{}};
                    r.headers.forEach((v, k) => {{ rh[k] = v; }});
                    const text = await r.text();
                    return JSON.stringify({{
                        status: r.status,
                        statusText: r.statusText,
                        url: r.url,
                        redirected: r.redirected,
                        type: r.type,
                        headers: rh,
                        body: text.slice(0, {max_body}),
                        truncated: text.length > {max_body}
                    }});
                }} catch(e) {{
                    return JSON.stringify({{ error: String(e) }});
                }}
            }})()"#,
        );

        // evaluate() uses awaitPromise=true — awaits the async IIFE directly
        let json_str: String = tab.page.evaluate(&js).await.map_err(err)?;

        // Parse and pretty-print for readability
        let val: serde_json::Value =
            serde_json::from_str(&json_str).unwrap_or(serde_json::Value::Null);
        let inner: serde_json::Value = if val.is_string() {
            serde_json::from_str(val.as_str().unwrap_or("{}")).unwrap_or(serde_json::Value::Null)
        } else {
            val
        };

        if let Some(e) = inner.get("error").and_then(|v| v.as_str()) {
            return text_ok(format!("fetch error: {}", e));
        }

        let status = inner.get("status").and_then(|v| v.as_u64()).unwrap_or(0);
        let status_text = inner
            .get("statusText")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let final_url = inner
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or(&req.0.url);
        let redirected = inner
            .get("redirected")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let resp_type = inner.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let body_str = inner.get("body").and_then(|v| v.as_str()).unwrap_or("");
        let truncated = inner
            .get("truncated")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let mut out = format!("Status: {} {}\nURL: {}", status, status_text, final_url);
        if redirected {
            out.push_str(&format!("\nRedirected: true (original: {})", req.0.url));
        }
        if !resp_type.is_empty() && resp_type != "basic" {
            out.push_str(&format!("\nType: {}", resp_type));
        }

        if let Some(headers_obj) = inner.get("headers").and_then(|v| v.as_object()) {
            if !headers_obj.is_empty() {
                out.push_str("\nHeaders:");
                for (k, v) in headers_obj {
                    out.push_str(&format!("\n  {}: {}", k, v.as_str().unwrap_or("")));
                }
            }
        }

        if !body_str.is_empty() {
            out.push_str(&format!("\nBody:\n{}", body_str));
            if truncated {
                out.push_str(&format!("\n[truncated at {} bytes]", max_body));
            }
        } else if resp_type == "opaqueredirect" {
            out.push_str(
                "\nBody: (opaque redirect — server tried to redirect, body not accessible)",
            );
        }

        text_ok(out)
    }

    #[tool(
        description = "Get all visible text on the page. Useful for reading content without elements."
    )]
    async fn page_text(&self) -> Result<CallToolResult, ErrorData> {
        self.ensure_browser().await?;
        let guard = self.state.lock().await;
        let state = guard.as_ref().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let tab = state.current_tab().ok_or_else(|| err(ERR_NO_TAB))?;
        match tab.page.text().await {
            Ok(text) => text_ok(text),
            Err(e) => {
                drop(guard);
                Err(self.check_transport_err(e).await)
            }
        }
    }

    #[tool(description = "Get current URL and page title.")]
    async fn page_info(&self) -> Result<CallToolResult, ErrorData> {
        self.ensure_browser().await?;
        let guard = self.state.lock().await;
        let state = guard.as_ref().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let tab = state.current_tab().ok_or_else(|| err(ERR_NO_TAB))?;
        match tab.page.url().await {
            Ok(url) => {
                let title = title_nonblocking(&tab.page).await;
                text_ok(format!("URL: {}\nTitle: {}", url, title))
            }
            Err(e) => {
                drop(guard);
                Err(self.check_transport_err(e).await)
            }
        }
    }

    #[tool(description = "Go back in browser history.")]
    async fn back(&self) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.state.lock().await;
        let state = guard.as_mut().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let tab = state.current_tab_mut().ok_or_else(|| err(ERR_NO_TAB))?;
        tab.elements.clear();
        tab.page.back().await.map_err(err)?;
        wait_for_stable(&tab.page).await.map_err(err)?;
        let url = tab.page.url().await.map_err(err)?;
        text_ok(format!("Navigated back to: {}", url))
    }

    #[tool(description = "Go forward in browser history.")]
    async fn forward(&self) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.state.lock().await;
        let state = guard.as_mut().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let tab = state.current_tab_mut().ok_or_else(|| err(ERR_NO_TAB))?;
        tab.elements.clear();
        tab.page.forward().await.map_err(err)?;
        wait_for_stable(&tab.page).await.map_err(err)?;
        let url = tab.page.url().await.map_err(err)?;
        text_ok(format!("Navigated forward to: {}", url))
    }

    // =========================================================================
    // SPA Navigation
    // =========================================================================

    #[tool(
        description = "Detect SPA router type and current route state. Returns router type (React Router, Next.js, Vue Router, etc.), current path, query params, and whether programmatic navigation is available."
    )]
    async fn spa_info(&self) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard.as_ref().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let tab = state.current_tab().ok_or_else(|| err(ERR_NO_TAB))?;

        let info = spa::detect_router(&tab.page).await.map_err(err)?;
        text_ok(info.to_string())
    }

    #[tool(
        description = "Navigate SPA to a new path without page reload. Uses the detected router (React Router, Next.js, Vue Router, etc.) or falls back to History API. Much faster than full page navigation for SPAs."
    )]
    async fn spa_navigate(
        &self,
        req: Parameters<SpaNavigateRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.state.lock().await;
        let state = guard.as_mut().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let tab = state.current_tab_mut().ok_or_else(|| err(ERR_NO_TAB))?;

        let info = spa::detect_router(&tab.page).await.map_err(err)?;
        let new_path = spa::spa_navigate(&tab.page, &info.router_type, &req.0.path)
            .await
            .map_err(err)?;

        tab.elements.clear(); // DOM will change
        text_ok(format!(
            "Navigated to {} via {} (no page reload)",
            new_path, info.router_type
        ))
    }

    #[tool(
        description = "Navigate browser history by delta steps. Use delta=-1 for back, delta=1 for forward, delta=-2 for back twice, etc. Works with both SPAs and regular pages."
    )]
    async fn history_go(
        &self,
        req: Parameters<HistoryGoRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.state.lock().await;
        let state = guard.as_mut().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let tab = state.current_tab_mut().ok_or_else(|| err(ERR_NO_TAB))?;

        spa::history_go(&tab.page, req.0.delta).await.map_err(err)?;
        tab.elements.clear(); // DOM will change

        let url = tab.page.url().await.map_err(err)?;
        let direction = if req.0.delta < 0 { "back" } else { "forward" };
        let steps = req.0.delta.abs();
        text_ok(format!(
            "Navigated {} {} step(s) to: {}",
            direction, steps, url
        ))
    }

    #[tool(description = "Get all cookies for the current page. Returns JSON array of cookies.")]
    async fn cookies(&self) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard.as_ref().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let tab = state.current_tab().ok_or_else(|| err(ERR_NO_TAB))?;
        let cookies = tab.page.cookies().await.map_err(err)?;
        let json = serde_json::to_string_pretty(&cookies).map_err(err)?;
        text_ok(json)
    }

    #[tool(description = "Set a cookie. Useful for restoring sessions or authentication.")]
    async fn set_cookie(
        &self,
        req: Parameters<SetCookieRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard.as_ref().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let tab = state.current_tab().ok_or_else(|| err(ERR_NO_TAB))?;
        tab.page
            .set_cookie(
                &req.0.name,
                &req.0.value,
                req.0.domain.as_deref(),
                req.0.path.as_deref(),
            )
            .await
            .map_err(err)?;
        text_ok(format!("Cookie '{}' set", req.0.name))
    }

    #[tool(
        description = "Detect and solve CAPTCHAs (hCaptcha, reCAPTCHA) using anti-captcha.com API"
    )]
    async fn solve_captcha(
        &self,
        req: Parameters<SolveCaptchaRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let solver = captcha::AntiCaptcha::new(req.0.api_key);

        let solution = match req.0.captcha_type.to_lowercase().as_str() {
            "hcaptcha" => {
                solver
                    .solve_hcaptcha(&req.0.website_url, &req.0.website_key)
                    .await
            }
            "recaptcha_v2" => {
                solver
                    .solve_recaptcha_v2(&req.0.website_url, &req.0.website_key)
                    .await
            }
            "recaptcha_v3" => {
                let page_action = req.0.page_action.unwrap_or_else(|| "submit".to_string());
                let min_score = req.0.min_score.unwrap_or(0.3);
                solver
                    .solve_recaptcha_v3(
                        &req.0.website_url,
                        &req.0.website_key,
                        &page_action,
                        min_score,
                    )
                    .await
            }
            _ => {
                return Err(err(format!(
                    "Unknown captcha type: {}. Use 'hcaptcha', 'recaptcha_v2', or 'recaptcha_v3'",
                    req.0.captcha_type
                )))
            }
        };

        match solution {
            Ok(token) => text_ok(format!(
                "Captcha solved! Token: {}...",
                &token[..token.len().min(50)]
            )),
            Err(e) => Err(err(format!("Failed to solve captcha: {}", e))),
        }
    }

    #[tool(
        description = "Detect hCaptcha or reCAPTCHA on the current page. Returns captcha type and sitekey."
    )]
    async fn detect_captcha(
        &self,
        req: Parameters<DetectCaptchaRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard.as_ref().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let tab = state.current_tab().ok_or_else(|| err(ERR_NO_TAB))?;

        if req.0.auto_detect.unwrap_or(true) {
            if let Some(info) = captcha::AntiCaptcha::detect_captcha_on_page(&tab.page).await {
                text_ok(format!(
                    "Captcha detected!\nType: {}\nSitekey: {}",
                    info.captcha_type, info.sitekey
                ))
            } else {
                text_ok("No captcha detected on current page".to_string())
            }
        } else {
            text_ok("Captcha detection disabled".to_string())
        }
    }

    #[tool(description = "Inject solved captcha token into page (for hCaptcha or reCAPTCHA v2)")]
    async fn inject_captcha_token(
        &self,
        req: Parameters<JsRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard.as_ref().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let tab = state.current_tab().ok_or_else(|| err(ERR_NO_TAB))?;

        let code = resolve_js(&req.0)?;
        // Execute the provided injection script
        tab.page.execute_sync(&code).await.map_err(err)?;

        text_ok("Captcha token injected")
    }

    #[tool(
        description = "Wait for a specified number of milliseconds. Useful for timing-sensitive operations where you need a pause between actions."
    )]
    async fn wait_ms(&self, req: Parameters<WaitMsRequest>) -> Result<CallToolResult, ErrorData> {
        let ms = req.0.ms.min(30_000);
        tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
        text_ok(format!("Waited {}ms", ms))
    }

    #[tool(
        description = "Click an element and intercept any navigation it triggers (location.assign/replace/href/pushState) WITHOUT actually navigating away. \
        Returns the intercepted URL so you can inspect what the page would have navigated to. \
        Use this when a click causes the page to redirect before you can read data from it. \
        After this call, the page stays on the current URL — use extract() to read window variables. \
        Set allow_nav=true if you want to let the navigation proceed after capturing the URL."
    )]
    async fn click_intercept_nav(
        &self,
        req: Parameters<ClickInterceptNavRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        self.ensure_browser().await?;
        let mut guard = self.state.lock().await;
        let state = guard.as_mut().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let config_viewport_only = state.config.viewport_only;
        let tab = state.current_tab_mut().ok_or_else(|| err(ERR_NO_TAB))?;

        let allow_nav = req.0.allow_nav.unwrap_or(false);
        let wait_ms = req.0.wait_ms.unwrap_or(3000).min(15_000);

        // Auto-observe for index targets
        let target = Target::parse(&req.0.target);
        if matches!(target, Target::Index(_)) && tab.elements.is_empty() {
            match observe::observe(&tab.page, config_viewport_only).await {
                Ok(e) => tab.elements = e,
                Err(e) => {
                    drop(guard);
                    return Err(self.check_transport_err(e).await);
                }
            }
        }

        let resolved = resolve_target(&tab.page, &tab.elements, &req.0.target).await?;

        // Inject navigation interceptor before clicking
        let allow_nav_js = if allow_nav { "true" } else { "false" };
        let intercept_js = format!(
            r#"
            (function() {{
                if (window.__eoka_nav_installed) return;
                window.__eoka_nav_installed = true;
                window.__eoka_nav = null;
                const _allow = {allow_nav};

                const _origAssign = location.assign.bind(location);
                const _origReplace = location.replace.bind(location);

                Object.defineProperty(window.location, 'href', {{
                    set: function(url) {{
                        window.__eoka_nav = {{ method: 'location.href', url: url }};
                        if (_allow) {{ _origAssign(url); }}
                    }},
                    configurable: true
                }});

                location.assign = function(url) {{
                    window.__eoka_nav = {{ method: 'location.assign', url: url }};
                    if (_allow) {{ _origAssign(url); }}
                }};

                location.replace = function(url) {{
                    window.__eoka_nav = {{ method: 'location.replace', url: url }};
                    if (_allow) {{ _origReplace(url); }}
                }};

                const _origPushState = history.pushState.bind(history);
                history.pushState = function(state, title, url) {{
                    window.__eoka_nav = {{ method: 'history.pushState', url: url, state: JSON.stringify(state) }};
                    if (_allow) {{ _origPushState(state, title, url); }}
                }};

                const _origReplaceState = history.replaceState.bind(history);
                history.replaceState = function(state, title, url) {{
                    window.__eoka_nav = {{ method: 'history.replaceState', url: url, state: JSON.stringify(state) }};
                    if (_allow) {{ _origReplaceState(state, title, url); }}
                }};
            }})();
            "#,
            allow_nav = allow_nav_js
        );

        tab.page.execute_sync(&intercept_js).await.map_err(err)?;

        // Click the element
        match tab.page.click(&resolved.selector).await {
            Ok(_) => {}
            Err(e)
                if e.to_string().contains("not found") || e.to_string().contains("not visible") =>
            {
                match observe::observe(&tab.page, config_viewport_only).await {
                    Ok(e) => tab.elements = e,
                    Err(e) => {
                        drop(guard);
                        return Err(self.check_transport_err(e).await);
                    }
                }
                let resolved2 = resolve_target(&tab.page, &tab.elements, &req.0.target).await?;
                if let Err(e) = tab.page.click(&resolved2.selector).await {
                    drop(guard);
                    return Err(self.check_transport_err(e).await);
                }
            }
            Err(e) => {
                drop(guard);
                return Err(self.check_transport_err(e).await);
            }
        }

        // Wait for the navigation intercept to fire
        tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;

        // Read the intercepted navigation
        let escaped =
            serde_json::to_string("window.__eoka_nav ? JSON.stringify(window.__eoka_nav) : null")
                .unwrap();
        let json_str: String = tab
            .page
            .evaluate_sync(&format!("JSON.stringify(eval({}))", escaped))
            .await
            .unwrap_or_else(|_| "null".to_string());

        tab.elements.clear();

        // Clean up interceptor flag so it can be re-installed next time
        let _ = tab
            .page
            .execute_sync("delete window.__eoka_nav_installed; delete window.__eoka_nav;")
            .await;

        let nav_info: serde_json::Value =
            serde_json::from_str(&json_str).unwrap_or(serde_json::Value::Null);
        let inner: serde_json::Value = if nav_info.is_string() {
            serde_json::from_str(nav_info.as_str().unwrap_or("null"))
                .unwrap_or(serde_json::Value::Null)
        } else {
            nav_info
        };

        if inner.is_null() {
            text_ok(format!(
                "Clicked {} — no navigation intercepted after {}ms\n\
                 (navigation may have used a method not covered, or page didn't navigate)",
                resolved.desc, wait_ms
            ))
        } else {
            let method = inner["method"].as_str().unwrap_or("unknown");
            let url = inner["url"].as_str().unwrap_or("(no url)");
            let state_info = if let Some(s) = inner["state"].as_str() {
                format!("\nstate: {}", s)
            } else {
                String::new()
            };
            text_ok(format!(
                "Clicked {} — navigation INTERCEPTED\nmethod: {}\nurl: {}{}",
                resolved.desc, method, url, state_info
            ))
        }
    }

    // =========================================================================
    // Cookies (extended)
    // =========================================================================

    #[tool(description = "Delete a specific cookie by name.")]
    async fn delete_cookie(
        &self,
        req: Parameters<DeleteCookieRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard.as_ref().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let tab = state.current_tab().ok_or_else(|| err(ERR_NO_TAB))?;
        tab.page
            .delete_cookie(&req.0.name, req.0.domain.as_deref())
            .await
            .map_err(err)?;
        text_ok(format!("Cookie '{}' deleted", req.0.name))
    }

    #[tool(description = "Clear all cookies for the current browser context.")]
    async fn clear_cookies(&self) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard.as_ref().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let tab = state.current_tab().ok_or_else(|| err(ERR_NO_TAB))?;
        tab.page.clear_all_cookies().await.map_err(err)?;
        text_ok("All cookies cleared")
    }

    // =========================================================================
    // Dialogs
    // =========================================================================

    #[tool(
        description = "Accept a JavaScript alert/confirm/prompt dialog. Optionally provide text for prompt() dialogs."
    )]
    async fn accept_dialog(
        &self,
        req: Parameters<AcceptDialogRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard.as_ref().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let tab = state.current_tab().ok_or_else(|| err(ERR_NO_TAB))?;
        tab.page
            .accept_dialog(req.0.prompt_text.as_deref())
            .await
            .map_err(err)?;
        text_ok("Dialog accepted")
    }

    #[tool(description = "Dismiss (cancel) a JavaScript alert/confirm/prompt dialog.")]
    async fn dismiss_dialog(&self) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard.as_ref().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let tab = state.current_tab().ok_or_else(|| err(ERR_NO_TAB))?;
        tab.page.dismiss_dialog().await.map_err(err)?;
        text_ok("Dialog dismissed")
    }

    // =========================================================================
    // Waiting
    // =========================================================================

    #[tool(
        description = "Wait for text to appear on the page (case-insensitive). Errors on timeout."
    )]
    async fn wait_for_text(
        &self,
        req: Parameters<WaitForTextRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard.as_ref().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let tab = state.current_tab().ok_or_else(|| err(ERR_NO_TAB))?;
        let timeout_ms = req.0.timeout_ms.unwrap_or(10_000);
        tab.page
            .wait_for_text(&req.0.text, timeout_ms)
            .await
            .map_err(err)?;
        text_ok(format!("Text \"{}\" found", req.0.text))
    }

    #[tool(
        description = "Wait for network to go idle (no XHR/fetch for idle_ms). Useful after actions that trigger API calls before reading the result."
    )]
    async fn wait_for_network_idle(
        &self,
        req: Parameters<WaitNetworkIdleRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard.as_ref().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let tab = state.current_tab().ok_or_else(|| err(ERR_NO_TAB))?;
        let idle_ms = req.0.idle_ms.unwrap_or(500);
        let timeout_ms = req.0.timeout_ms.unwrap_or(10_000);
        tab.page
            .wait_for_network_idle(idle_ms, timeout_ms)
            .await
            .map_err(err)?;
        text_ok("Network idle")
    }

    // =========================================================================
    // Storage
    // =========================================================================

    #[tool(description = "Get a value from localStorage. Returns null message if key not found.")]
    async fn local_storage_get(
        &self,
        req: Parameters<StorageKeyRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard.as_ref().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let tab = state.current_tab().ok_or_else(|| err(ERR_NO_TAB))?;
        let value = tab.page.local_storage_get(&req.0.key).await.map_err(err)?;
        match value {
            Some(v) => text_ok(v),
            None => text_ok(format!("(key '{}' not in localStorage)", req.0.key)),
        }
    }

    #[tool(description = "Set a value in localStorage.")]
    async fn local_storage_set(
        &self,
        req: Parameters<StorageSetRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard.as_ref().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let tab = state.current_tab().ok_or_else(|| err(ERR_NO_TAB))?;
        tab.page
            .local_storage_set(&req.0.key, &req.0.value)
            .await
            .map_err(err)?;
        text_ok(format!("localStorage['{}'] set", req.0.key))
    }

    #[tool(description = "Get a value from sessionStorage. Returns null message if key not found.")]
    async fn session_storage_get(
        &self,
        req: Parameters<StorageKeyRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard.as_ref().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let tab = state.current_tab().ok_or_else(|| err(ERR_NO_TAB))?;
        let value = tab
            .page
            .session_storage_get(&req.0.key)
            .await
            .map_err(err)?;
        match value {
            Some(v) => text_ok(v),
            None => text_ok(format!("(key '{}' not in sessionStorage)", req.0.key)),
        }
    }

    #[tool(description = "Set a value in sessionStorage.")]
    async fn session_storage_set(
        &self,
        req: Parameters<StorageSetRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard.as_ref().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let tab = state.current_tab().ok_or_else(|| err(ERR_NO_TAB))?;
        tab.page
            .session_storage_set(&req.0.key, &req.0.value)
            .await
            .map_err(err)?;
        text_ok(format!("sessionStorage['{}'] set", req.0.key))
    }

    #[tool(
        description = "Dump all client-side storage as JSON: localStorage, sessionStorage, and cookies. Use to capture session state for later restore or analysis."
    )]
    async fn dump_storage(&self) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard.as_ref().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let tab = state.current_tab().ok_or_else(|| err(ERR_NO_TAB))?;
        let storage = tab.page.dump_storage().await.map_err(err)?;
        let json = serde_json::to_string_pretty(&storage).map_err(err)?;
        text_ok(json)
    }

    // =========================================================================
    // Headers, UA, CSP
    // =========================================================================

    #[tool(description = "Close the browser. Call when done to free resources.")]
    async fn close(&self) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.state.lock().await;
        if let Some(state) = guard.take() {
            state.close().await.map_err(err)?;
        }
        text_ok("Browser closed.")
    }
}

/// Generate element list string
fn element_list(elements: &[InteractiveElement]) -> String {
    let mut out = String::with_capacity(elements.len() * 40);
    for el in elements {
        out.push_str(&el.to_string());
        out.push('\n');
    }
    out
}

#[tool_handler]
impl ServerHandler for EokaServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::LATEST,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "eoka-tools".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                title: None,
                icons: None,
                website_url: None,
            },
            instructions: Some(
                "Browser automation.\n\n\
                 TARGETING: Index (0) uses cache. Everything else is LIVE (resolved at action time):\n\
                 Submit, text:Submit, placeholder:code, css:button, id:btn, role:button\n\n\
                 OBSERVE: filter='inputs'|'buttons', max=N\n\
                 BATCH: batch([{action:'fill',target:'placeholder:code',text:'X'},{action:'click',target:'Submit'}])\n\
                 AUTO-RETRY: click/fill retry once on stale\n\
                 SPA: spa_info, spa_navigate, history_go\n\
                 Tabs: list_tabs, new_tab, switch_tab (auto-attaches to popups), close_tab\n\
                 TIMING: wait_ms(ms) — pause between actions\n\
                 NAV INTERCEPT: click_intercept_nav — capture location.assign/replace/href/pushState without navigating away\n\
                 FETCH: fetch(url, method?, headers?, body?, redirect?, max_body?) — HTTP request from browser session, bypasses Cloudflare/bot detection\n\
                 COOKIES: cookies, set_cookie, delete_cookie, clear_cookies\n\
                 DIALOGS: accept_dialog, dismiss_dialog\n\
                 STORAGE: local_storage_get/set, session_storage_get/set, dump_storage\n\
                 NAVIGATE EXTRAS: navigate(url, headers?, user_agent?, bypass_csp?) — headers/UA/CSP override per navigation\n\
                 WAIT: wait_ms, wait_for_text, wait_for_network_idle"
                    .into(),
            ),
        }
    }
}

async fn cleanup_browser(state: &Arc<Mutex<Option<BrowserState>>>) {
    let mut guard = state.lock().await;
    if let Some(bs) = guard.take() {
        eprintln!("[eoka-agent] closing browser...");
        let _ = bs.close().await;
        eprintln!("[eoka-agent] browser closed.");
    }
}

pub async fn run_server() -> anyhow::Result<()> {
    use rmcp::ServiceExt;

    let server = EokaServer::new();
    let state_handle = Arc::clone(&server.state);

    // Spawn signal handler — close browser on SIGTERM/SIGINT (e.g. MCP client killed)
    let sig_state = Arc::clone(&state_handle);
    tokio::spawn(async move {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to register SIGTERM handler");
        let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
            .expect("failed to register SIGINT handler");
        tokio::select! {
            _ = sigterm.recv() => eprintln!("[eoka-agent] received SIGTERM"),
            _ = sigint.recv() => eprintln!("[eoka-agent] received SIGINT"),
        }
        cleanup_browser(&sig_state).await;
        std::process::exit(0);
    });

    let service = server.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;

    // MCP connection closed (stdin EOF or client disconnected) — clean up browser
    eprintln!("[eoka-agent] MCP connection closed, cleaning up.");
    cleanup_browser(&state_handle).await;
    Ok(())
}
