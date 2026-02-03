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

use eoka_tools::Session;

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
    agent: Arc<Mutex<Option<Session>>>,
    tool_router: ToolRouter<Self>,
}

impl EokaServer {
    async fn ensure_agent(&self) -> Result<(), ErrorData> {
        let mut guard = self.agent.lock().await;
        if guard.is_none() {
            let agent = Session::launch().await.map_err(err)?;
            *guard = Some(agent);
        }
        Ok(())
    }
}

#[tool_router]
impl EokaServer {
    pub fn new() -> Self {
        Self {
            agent: Arc::new(Mutex::new(None)),
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "Navigate to a URL. Launches browser on first call.")]
    async fn navigate(
        &self,
        req: Parameters<NavigateRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        self.ensure_agent().await?;
        let mut guard = self.agent.lock().await;
        let agent = guard.as_mut().unwrap();
        agent.goto(&req.0.url).await.map_err(err)?;
        let url = agent.url().await.map_err(err)?;
        let title = agent.title().await.map_err(err)?;
        text_ok(format!("Navigated to: {}\nTitle: {}", url, title))
    }

    #[tool(
        description = "Enumerate all interactive elements on the page. Returns a compact text list. Must be called before click/fill/select actions."
    )]
    async fn observe(&self) -> Result<CallToolResult, ErrorData> {
        self.ensure_agent().await?;
        let mut guard = self.agent.lock().await;
        let agent = guard.as_mut().unwrap();
        agent.observe().await.map_err(err)?;
        let list = agent.element_list();
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
        self.ensure_agent().await?;
        let mut guard = self.agent.lock().await;
        let agent = guard.as_mut().unwrap();
        let png = agent.screenshot().await.map_err(err)?;
        let count = agent.len();
        let b64 = BASE64.encode(&png);
        Ok(CallToolResult::success(vec![
            Content::image(b64, "image/png"),
            Content::text(format!("{} interactive elements on page.", count)),
        ]))
    }

    #[tool(
        description = "Click an element by its index from observe. Auto-recovers if element moved, waits for page to stabilize."
    )]
    async fn click(&self, req: Parameters<ClickRequest>) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.agent.lock().await;
        let agent = guard.as_mut().ok_or_else(|| {
            ErrorData::internal_error("No page open. Use navigate first.", None::<Value>)
        })?;

        // Get element description before action
        let desc = agent
            .get(req.0.index)
            .map(|e| e.to_string())
            .unwrap_or_else(|| format!("[{}]", req.0.index));

        agent.click(req.0.index).await.map_err(err)?;
        text_ok(format!("Clicked {}", desc))
    }

    #[tool(
        description = "Clear and type text into an input element by index. Auto-recovers if element moved."
    )]
    async fn fill(&self, req: Parameters<FillRequest>) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.agent.lock().await;
        let agent = guard.as_mut().ok_or_else(|| {
            ErrorData::internal_error("No page open. Use navigate first.", None::<Value>)
        })?;

        let desc = agent
            .get(req.0.index)
            .map(|e| e.to_string())
            .unwrap_or_else(|| format!("[{}]", req.0.index));

        agent.fill(req.0.index, &req.0.text).await.map_err(err)?;
        text_ok(format!("Filled {} with \"{}\"", desc, req.0.text))
    }

    #[tool(description = "Select a dropdown option by element index and value/text.")]
    async fn select(&self, req: Parameters<SelectRequest>) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.agent.lock().await;
        let agent = guard.as_mut().ok_or_else(|| {
            ErrorData::internal_error("No page open. Use navigate first.", None::<Value>)
        })?;

        agent.select(req.0.index, &req.0.value).await.map_err(err)?;
        text_ok(format!(
            "Selected \"{}\" in element [{}]",
            req.0.value, req.0.index
        ))
    }

    #[tool(
        description = "Hover over an element by index to trigger hover states, tooltips, or menus."
    )]
    async fn hover(&self, req: Parameters<HoverRequest>) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.agent.lock().await;
        let agent = guard.as_mut().ok_or_else(|| {
            ErrorData::internal_error("No page open. Use navigate first.", None::<Value>)
        })?;

        let desc = agent
            .get(req.0.index)
            .map(|e| e.to_string())
            .unwrap_or_else(|| format!("[{}]", req.0.index));

        agent.hover(req.0.index).await.map_err(err)?;
        text_ok(format!("Hovered {}", desc))
    }

    #[tool(description = "Press a keyboard key (e.g. Enter, Tab, Escape, ArrowDown, Backspace).")]
    async fn type_key(&self, req: Parameters<TypeKeyRequest>) -> Result<CallToolResult, ErrorData> {
        let guard = self.agent.lock().await;
        let agent = guard.as_ref().ok_or_else(|| {
            ErrorData::internal_error("No page open. Use navigate first.", None::<Value>)
        })?;
        agent.press_key(&req.0.key).await.map_err(err)?;
        text_ok(format!("Pressed {}", req.0.key))
    }

    #[tool(
        description = "Scroll the page. Target: 'up', 'down', 'top', 'bottom', or an element index to scroll into view."
    )]
    async fn scroll(&self, req: Parameters<ScrollRequest>) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.agent.lock().await;
        let agent = guard.as_mut().ok_or_else(|| {
            ErrorData::internal_error("No page open. Use navigate first.", None::<Value>)
        })?;

        match req.0.target.as_str() {
            "up" => agent.scroll_up().await.map_err(err)?,
            "down" => agent.scroll_down().await.map_err(err)?,
            "top" => agent.scroll_to_top().await.map_err(err)?,
            "bottom" => agent.scroll_to_bottom().await.map_err(err)?,
            other => {
                let idx: usize = other.parse().map_err(|_| {
                    ErrorData::invalid_params(
                        "target must be up/down/top/bottom or an element index",
                        None::<Value>,
                    )
                })?;
                agent.scroll_to(idx).await.map_err(err)?;
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
        let guard = self.agent.lock().await;
        let agent = guard.as_ref().ok_or_else(|| {
            ErrorData::internal_error("No page open. Use navigate first.", None::<Value>)
        })?;

        if agent.is_empty() {
            return Err(ErrorData::internal_error(
                "No elements observed. Run observe first.",
                None::<Value>,
            ));
        }

        let needle = req.0.text.to_lowercase();
        let matches: Vec<_> = agent
            .elements()
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
        let guard = self.agent.lock().await;
        let agent = guard.as_ref().ok_or_else(|| {
            ErrorData::internal_error("No page open. Use navigate first.", None::<Value>)
        })?;
        let js = format!("JSON.stringify((()=>{{ return {}; }})())", req.0.js);
        let json_str: String = agent.eval(&js).await.map_err(err)?;
        text_ok(json_str)
    }

    #[tool(description = "Get the visible text content of the page.")]
    async fn page_text(&self) -> Result<CallToolResult, ErrorData> {
        let guard = self.agent.lock().await;
        let agent = guard.as_ref().ok_or_else(|| {
            ErrorData::internal_error("No page open. Use navigate first.", None::<Value>)
        })?;
        let text = agent.text().await.map_err(err)?;
        text_ok(text)
    }

    #[tool(description = "Get the current page URL and title.")]
    async fn page_info(&self) -> Result<CallToolResult, ErrorData> {
        let guard = self.agent.lock().await;
        let agent = guard.as_ref().ok_or_else(|| {
            ErrorData::internal_error("No page open. Use navigate first.", None::<Value>)
        })?;
        let url = agent.url().await.map_err(err)?;
        let title = agent.title().await.map_err(err)?;
        text_ok(format!("URL: {}\nTitle: {}", url, title))
    }

    #[tool(description = "Go back in browser history.")]
    async fn back(&self) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.agent.lock().await;
        let agent = guard.as_mut().ok_or_else(|| {
            ErrorData::internal_error("No page open. Use navigate first.", None::<Value>)
        })?;
        agent.back().await.map_err(err)?;
        text_ok("Navigated back.")
    }

    #[tool(description = "Go forward in browser history.")]
    async fn forward(&self) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.agent.lock().await;
        let agent = guard.as_mut().ok_or_else(|| {
            ErrorData::internal_error("No page open. Use navigate first.", None::<Value>)
        })?;
        agent.forward().await.map_err(err)?;
        text_ok("Navigated forward.")
    }

    #[tool(description = "Close the browser and release resources.")]
    async fn close(&self) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.agent.lock().await;
        if let Some(agent) = guard.take() {
            agent.close().await.map_err(err)?;
        }
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
                name: "eoka-tools".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                title: None,
                icons: None,
                website_url: None,
            },
            instructions: Some(
                "Browser automation server. Use 'navigate' to open a URL (launches browser automatically), \
                 'observe' to list interactive elements, 'screenshot' for annotated screenshots, \
                 then interact by element index with click/fill/select/hover. \
                 Actions auto-wait for page stability and recover from stale elements. \
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
