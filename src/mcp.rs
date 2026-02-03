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
pub struct HoverRequest {
    #[schemars(description = "Element index (number) OR text to find")]
    pub target: String,
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
pub struct ExtractRequest {
    #[schemars(description = "JavaScript expression that returns a JSON-serializable value")]
    pub js: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetCookieRequest {
    #[schemars(description = "Cookie name")]
    pub name: String,
    #[schemars(description = "Cookie value")]
    pub value: String,
    #[schemars(description = "Cookie domain (e.g. '.example.com'). If omitted, uses current page domain.")]
    pub domain: Option<String>,
    #[schemars(description = "Cookie path (default: '/')")]
    pub path: Option<String>,
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
/// Returns the resolved index.
fn resolve_target(session: &Session, target: &str) -> Result<usize, ErrorData> {
    // Try parsing as number first
    if let Ok(idx) = target.parse::<usize>() {
        if idx < session.len() {
            return Ok(idx);
        }
        return Err(ErrorData::invalid_params(
            format!("Index {} out of range (have {} elements)", idx, session.len()),
            None::<Value>,
        ));
    }

    // Otherwise search by text
    session.find_by_text(target).ok_or_else(|| {
        ErrorData::invalid_params(
            format!(
                "No element found matching \"{}\". Run observe first or check spelling.",
                target
            ),
            None::<Value>,
        )
    })
}

#[derive(Clone)]
pub struct EokaServer {
    session: Arc<Mutex<Option<Session>>>,
    tool_router: ToolRouter<Self>,
    headless: bool,
}

impl EokaServer {
    async fn ensure_session(&self) -> Result<(), ErrorData> {
        let mut guard = self.session.lock().await;
        if guard.is_none() {
            let session = if self.headless {
                Session::launch().await.map_err(err)?
            } else {
                // Launch with headed mode
                use eoka_tools::StealthConfig;
                Session::launch_with_config(StealthConfig {
                    headless: false,
                    ..Default::default()
                })
                .await
                .map_err(err)?
            };
            *guard = Some(session);
        }
        Ok(())
    }

    /// Reset session (call this when connection is broken)
    async fn reset_session(&self) {
        let mut guard = self.session.lock().await;
        if let Some(session) = guard.take() {
            let _ = session.close().await;
        }
    }

    /// Check error and reset session if it's a transport error.
    /// Returns an error with a hint to retry.
    async fn check_transport_err<E: std::fmt::Display>(&self, e: E) -> ErrorData {
        let msg = e.to_string();
        if is_transport_error(&e) {
            self.reset_session().await;
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
        // Check environment for configuration
        let headless = std::env::var("EOKA_HEADLESS")
            .map(|v| v != "false" && v != "0")
            .unwrap_or(true);

        Self {
            session: Arc::new(Mutex::new(None)),
            tool_router: Self::tool_router(),
            headless,
        }
    }

    #[tool(description = "Navigate to a URL. Launches browser on first call. Returns page title.")]
    async fn navigate(
        &self,
        req: Parameters<NavigateRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        self.ensure_session().await?;
        let mut guard = self.session.lock().await;
        let session = guard.as_mut().unwrap();

        if let Err(e) = session.goto(&req.0.url).await {
            drop(guard);
            return Err(self.check_transport_err(e).await);
        }

        let url = session.url().await.map_err(err)?;
        let title = session.title().await.map_err(err)?;
        text_ok(format!("Navigated to: {}\nTitle: {}", url, title))
    }

    #[tool(
        description = "List all interactive elements on the page. Returns indexed list like [0] <button> \"Submit\". Call before click/fill/select, or use text targeting."
    )]
    async fn observe(&self) -> Result<CallToolResult, ErrorData> {
        self.ensure_session().await?;
        let mut guard = self.session.lock().await;
        let session = guard.as_mut().unwrap();

        if let Err(e) = session.observe().await {
            drop(guard);
            return Err(self.check_transport_err(e).await);
        }

        let list = session.element_list();
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
        self.ensure_session().await?;
        let mut guard = self.session.lock().await;
        let session = guard.as_mut().unwrap();

        let png = match session.screenshot().await {
            Ok(p) => p,
            Err(e) => {
                drop(guard);
                return Err(self.check_transport_err(e).await);
            }
        };
        let list = session.element_list();
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
    async fn click(&self, req: Parameters<ClickRequest>) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.session.lock().await;
        let session = guard.as_mut().ok_or_else(|| {
            ErrorData::internal_error("No page open. Use navigate first.", None::<Value>)
        })?;

        // Auto-observe if empty
        if session.is_empty() {
            session.observe().await.map_err(err)?;
        }

        let idx = resolve_target(session, &req.0.target)?;
        let desc = session
            .get(idx)
            .map(|e| e.to_string())
            .unwrap_or_else(|| format!("[{}]", idx));

        session.click(idx).await.map_err(err)?;
        text_ok(format!("Clicked {}", desc))
    }

    #[tool(
        description = "Type text into an input. Target can be index (0) or text/placeholder (\"Email\", \"Search\"). Clears existing text first."
    )]
    async fn fill(&self, req: Parameters<FillRequest>) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.session.lock().await;
        let session = guard.as_mut().ok_or_else(|| {
            ErrorData::internal_error("No page open. Use navigate first.", None::<Value>)
        })?;

        if session.is_empty() {
            session.observe().await.map_err(err)?;
        }

        let idx = resolve_target(session, &req.0.target)?;
        let desc = session
            .get(idx)
            .map(|e| e.to_string())
            .unwrap_or_else(|| format!("[{}]", idx));

        session.fill(idx, &req.0.text).await.map_err(err)?;
        text_ok(format!("Filled {} with \"{}\"", desc, req.0.text))
    }

    #[tool(description = "Select dropdown option. Target can be index or text. Value matches option value or visible text.")]
    async fn select(&self, req: Parameters<SelectRequest>) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.session.lock().await;
        let session = guard.as_mut().ok_or_else(|| {
            ErrorData::internal_error("No page open. Use navigate first.", None::<Value>)
        })?;

        if session.is_empty() {
            session.observe().await.map_err(err)?;
        }

        let idx = resolve_target(session, &req.0.target)?;
        session.select(idx, &req.0.value).await.map_err(err)?;
        text_ok(format!("Selected \"{}\" in element [{}]", req.0.value, idx))
    }

    #[tool(description = "Hover over element to trigger tooltips, menus, or hover states. Target can be index or text.")]
    async fn hover(&self, req: Parameters<HoverRequest>) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.session.lock().await;
        let session = guard.as_mut().ok_or_else(|| {
            ErrorData::internal_error("No page open. Use navigate first.", None::<Value>)
        })?;

        if session.is_empty() {
            session.observe().await.map_err(err)?;
        }

        let idx = resolve_target(session, &req.0.target)?;
        let desc = session
            .get(idx)
            .map(|e| e.to_string())
            .unwrap_or_else(|| format!("[{}]", idx));

        session.hover(idx).await.map_err(err)?;
        text_ok(format!("Hovered {}", desc))
    }

    #[tool(description = "Press keyboard key. Common: Enter, Tab, Escape, ArrowDown, ArrowUp, Backspace, Space.")]
    async fn type_key(&self, req: Parameters<TypeKeyRequest>) -> Result<CallToolResult, ErrorData> {
        let guard = self.session.lock().await;
        let session = guard.as_ref().ok_or_else(|| {
            ErrorData::internal_error("No page open. Use navigate first.", None::<Value>)
        })?;
        session.press_key(&req.0.key).await.map_err(err)?;
        text_ok(format!("Pressed {}", req.0.key))
    }

    #[tool(
        description = "Scroll page. Target: 'up', 'down', 'top', 'bottom', or element index/text to scroll into view."
    )]
    async fn scroll(&self, req: Parameters<ScrollRequest>) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.session.lock().await;
        let session = guard.as_mut().ok_or_else(|| {
            ErrorData::internal_error("No page open. Use navigate first.", None::<Value>)
        })?;

        match req.0.target.as_str() {
            "up" => session.scroll_up().await.map_err(err)?,
            "down" => session.scroll_down().await.map_err(err)?,
            "top" => session.scroll_to_top().await.map_err(err)?,
            "bottom" => session.scroll_to_bottom().await.map_err(err)?,
            target => {
                if session.is_empty() {
                    session.observe().await.map_err(err)?;
                }
                let idx = resolve_target(session, target)?;
                session.scroll_to(idx).await.map_err(err)?;
            }
        }
        text_ok(format!("Scrolled {}", req.0.target))
    }

    #[tool(
        description = "Find elements by text content (case-insensitive). Returns matching elements with indices. Useful before clicking by index."
    )]
    async fn find_text(
        &self,
        req: Parameters<FindTextRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.session.lock().await;
        let session = guard.as_mut().ok_or_else(|| {
            ErrorData::internal_error("No page open. Use navigate first.", None::<Value>)
        })?;

        if session.is_empty() {
            session.observe().await.map_err(err)?;
        }

        let needle = req.0.text.to_lowercase();
        let matches: Vec<_> = session
            .elements()
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

    #[tool(description = "Run JavaScript and return result. Expression should return a JSON-serializable value.")]
    async fn extract(&self, req: Parameters<ExtractRequest>) -> Result<CallToolResult, ErrorData> {
        let guard = self.session.lock().await;
        let session = guard.as_ref().ok_or_else(|| {
            ErrorData::internal_error("No page open. Use navigate first.", None::<Value>)
        })?;
        let js = format!("JSON.stringify((()=>{{ return {}; }})())", req.0.js);
        let json_str: String = session.eval(&js).await.map_err(err)?;
        text_ok(json_str)
    }

    #[tool(description = "Get all visible text on the page. Useful for reading content without elements.")]
    async fn page_text(&self) -> Result<CallToolResult, ErrorData> {
        let guard = self.session.lock().await;
        let session = guard.as_ref().ok_or_else(|| {
            ErrorData::internal_error("No page open. Use navigate first.", None::<Value>)
        })?;
        let text = session.text().await.map_err(err)?;
        text_ok(text)
    }

    #[tool(description = "Get current URL and page title.")]
    async fn page_info(&self) -> Result<CallToolResult, ErrorData> {
        let guard = self.session.lock().await;
        let session = guard.as_ref().ok_or_else(|| {
            ErrorData::internal_error("No page open. Use navigate first.", None::<Value>)
        })?;
        let url = session.url().await.map_err(err)?;
        let title = session.title().await.map_err(err)?;
        text_ok(format!("URL: {}\nTitle: {}", url, title))
    }

    #[tool(description = "Go back in browser history.")]
    async fn back(&self) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.session.lock().await;
        let session = guard.as_mut().ok_or_else(|| {
            ErrorData::internal_error("No page open. Use navigate first.", None::<Value>)
        })?;
        session.back().await.map_err(err)?;
        let url = session.url().await.map_err(err)?;
        text_ok(format!("Navigated back to: {}", url))
    }

    #[tool(description = "Go forward in browser history.")]
    async fn forward(&self) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.session.lock().await;
        let session = guard.as_mut().ok_or_else(|| {
            ErrorData::internal_error("No page open. Use navigate first.", None::<Value>)
        })?;
        session.forward().await.map_err(err)?;
        let url = session.url().await.map_err(err)?;
        text_ok(format!("Navigated forward to: {}", url))
    }

    #[tool(description = "Get all cookies for the current page. Returns JSON array of cookies with name, value, domain, path, etc.")]
    async fn cookies(&self) -> Result<CallToolResult, ErrorData> {
        let guard = self.session.lock().await;
        let session = guard.as_ref().ok_or_else(|| {
            ErrorData::internal_error("No page open. Use navigate first.", None::<Value>)
        })?;
        let cookies = session.page().cookies().await.map_err(err)?;
        let json = serde_json::to_string_pretty(&cookies).map_err(err)?;
        text_ok(json)
    }

    #[tool(description = "Set a cookie. Useful for restoring sessions or authentication.")]
    async fn set_cookie(
        &self,
        req: Parameters<SetCookieRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let guard = self.session.lock().await;
        let session = guard.as_ref().ok_or_else(|| {
            ErrorData::internal_error("No page open. Use navigate first.", None::<Value>)
        })?;
        session
            .page()
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
        let mut guard = self.session.lock().await;
        if let Some(session) = guard.take() {
            session.close().await.map_err(err)?;
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
                "Browser automation tools. Workflow: navigate → screenshot (see page + elements) → click/fill by index or text.\n\n\
                 Text targeting: click({ target: \"Submit\" }) finds and clicks element containing \"Submit\".\n\
                 Index targeting: click({ target: \"0\" }) clicks first element from observe/screenshot.\n\n\
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
