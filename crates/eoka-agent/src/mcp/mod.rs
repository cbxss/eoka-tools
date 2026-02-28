mod error;
mod helpers;
mod state;
mod types;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rmcp::{
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::*,
    tool, tool_handler, tool_router, ServerHandler,
};
use serde_json::Value;
use std::fmt::Write;
use std::sync::Arc;
use tokio::sync::Mutex;

use eoka_agent::{annotate, captcha, observe, spa, InteractiveElement};

use error::{internal, invalid, is_transport_error_msg, AgentError};
use helpers::{
    auto_observe_if_needed, click_with_retry, element_list, fill_with_retry, resolve_js,
    resolve_target, text_ok, title_nonblocking, wait_for_stable, VALID_OBSERVE_FILTERS,
};
use state::{BrowserState, TabState};
use types::*;

// ---------------------------------------------------------------------------
// Server
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct EokaServer {
    state: Arc<Mutex<Option<BrowserState>>>,
    tool_router: ToolRouter<Self>,
    headless: bool,
}

impl EokaServer {
    /// Lock browser state, returning Err if no browser is open.
    #[allow(dead_code)]
    async fn lock_browser(
        &self,
    ) -> Result<tokio::sync::MutexGuard<'_, Option<BrowserState>>, ErrorData> {
        let guard = self.state.lock().await;
        if guard.is_none() {
            return Err(ErrorData::from(AgentError::NoBrowser));
        }
        Ok(guard)
    }

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
            let state = BrowserState::new(self.headless).await.map_err(internal)?;
            *guard = Some(state);
        }
        Ok(())
    }

    /// Check error and mark state unhealthy if it's a transport error.
    async fn check_transport_err<E: std::fmt::Display>(&self, e: E) -> ErrorData {
        let msg = e.to_string();
        if is_transport_error_msg(&msg) {
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
            internal(e)
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

        let tabs = state.list_tabs().await.map_err(internal)?;
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

        let (tab_id, tab) = state
            .new_tab(req.0.url.as_deref())
            .await
            .map_err(internal)?;

        let url = tab.page.url().await.map_err(internal)?;
        let title = title_nonblocking(&tab.page).await;
        text_ok(format!(
            "Opened new tab [{}]\nURL: {}\nTitle: {}",
            tab_id, url, title
        ))
    }

    #[tool(description = "Switch to a different browser tab by ID. Get IDs from list_tabs.")]
    async fn switch_tab(&self, req: Parameters<TabIdRequest>) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.state.lock().await;
        let state = guard
            .as_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;

        state.switch_tab(&req.0.tab_id).await.map_err(internal)?;

        let tab = state.current_tab().unwrap();
        let url = tab.page.url().await.map_err(internal)?;
        let title = title_nonblocking(&tab.page).await;
        text_ok(format!(
            "Switched to tab [{}]\nURL: {}\nTitle: {}",
            req.0.tab_id, url, title
        ))
    }

    #[tool(description = "Close a browser tab by ID. Cannot close the last remaining tab.")]
    async fn close_tab(&self, req: Parameters<TabIdRequest>) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.state.lock().await;
        let state = guard
            .as_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;

        state.close_tab(&req.0.tab_id).await.map_err(internal)?;
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
        self.ensure_browser().await?;
        let mut guard = self.state.lock().await;
        let state = guard.as_mut().unwrap();

        let has_extras = req.0.headers.is_some()
            || req.0.user_agent.is_some()
            || req.0.bypass_csp.unwrap_or(false);

        // If extras are set, apply them on the page BEFORE navigating (avoids double nav).
        if has_extras {
            // Ensure we have a tab (may create one at about:blank)
            let tab = if state.current_tab_id.is_some() {
                state.current_tab_mut().unwrap()
            } else {
                let page = state.browser.new_blank_page().await.map_err(internal)?;
                let id = page.target_id().to_string();
                state.tabs.insert(id.clone(), TabState::new(page));
                state.current_tab_id = Some(id.clone());
                state.tabs.get_mut(&id).unwrap()
            };
            if let Some(ua) = &req.0.user_agent {
                tab.page.set_user_agent(ua).await.map_err(internal)?;
            }
            if req.0.bypass_csp.unwrap_or(false) {
                tab.page.set_bypass_csp(true).await.map_err(internal)?;
            }
            if let Some(ref h) = req.0.headers {
                tab.page
                    .set_extra_headers(h.clone())
                    .await
                    .map_err(internal)?;
            }
            tab.elements.clear();
            let nav_result = tab.page.goto(&req.0.url).await;
            if req.0.headers.is_some() {
                let _ = tab.page.clear_extra_headers().await;
            }
            nav_result.map_err(internal)?;
        } else {
            let tab = match state.ensure_tab(&req.0.url).await {
                Ok(t) => t,
                Err(e) => {
                    drop(guard);
                    return Err(self.check_transport_err(e).await);
                }
            };
            let _ = tab; // used by ensure_tab
        };

        let tab = state.current_tab_mut().unwrap();
        if let Err(e) = wait_for_stable(&tab.page).await {
            eprintln!("[eoka-agent] wait_for_stable after navigate: {}", e);
        }
        let url = tab.page.url().await.map_err(internal)?;
        let title = title_nonblocking(&tab.page).await;
        text_ok(format!("Navigated to: {}\nTitle: {}", url, title))
    }

    #[tool(
        description = "List interactive elements. Optional filter: 'inputs' (form elements), 'buttons' (clickables), 'all'. Optional max limit. Use live targeting (text:, css:) to skip observe."
    )]
    async fn observe(&self, req: Parameters<ObserveRequest>) -> Result<CallToolResult, ErrorData> {
        // Validate filter before doing any work
        if let Some(ref f) = req.0.filter {
            if !VALID_OBSERVE_FILTERS.contains(&f.as_str()) {
                return Err(invalid(format!(
                    "Unknown filter '{}'. Valid: {}",
                    f,
                    VALID_OBSERVE_FILTERS.join(", ")
                )));
            }
        }

        let mut guard = self.state.lock().await;
        let state = guard
            .as_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let viewport_only = state.config.viewport_only;
        let tab = state
            .current_tab_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;

        tab.elements = match observe::observe(&tab.page, viewport_only).await {
            Ok(e) => e,
            Err(e) => {
                drop(guard);
                return Err(self.check_transport_err(e).await);
            }
        };

        let filter_fn: fn(&&InteractiveElement) -> bool = match req.0.filter.as_deref() {
            Some("inputs") => |e| {
                matches!(
                    e.tag.as_str(),
                    "input" | "select" | "textarea" | "contenteditable"
                )
            },
            Some("buttons") => {
                |e| matches!(e.tag.as_str(), "button" | "a") || e.role.as_deref() == Some("button")
            }
            _ => |_| true,
        };
        let max = req.0.max.unwrap_or(usize::MAX);
        let mut list = String::new();
        for el in tab.elements.iter().filter(filter_fn).take(max) {
            let _ = writeln!(list, "{}", el);
        }
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
        let state = guard
            .as_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let config_viewport_only = state.config.viewport_only;
        let tab = state
            .current_tab_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;

        // Auto-observe if needed
        if tab.elements.is_empty() {
            tab.elements = observe::observe(&tab.page, config_viewport_only)
                .await
                .map_err(internal)?;
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
        let state = guard
            .as_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let vp = state.config.viewport_only;
        let tab = state
            .current_tab_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;

        auto_observe_if_needed(tab, &req.0.target, vp).await?;
        let desc = click_with_retry(tab, &req.0.target, vp).await?;
        let _ = wait_for_stable(&tab.page).await;
        tab.elements.clear();
        text_ok(format!("Clicked {}", desc))
    }

    #[tool(
        description = "Type text into an input. Target: index, text:Label, placeholder:Enter code, css:input, id:field. Auto-retries once on stale element. Clears existing text."
    )]
    async fn fill(&self, req: Parameters<FillRequest>) -> Result<CallToolResult, ErrorData> {
        self.ensure_browser().await?;
        let mut guard = self.state.lock().await;
        let state = guard
            .as_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let vp = state.config.viewport_only;
        let tab = state
            .current_tab_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;

        auto_observe_if_needed(tab, &req.0.target, vp).await?;
        let desc = fill_with_retry(tab, &req.0.target, &req.0.text, vp).await?;
        // No wait_for_stable — fill doesn't cause navigation
        tab.elements.clear();
        text_ok(format!("Filled {} with \"{}\"", desc, req.0.text))
    }

    #[tool(
        description = "Select dropdown option. Target: index, text:Label, css:select, id:dropdown. Value matches option value or visible text."
    )]
    async fn select(&self, req: Parameters<SelectRequest>) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.state.lock().await;
        let state = guard
            .as_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let vp = state.config.viewport_only;
        let tab = state
            .current_tab_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        auto_observe_if_needed(tab, &req.0.target, vp).await?;

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
        let selected: bool = tab.page.evaluate(&js).await.map_err(internal)?;
        if !selected {
            return Err(invalid(format!(
                "Option \"{}\" not found in {}",
                req.0.value, resolved.desc
            )));
        }
        if let Err(e) = wait_for_stable(&tab.page).await {
            eprintln!("[eoka-agent] wait_for_stable: {}", e);
        }
        tab.elements.clear();
        text_ok(format!("Selected \"{}\" in {}", req.0.value, resolved.desc))
    }

    #[tool(
        description = "Hover over element to trigger tooltips, menus, or hover states. Target: index, text:Label, css:selector, etc."
    )]
    async fn hover(&self, req: Parameters<TargetRequest>) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.state.lock().await;
        let state = guard
            .as_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let vp = state.config.viewport_only;
        let tab = state
            .current_tab_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        auto_observe_if_needed(tab, &req.0.target, vp).await?;

        let resolved = resolve_target(&tab.page, &tab.elements, &req.0.target).await?;
        let cx = resolved.bbox.x + resolved.bbox.width / 2.0;
        let cy = resolved.bbox.y + resolved.bbox.height / 2.0;
        tab.page
            .session()
            .dispatch_mouse_event(eoka::cdp::MouseEventType::MouseMoved, cx, cy, None, None)
            .await
            .map_err(internal)?;
        text_ok(format!("Hovered {}", resolved.desc))
    }

    #[tool(
        description = "Press keyboard key. Common: Enter, Tab, Escape, ArrowDown, ArrowUp, Backspace, Space."
    )]
    async fn type_key(&self, req: Parameters<TypeKeyRequest>) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        tab.page
            .human()
            .press_key(&req.0.key)
            .await
            .map_err(internal)?;
        text_ok(format!("Pressed {}", req.0.key))
    }

    #[tool(
        description = "Execute multiple actions in sequence. Reduces round trips. Actions: click, fill, type_key. Uses live targeting."
    )]
    async fn batch(&self, req: Parameters<BatchRequest>) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.state.lock().await;
        let state = guard
            .as_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;

        let mut results = Vec::new();

        for (i, action) in req.0.actions.iter().enumerate() {
            let result =
                match action.action.as_str() {
                    "click" => {
                        let target = action.target.as_ref().ok_or_else(|| {
                            invalid(format!("Action {} (click): missing target", i))
                        })?;
                        let resolved = resolve_target(&tab.page, &tab.elements, target).await?;
                        tab.page.click(&resolved.selector).await.map_err(internal)?;
                        format!("click {}", resolved.desc)
                    }
                    "fill" => {
                        let target = action.target.as_ref().ok_or_else(|| {
                            invalid(format!("Action {} (fill): missing target", i))
                        })?;
                        let text = action
                            .text
                            .as_ref()
                            .ok_or_else(|| invalid(format!("Action {} (fill): missing text", i)))?;
                        let resolved = resolve_target(&tab.page, &tab.elements, target).await?;
                        tab.page
                            .fill(&resolved.selector, text)
                            .await
                            .map_err(internal)?;
                        format!("fill {} with \"{}\"", resolved.desc, text)
                    }
                    "type_key" => {
                        let key = action.text.as_ref().ok_or_else(|| {
                            invalid(format!("Action {} (type_key): missing text (key name)", i))
                        })?;
                        tab.page.human().press_key(key).await.map_err(internal)?;
                        format!("press {}", key)
                    }
                    other => {
                        return Err(invalid(format!(
                            "Action {} unknown action type: {}",
                            i, other
                        )));
                    }
                };
            results.push(result);
        }

        if let Err(e) = wait_for_stable(&tab.page).await {
            eprintln!("[eoka-agent] wait_for_stable: {}", e);
        }
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
        let state = guard
            .as_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let vp = state.config.viewport_only;
        let tab = state
            .current_tab_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;

        match req.0.target.as_str() {
            "up" => tab
                .page
                .execute_sync("window.scrollBy(0, -window.innerHeight * 0.8)")
                .await
                .map_err(internal)?,
            "down" => tab
                .page
                .execute_sync("window.scrollBy(0, window.innerHeight * 0.8)")
                .await
                .map_err(internal)?,
            "top" => tab
                .page
                .execute_sync("window.scrollTo(0, 0)")
                .await
                .map_err(internal)?,
            "bottom" => tab
                .page
                .execute_sync("window.scrollTo(0, document.body.scrollHeight)")
                .await
                .map_err(internal)?,
            target_str => {
                auto_observe_if_needed(tab, target_str, vp).await?;
                let resolved = resolve_target(&tab.page, &tab.elements, target_str).await?;
                let js = format!(
                    "document.querySelector({})?.scrollIntoView({{behavior:'smooth',block:'center'}})",
                    serde_json::to_string(&resolved.selector).unwrap()
                );
                tab.page.execute_sync(&js).await.map_err(internal)?;
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
        let state = guard
            .as_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let config_viewport_only = state.config.viewport_only;
        let tab = state
            .current_tab_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;

        if tab.elements.is_empty() {
            tab.elements = observe::observe(&tab.page, config_viewport_only)
                .await
                .map_err(internal)?;
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
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        let code = resolve_js(&req.0)?;
        let escaped_js = serde_json::to_string(&code).map_err(internal)?;
        let js = format!("JSON.stringify(eval({}))", escaped_js);
        let json_str: String = tab.page.evaluate_sync(&js).await.map_err(internal)?;
        text_ok(json_str)
    }

    #[tool(
        description = "Execute JavaScript without expecting a return value. Use for side effects like clicking elements via JS. \
        Pass js= for inline code, or file= with an absolute path to load from disk (saves tokens for repeated/complex scripts)."
    )]
    async fn exec(&self, req: Parameters<JsRequest>) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        let code = resolve_js(&req.0)?;
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
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;

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

        let json_str: String = tab.page.evaluate(&js).await.map_err(internal)?;

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
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
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
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
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
        let state = guard
            .as_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        tab.elements.clear();
        tab.page.back().await.map_err(internal)?;
        if let Err(e) = wait_for_stable(&tab.page).await {
            eprintln!("[eoka-agent] wait_for_stable after back: {}", e);
        }
        let url = tab.page.url().await.map_err(internal)?;
        text_ok(format!("Navigated back to: {}", url))
    }

    #[tool(description = "Go forward in browser history.")]
    async fn forward(&self) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.state.lock().await;
        let state = guard
            .as_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        tab.elements.clear();
        tab.page.forward().await.map_err(internal)?;
        if let Err(e) = wait_for_stable(&tab.page).await {
            eprintln!("[eoka-agent] wait_for_stable after forward: {}", e);
        }
        let url = tab.page.url().await.map_err(internal)?;
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
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;

        let info = spa::detect_router(&tab.page).await.map_err(internal)?;
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
        let state = guard
            .as_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;

        let info = spa::detect_router(&tab.page).await.map_err(internal)?;
        let new_path = spa::spa_navigate(&tab.page, &info.router_type, &req.0.path)
            .await
            .map_err(internal)?;

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
        let state = guard
            .as_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;

        spa::history_go(&tab.page, req.0.delta)
            .await
            .map_err(internal)?;
        tab.elements.clear(); // DOM will change

        let url = tab.page.url().await.map_err(internal)?;
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
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        let cookies = tab.page.cookies().await.map_err(internal)?;
        let json = serde_json::to_string_pretty(&cookies).map_err(internal)?;
        text_ok(json)
    }

    #[tool(description = "Set a cookie. Useful for restoring sessions or authentication.")]
    async fn set_cookie(
        &self,
        req: Parameters<SetCookieRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        tab.page
            .set_cookie(
                &req.0.name,
                &req.0.value,
                req.0.domain.as_deref(),
                req.0.path.as_deref(),
            )
            .await
            .map_err(internal)?;
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
                return Err(internal(format!(
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
            Err(e) => Err(internal(format!("Failed to solve captcha: {}", e))),
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
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;

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
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;

        let code = resolve_js(&req.0)?;
        tab.page.execute_sync(&code).await.map_err(internal)?;

        text_ok("Captcha token injected")
    }

    #[tool(
        description = "Wait for a specified number of milliseconds. Useful for timing-sensitive operations where you need a pause between actions."
    )]
    async fn wait_ms(&self, req: Parameters<WaitMsRequest>) -> Result<CallToolResult, ErrorData> {
        let capped = req.0.ms.min(30_000);
        tokio::time::sleep(std::time::Duration::from_millis(capped)).await;
        if capped < req.0.ms {
            text_ok(format!(
                "Waited {}ms (capped from {}ms, max 30s)",
                capped, req.0.ms
            ))
        } else {
            text_ok(format!("Waited {}ms", capped))
        }
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
        let state = guard
            .as_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let vp = state.config.viewport_only;
        let tab = state
            .current_tab_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;

        let allow_nav = req.0.allow_nav.unwrap_or(false);
        let wait_ms = req.0.wait_ms.unwrap_or(3000).min(15_000);

        auto_observe_if_needed(tab, &req.0.target, vp).await?;
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

        tab.page
            .execute_sync(&intercept_js)
            .await
            .map_err(internal)?;
        click_with_retry(tab, &req.0.target, vp).await?;

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
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        tab.page
            .delete_cookie(&req.0.name, req.0.domain.as_deref())
            .await
            .map_err(internal)?;
        text_ok(format!("Cookie '{}' deleted", req.0.name))
    }

    #[tool(description = "Clear all cookies for the current browser context.")]
    async fn clear_cookies(&self) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        tab.page.clear_all_cookies().await.map_err(internal)?;
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
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        tab.page
            .accept_dialog(req.0.prompt_text.as_deref())
            .await
            .map_err(internal)?;
        text_ok("Dialog accepted")
    }

    #[tool(description = "Dismiss (cancel) a JavaScript alert/confirm/prompt dialog.")]
    async fn dismiss_dialog(&self) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        tab.page.dismiss_dialog().await.map_err(internal)?;
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
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        let timeout_ms = req.0.timeout_ms.unwrap_or(10_000);
        tab.page
            .wait_for_text(&req.0.text, timeout_ms)
            .await
            .map_err(internal)?;
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
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        let idle_ms = req.0.idle_ms.unwrap_or(500);
        let timeout_ms = req.0.timeout_ms.unwrap_or(10_000);
        tab.page
            .wait_for_network_idle(idle_ms, timeout_ms)
            .await
            .map_err(internal)?;
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
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        let value = tab
            .page
            .local_storage_get(&req.0.key)
            .await
            .map_err(internal)?;
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
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        tab.page
            .local_storage_set(&req.0.key, &req.0.value)
            .await
            .map_err(internal)?;
        text_ok(format!("localStorage['{}'] set", req.0.key))
    }

    #[tool(description = "Get a value from sessionStorage. Returns null message if key not found.")]
    async fn session_storage_get(
        &self,
        req: Parameters<StorageKeyRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        let value = tab
            .page
            .session_storage_get(&req.0.key)
            .await
            .map_err(internal)?;
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
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        tab.page
            .session_storage_set(&req.0.key, &req.0.value)
            .await
            .map_err(internal)?;
        text_ok(format!("sessionStorage['{}'] set", req.0.key))
    }

    #[tool(
        description = "Dump all client-side storage as JSON: localStorage, sessionStorage, and cookies. Use to capture session state for later restore or analysis."
    )]
    async fn dump_storage(&self) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        let storage = tab.page.dump_storage().await.map_err(internal)?;
        let json = serde_json::to_string_pretty(&storage).map_err(internal)?;
        text_ok(json)
    }

    // =========================================================================
    // Close
    // =========================================================================

    #[tool(description = "Close the browser. Call when done to free resources.")]
    async fn close(&self) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.state.lock().await;
        if let Some(state) = guard.take() {
            state.close().await.map_err(internal)?;
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
