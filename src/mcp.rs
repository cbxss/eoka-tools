use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rmcp::{
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::*,
    tool, tool_handler, tool_router, ServerHandler,
};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

use eoka::Page;
use eoka_agent::{AgentPage, Browser, InteractiveElement};

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

struct State {
    browser: Option<Browser>,
    page: Option<Page>,
    elements: Vec<InteractiveElement>,
}

impl State {
    fn new() -> Self {
        Self {
            browser: None,
            page: None,
            elements: Vec::new(),
        }
    }

    async fn ensure_page(&mut self) -> anyhow::Result<()> {
        if self.page.is_none() {
            let browser = Browser::launch().await?;
            let page = browser.new_page("about:blank").await?;
            self.browser = Some(browser);
            self.page = Some(page);
        }
        Ok(())
    }

    fn require_page(&self) -> Result<&Page, ErrorData> {
        self.page.as_ref().ok_or_else(|| {
            ErrorData::internal_error("No page open. Use navigate first.", None::<Value>)
        })
    }

    fn require_element(&self, index: usize) -> Result<&InteractiveElement, ErrorData> {
        self.elements.get(index).ok_or_else(|| {
            ErrorData::internal_error(
                format!(
                    "Element [{}] not found ({} observed). Run observe first.",
                    index,
                    self.elements.len()
                ),
                None::<Value>,
            )
        })
    }
}

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct NavigateRequest {
    #[schemars(description = "URL to navigate to")]
    pub url: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ClickRequest {
    #[schemars(description = "Element index from observe")]
    pub index: usize,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FillRequest {
    #[schemars(description = "Element index from observe")]
    pub index: usize,
    #[schemars(description = "Text to type into the element")]
    pub text: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SelectRequest {
    #[schemars(description = "Element index of the <select> from observe")]
    pub index: usize,
    #[schemars(description = "Option value or visible text to select")]
    pub value: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct HoverRequest {
    #[schemars(description = "Element index from observe")]
    pub index: usize,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TypeKeyRequest {
    #[schemars(description = "Key to press (e.g. Enter, Tab, Escape, ArrowDown, Backspace)")]
    pub key: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ScrollRequest {
    #[schemars(
        description = "Direction: up, down, top, bottom, or element index to scroll into view"
    )]
    pub target: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FindTextRequest {
    #[schemars(description = "Text substring to search for (case-insensitive)")]
    pub text: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ExtractRequest {
    #[schemars(description = "JavaScript expression that returns a JSON-serializable value")]
    pub js: String,
}

// ---------------------------------------------------------------------------
// Server
// ---------------------------------------------------------------------------

fn err(e: impl std::fmt::Display) -> ErrorData {
    ErrorData::internal_error(e.to_string(), None::<Value>)
}

fn text_ok(s: impl Into<String>) -> Result<CallToolResult, ErrorData> {
    Ok(CallToolResult::success(vec![Content::text(s.into())]))
}

#[derive(Clone)]
pub struct EokaServer {
    state: Arc<Mutex<State>>,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl EokaServer {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(State::new())),
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "Navigate to a URL. Launches browser on first call.")]
    async fn navigate(
        &self,
        req: Parameters<NavigateRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let mut state = self.state.lock().await;
        state.ensure_page().await.map_err(err)?;
        let page = state.require_page()?;
        page.goto(&req.0.url).await.map_err(err)?;
        state.elements.clear();
        let page = state.require_page()?;
        let url = page.url().await.map_err(err)?;
        let title = page.title().await.map_err(err)?;
        text_ok(format!("Navigated to: {}\nTitle: {}", url, title))
    }

    #[tool(
        description = "Enumerate all interactive elements on the page. Returns a compact text list. Must be called before click/fill/select actions."
    )]
    async fn observe(&self) -> Result<CallToolResult, ErrorData> {
        let mut state = self.state.lock().await;
        let page = state.require_page()?;
        let mut agent = AgentPage::new(page);
        agent.observe().await.map_err(err)?;
        let list = agent.element_list();
        state.elements = agent.elements().to_vec();
        text_ok(if list.is_empty() {
            "No interactive elements found.".into()
        } else {
            list
        })
    }

    #[tool(
        description = "Take an annotated screenshot with numbered element labels. Returns base64 PNG image. Also runs observe() to refresh the element list."
    )]
    async fn screenshot(&self) -> Result<CallToolResult, ErrorData> {
        let mut state = self.state.lock().await;
        let page = state.require_page()?;
        // AgentPage starts with no elements, so screenshot() will call observe()
        // internally, giving us a fresh element snapshot every time.
        let mut agent = AgentPage::new(page);
        let png = agent.screenshot().await.map_err(err)?;
        state.elements = agent.elements().to_vec();
        let b64 = BASE64.encode(&png);
        Ok(CallToolResult::success(vec![
            Content::image(b64, "image/png"),
            Content::text(format!(
                "{} interactive elements on page.",
                state.elements.len()
            )),
        ]))
    }

    #[tool(description = "Click an element by its index from observe.")]
    async fn click(&self, req: Parameters<ClickRequest>) -> Result<CallToolResult, ErrorData> {
        let state = self.state.lock().await;
        let el = state.require_element(req.0.index)?;
        let desc = el.to_string();
        let sel = el.selector.clone();
        let page = state.require_page()?;
        page.click(&sel).await.map_err(err)?;
        text_ok(format!("Clicked {}", desc))
    }

    #[tool(description = "Clear and type text into an input element by index.")]
    async fn fill(&self, req: Parameters<FillRequest>) -> Result<CallToolResult, ErrorData> {
        let state = self.state.lock().await;
        let el = state.require_element(req.0.index)?;
        let desc = el.to_string();
        let sel = el.selector.clone();
        let page = state.require_page()?;
        page.fill(&sel, &req.0.text).await.map_err(err)?;
        text_ok(format!("Filled {} with \"{}\"", desc, req.0.text))
    }

    #[tool(description = "Select a dropdown option by element index and value/text.")]
    async fn select(&self, req: Parameters<SelectRequest>) -> Result<CallToolResult, ErrorData> {
        let state = self.state.lock().await;
        let el = state.require_element(req.0.index)?;
        let sel = el.selector.clone();
        let page = state.require_page()?;
        // Duplicates AgentPage::select logic — can't use AgentPage here due to lifetime constraints.
        let arg = serde_json::json!({ "sel": sel, "val": req.0.value });
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
        let selected: bool = page.evaluate(&js).await.map_err(err)?;
        if !selected {
            return Err(ErrorData::internal_error(
                format!(
                    "Option \"{}\" not found in element [{}]",
                    req.0.value, req.0.index
                ),
                None::<Value>,
            ));
        }
        text_ok(format!(
            "Selected \"{}\" in element [{}]",
            req.0.value, req.0.index
        ))
    }

    #[tool(
        description = "Hover over an element by index to trigger hover states, tooltips, or menus."
    )]
    async fn hover(&self, req: Parameters<HoverRequest>) -> Result<CallToolResult, ErrorData> {
        let state = self.state.lock().await;
        let el = state.require_element(req.0.index)?;
        let desc = el.to_string();
        let cx = el.bbox.x + el.bbox.width / 2.0;
        let cy = el.bbox.y + el.bbox.height / 2.0;
        let page = state.require_page()?;
        page.session()
            .dispatch_mouse_event(eoka::cdp::MouseEventType::MouseMoved, cx, cy, None, None)
            .await
            .map_err(err)?;
        text_ok(format!("Hovered {}", desc))
    }

    #[tool(description = "Press a keyboard key (e.g. Enter, Tab, Escape, ArrowDown, Backspace).")]
    async fn type_key(&self, req: Parameters<TypeKeyRequest>) -> Result<CallToolResult, ErrorData> {
        let state = self.state.lock().await;
        let page = state.require_page()?;
        page.human().press_key(&req.0.key).await.map_err(err)?;
        text_ok(format!("Pressed {}", req.0.key))
    }

    #[tool(
        description = "Scroll the page. Target: 'up', 'down', 'top', 'bottom', or an element index to scroll into view."
    )]
    async fn scroll(&self, req: Parameters<ScrollRequest>) -> Result<CallToolResult, ErrorData> {
        let state = self.state.lock().await;
        let page = state.require_page()?;
        match req.0.target.as_str() {
            "up" => page
                .execute("window.scrollBy(0, -window.innerHeight * 0.8)")
                .await
                .map_err(err)?,
            "down" => page
                .execute("window.scrollBy(0, window.innerHeight * 0.8)")
                .await
                .map_err(err)?,
            "top" => page.execute("window.scrollTo(0, 0)").await.map_err(err)?,
            "bottom" => page
                .execute("window.scrollTo(0, document.body.scrollHeight)")
                .await
                .map_err(err)?,
            other => {
                let idx: usize = other.parse().map_err(|_| {
                    ErrorData::invalid_params(
                        "target must be up/down/top/bottom or an element index",
                        None::<Value>,
                    )
                })?;
                let el = state.require_element(idx)?;
                let js = format!(
                    "document.querySelector({})?.scrollIntoView({{behavior:'smooth',block:'center'}})",
                    serde_json::to_string(&el.selector).unwrap()
                );
                page.execute(&js).await.map_err(err)?;
            }
        }
        text_ok(format!("Scrolled {}", req.0.target))
    }

    #[tool(
        description = "Find elements whose text contains a substring (case-insensitive). Searches the last observe() results."
    )]
    async fn find_text(
        &self,
        req: Parameters<FindTextRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let state = self.state.lock().await;
        if state.elements.is_empty() {
            return Err(ErrorData::internal_error(
                "No elements observed. Run observe first.",
                None::<Value>,
            ));
        }
        let needle = req.0.text.to_lowercase();
        let matches: Vec<&InteractiveElement> = state
            .elements
            .iter()
            .filter(|e| e.text.to_lowercase().contains(&needle))
            .collect();
        if matches.is_empty() {
            text_ok(format!("No elements found matching \"{}\"", req.0.text))
        } else {
            let out: String = matches.iter().map(|e| format!("{}\n", e)).collect();
            text_ok(out)
        }
    }

    #[tool(description = "Run a JavaScript expression and return the result as JSON.")]
    async fn extract(&self, req: Parameters<ExtractRequest>) -> Result<CallToolResult, ErrorData> {
        let state = self.state.lock().await;
        let page = state.require_page()?;
        let js = format!("JSON.stringify((()=>{{ return {}; }})())", req.0.js);
        let json_str: String = page.evaluate(&js).await.map_err(err)?;
        text_ok(json_str)
    }

    #[tool(description = "Get the visible text content of the page.")]
    async fn page_text(&self) -> Result<CallToolResult, ErrorData> {
        let state = self.state.lock().await;
        let page = state.require_page()?;
        let text = page.text().await.map_err(err)?;
        text_ok(text)
    }

    #[tool(description = "Get the current page URL and title.")]
    async fn page_info(&self) -> Result<CallToolResult, ErrorData> {
        let state = self.state.lock().await;
        let page = state.require_page()?;
        let url = page.url().await.map_err(err)?;
        let title = page.title().await.map_err(err)?;
        text_ok(format!("URL: {}\nTitle: {}", url, title))
    }

    #[tool(description = "Go back in browser history.")]
    async fn back(&self) -> Result<CallToolResult, ErrorData> {
        let mut state = self.state.lock().await;
        let page = state.require_page()?;
        page.back().await.map_err(err)?;
        state.elements.clear();
        text_ok("Navigated back.")
    }

    #[tool(description = "Go forward in browser history.")]
    async fn forward(&self) -> Result<CallToolResult, ErrorData> {
        let mut state = self.state.lock().await;
        let page = state.require_page()?;
        page.forward().await.map_err(err)?;
        state.elements.clear();
        text_ok("Navigated forward.")
    }

    #[tool(description = "Close the browser and release resources.")]
    async fn close(&self) -> Result<CallToolResult, ErrorData> {
        let mut state = self.state.lock().await;
        state.elements.clear();
        state.page = None;
        state.browser = None;
        text_ok("Browser closed.")
    }
}

#[tool_handler]
impl ServerHandler for EokaServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::LATEST,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "eoka-agent".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                title: None,
                icons: None,
                website_url: None,
            },
            instructions: Some(
                "Browser automation server. Use 'navigate' to open a URL (launches browser automatically), \
                 'observe' to list interactive elements, 'screenshot' for annotated screenshots, \
                 then interact by element index with click/fill/select/hover. \
                 Use 'scroll' and 'type_key' for navigation. 'extract' runs JS expressions."
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
