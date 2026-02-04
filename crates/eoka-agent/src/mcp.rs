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
use eoka_agent::{annotate, observe, InteractiveElement, ObserveConfig};

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
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TargetRequest {
    #[schemars(
        description = "Element index (number) OR text to find (string). Examples: 0, \"Submit\", \"Sign in\""
    )]
    pub target: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FillRequest {
    #[schemars(
        description = "Element index (number) OR text/placeholder to find. Examples: 0, \"Email\", \"Search\""
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
    #[schemars(description = "JavaScript code to execute")]
    pub js: String,
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
}

impl BrowserState {
    async fn new(headless: bool) -> eoka::Result<Self> {
        let config = if headless {
            StealthConfig::default()
        } else {
            StealthConfig {
                headless: false,
                ..Default::default()
            }
        };
        let browser = Browser::launch_with_config(config).await?;
        Ok(Self {
            browser,
            tabs: HashMap::new(),
            current_tab_id: None,
            config: ObserveConfig::default(),
        })
    }

    /// Get or create the current tab, navigating to URL
    async fn ensure_tab(&mut self, url: &str) -> eoka::Result<&mut TabState> {
        if self.current_tab_id.is_none() {
            // Create first tab
            let page = self.browser.new_page(url).await?;
            let tab_id = page.target_id().to_string();
            self.tabs.insert(tab_id.clone(), TabState::new(page));
            self.current_tab_id = Some(tab_id);
        } else {
            // Navigate current tab
            let tab_id = self.current_tab_id.as_ref().unwrap();
            if let Some(tab) = self.tabs.get_mut(tab_id) {
                tab.elements.clear();
                tab.page.goto(url).await?;
            }
        }
        Ok(self
            .tabs
            .get_mut(self.current_tab_id.as_ref().unwrap())
            .unwrap())
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

    /// Switch to a tab by ID
    async fn switch_tab(&mut self, tab_id: &str) -> eoka::Result<()> {
        if !self.tabs.contains_key(tab_id) {
            return Err(eoka::Error::ElementNotFound(format!(
                "Tab {} not found",
                tab_id
            )));
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
    ErrorData::internal_error(e.to_string(), None::<Value>)
}

/// Check if an error indicates a broken connection that requires session reset
fn is_transport_error(e: &impl std::fmt::Display) -> bool {
    let msg = e.to_string().to_lowercase();
    msg.contains("websocket")
        || msg.contains("transport")
        || msg.contains("connection")
        || msg.contains("broken pipe")
        || msg.contains("reset by peer")
}

fn text_ok(s: impl Into<String>) -> Result<CallToolResult, ErrorData> {
    Ok(CallToolResult::success(vec![Content::text(s.into())]))
}

/// Parse target as either an index or text to find.
fn resolve_target(elements: &[InteractiveElement], target: &str) -> Result<usize, ErrorData> {
    // Try parsing as number first
    if let Ok(idx) = target.parse::<usize>() {
        if idx < elements.len() {
            return Ok(idx);
        }
        return Err(ErrorData::invalid_params(
            format!(
                "Index {} out of range (have {} elements)",
                idx,
                elements.len()
            ),
            None::<Value>,
        ));
    }

    // Otherwise search by text
    let needle = target.to_lowercase();
    elements
        .iter()
        .find(|e| e.text.to_lowercase().contains(&needle))
        .map(|e| e.index)
        .ok_or_else(|| {
            ErrorData::invalid_params(
                format!(
                    "No element found matching \"{}\". Run observe first or check spelling.",
                    target
                ),
                None::<Value>,
            )
        })
}

/// Wait for page stability after an action
async fn wait_for_stable(page: &Page) -> eoka::Result<()> {
    let _ = page.wait_for_network_idle(200, 2000).await;
    page.wait(50).await;
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
        if guard.is_none() {
            let state = BrowserState::new(self.headless).await.map_err(err)?;
            *guard = Some(state);
        }
        Ok(())
    }

    /// Reset state (call this when connection is broken)
    async fn reset_state(&self) {
        let mut guard = self.state.lock().await;
        if let Some(state) = guard.take() {
            let _ = state.close().await;
        }
    }

    /// Check error and reset state if it's a transport error.
    async fn check_transport_err<E: std::fmt::Display>(&self, e: E) -> ErrorData {
        let msg = e.to_string();
        if is_transport_error(&e) {
            self.reset_state().await;
            ErrorData::internal_error(
                format!("{} (connection lost - retry to reconnect)", msg),
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
        let title = tab.page.title().await.map_err(err)?;
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
        let title = tab.page.title().await.map_err(err)?;
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

    #[tool(description = "Navigate to a URL. Launches browser on first call. Returns page title.")]
    async fn navigate(
        &self,
        req: Parameters<NavigateRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        self.ensure_browser().await?;
        let mut guard = self.state.lock().await;
        let state = guard.as_mut().unwrap();

        let tab = match state.ensure_tab(&req.0.url).await {
            Ok(t) => t,
            Err(e) => {
                drop(guard);
                return Err(self.check_transport_err(e).await);
            }
        };

        wait_for_stable(&tab.page).await.map_err(err)?;
        let url = tab.page.url().await.map_err(err)?;
        let title = tab.page.title().await.map_err(err)?;
        text_ok(format!("Navigated to: {}\nTitle: {}", url, title))
    }

    #[tool(
        description = "List all interactive elements on the page. Returns indexed list like [0] <button> \"Submit\". Call before click/fill/select, or use text targeting."
    )]
    async fn observe(&self) -> Result<CallToolResult, ErrorData> {
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

        let list = element_list(&tab.elements);
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
        description = "Click an element. Target can be index (0) or text (\"Submit\"). Auto-waits for page stability."
    )]
    async fn click(&self, req: Parameters<TargetRequest>) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.state.lock().await;
        let state = guard.as_mut().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let config_viewport_only = state.config.viewport_only;
        let tab = state.current_tab_mut().ok_or_else(|| err(ERR_NO_TAB))?;

        // Auto-observe if empty
        if tab.elements.is_empty() {
            tab.elements = observe::observe(&tab.page, config_viewport_only)
                .await
                .map_err(err)?;
        }

        let idx = resolve_target(&tab.elements, &req.0.target)?;
        let desc = tab
            .elements
            .get(idx)
            .map(|e| e.to_string())
            .unwrap_or_else(|| format!("[{}]", idx));
        let selector = tab.elements[idx].selector.clone();

        tab.page.click(&selector).await.map_err(err)?;
        wait_for_stable(&tab.page).await.map_err(err)?;
        tab.elements.clear(); // Clicks often change the page
        text_ok(format!("Clicked {}", desc))
    }

    #[tool(
        description = "Type text into an input. Target can be index (0) or text/placeholder (\"Email\", \"Search\"). Clears existing text first."
    )]
    async fn fill(&self, req: Parameters<FillRequest>) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.state.lock().await;
        let state = guard.as_mut().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let config_viewport_only = state.config.viewport_only;
        let tab = state.current_tab_mut().ok_or_else(|| err(ERR_NO_TAB))?;

        if tab.elements.is_empty() {
            tab.elements = observe::observe(&tab.page, config_viewport_only)
                .await
                .map_err(err)?;
        }

        let idx = resolve_target(&tab.elements, &req.0.target)?;
        let desc = tab
            .elements
            .get(idx)
            .map(|e| e.to_string())
            .unwrap_or_else(|| format!("[{}]", idx));
        let selector = tab.elements[idx].selector.clone();

        tab.page.fill(&selector, &req.0.text).await.map_err(err)?;
        wait_for_stable(&tab.page).await.map_err(err)?;
        text_ok(format!("Filled {} with \"{}\"", desc, req.0.text))
    }

    #[tool(
        description = "Select dropdown option. Target can be index or text. Value matches option value or visible text."
    )]
    async fn select(&self, req: Parameters<SelectRequest>) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.state.lock().await;
        let state = guard.as_mut().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let config_viewport_only = state.config.viewport_only;
        let tab = state.current_tab_mut().ok_or_else(|| err(ERR_NO_TAB))?;

        if tab.elements.is_empty() {
            tab.elements = observe::observe(&tab.page, config_viewport_only)
                .await
                .map_err(err)?;
        }

        let idx = resolve_target(&tab.elements, &req.0.target)?;
        let selector = tab.elements[idx].selector.clone();

        let arg = serde_json::json!({ "sel": selector, "val": req.0.value });
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
                format!("Option \"{}\" not found in element [{}]", req.0.value, idx),
                None::<Value>,
            ));
        }
        wait_for_stable(&tab.page).await.map_err(err)?;
        tab.elements.clear();
        text_ok(format!("Selected \"{}\" in element [{}]", req.0.value, idx))
    }

    #[tool(
        description = "Hover over element to trigger tooltips, menus, or hover states. Target can be index or text."
    )]
    async fn hover(&self, req: Parameters<TargetRequest>) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.state.lock().await;
        let state = guard.as_mut().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let config_viewport_only = state.config.viewport_only;
        let tab = state.current_tab_mut().ok_or_else(|| err(ERR_NO_TAB))?;

        if tab.elements.is_empty() {
            tab.elements = observe::observe(&tab.page, config_viewport_only)
                .await
                .map_err(err)?;
        }

        let idx = resolve_target(&tab.elements, &req.0.target)?;
        let desc = tab
            .elements
            .get(idx)
            .map(|e| e.to_string())
            .unwrap_or_else(|| format!("[{}]", idx));
        let el = &tab.elements[idx];

        let cx = el.bbox.x + el.bbox.width / 2.0;
        let cy = el.bbox.y + el.bbox.height / 2.0;
        tab.page
            .session()
            .dispatch_mouse_event(eoka::cdp::MouseEventType::MouseMoved, cx, cy, None, None)
            .await
            .map_err(err)?;
        text_ok(format!("Hovered {}", desc))
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
        description = "Scroll page. Target: 'up', 'down', 'top', 'bottom', or element index/text to scroll into view."
    )]
    async fn scroll(&self, req: Parameters<ScrollRequest>) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.state.lock().await;
        let state = guard.as_mut().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let config_viewport_only = state.config.viewport_only;
        let tab = state.current_tab_mut().ok_or_else(|| err(ERR_NO_TAB))?;

        match req.0.target.as_str() {
            "up" => tab
                .page
                .execute("window.scrollBy(0, -window.innerHeight * 0.8)")
                .await
                .map_err(err)?,
            "down" => tab
                .page
                .execute("window.scrollBy(0, window.innerHeight * 0.8)")
                .await
                .map_err(err)?,
            "top" => tab
                .page
                .execute("window.scrollTo(0, 0)")
                .await
                .map_err(err)?,
            "bottom" => tab
                .page
                .execute("window.scrollTo(0, document.body.scrollHeight)")
                .await
                .map_err(err)?,
            target => {
                if tab.elements.is_empty() {
                    tab.elements = observe::observe(&tab.page, config_viewport_only)
                        .await
                        .map_err(err)?;
                }
                let idx = resolve_target(&tab.elements, target)?;
                let selector = &tab.elements[idx].selector;
                let js = format!(
                    "document.querySelector({})?.scrollIntoView({{behavior:'smooth',block:'center'}})",
                    serde_json::to_string(selector).unwrap()
                );
                tab.page.execute(&js).await.map_err(err)?;
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
        description = "Run JavaScript and return result. Supports multi-statement code; the last expression's value is returned as JSON."
    )]
    async fn extract(&self, req: Parameters<JsRequest>) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard.as_ref().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let tab = state.current_tab().ok_or_else(|| err(ERR_NO_TAB))?;
        // Use eval() to handle multi-statement code - returns value of last expression
        // Safely escape the JS code as a JSON string to prevent injection
        let escaped_js = serde_json::to_string(&req.0.js).map_err(err)?;
        let js = format!("JSON.stringify(eval({}))", escaped_js);
        let json_str: String = tab.page.evaluate(&js).await.map_err(err)?;
        text_ok(json_str)
    }

    #[tool(
        description = "Execute JavaScript without expecting a return value. Use for side effects like clicking elements via JS."
    )]
    async fn exec(&self, req: Parameters<JsRequest>) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard.as_ref().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let tab = state.current_tab().ok_or_else(|| err(ERR_NO_TAB))?;
        // Execute JS without caring about return value
        tab.page.execute(&req.0.js).await.map_err(err)?;
        text_ok("Executed successfully")
    }

    #[tool(
        description = "Get all visible text on the page. Useful for reading content without elements."
    )]
    async fn page_text(&self) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard.as_ref().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let tab = state.current_tab().ok_or_else(|| err(ERR_NO_TAB))?;
        let text = tab.page.text().await.map_err(err)?;
        text_ok(text)
    }

    #[tool(description = "Get current URL and page title.")]
    async fn page_info(&self) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard.as_ref().ok_or_else(|| err(ERR_NO_BROWSER))?;
        let tab = state.current_tab().ok_or_else(|| err(ERR_NO_TAB))?;
        let url = tab.page.url().await.map_err(err)?;
        let title = tab.page.title().await.map_err(err)?;
        text_ok(format!("URL: {}\nTitle: {}", url, title))
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
                "Browser automation with multi-tab support.\n\n\
                 Workflow: navigate → screenshot (see page + elements) → click/fill by index or text.\n\n\
                 Tab Management:\n\
                 - list_tabs: see all open tabs (* marks current)\n\
                 - new_tab: open a new tab\n\
                 - switch_tab: switch to a tab by ID\n\
                 - close_tab: close a tab\n\n\
                 Tips:\n\
                 - screenshot returns both image AND element list\n\
                 - Actions auto-observe if needed\n\
                 - Actions auto-wait for page stability\n\
                 - Use cookies/set_cookie for session persistence\n\
                 - Set EOKA_HEADLESS=false to see browser window"
                    .into(),
            ),
        }
    }
}

pub async fn run_server() -> anyhow::Result<()> {
    use rmcp::ServiceExt;

    let server = EokaServer::new();
    let service = server.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}
