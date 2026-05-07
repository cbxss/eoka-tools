//! Command handler — maps CLI commands to eoka browser operations.

pub mod intercept;
mod persist;
pub mod profile;
pub mod state;
mod target;

use std::collections::HashMap;
use std::fmt::Write as FmtWrite;

use base64::Engine;
use eoka_agent::{annotate, observe, snapshot};
use serde_json::{json, Value};

use crate::launch_spec::LaunchSpec;
use crate::protocol::Response;
use intercept::{InterceptLogEntry, InterceptState};
use persist::{capture_state, ensure_console_capture, restore_state, SavedState};
use state::{BrowserState, TabState};
use target::{
    auto_observe_if_needed, click_with_retry, fill_with_retry, json_str, resolve_target,
    title_nonblocking, wait_for_stable,
};

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

pub struct Handler {
    state: Option<BrowserState>,
    spec: LaunchSpec,
    intercept: InterceptState,
}

impl Handler {
    pub fn new(spec: LaunchSpec) -> Self {
        Self {
            state: None,
            spec,
            intercept: InterceptState::new(),
        }
    }

    async fn ensure_browser(&mut self) -> Result<(), String> {
        if self.state.is_some() {
            return Ok(());
        }
        let mut state = match &self.spec {
            LaunchSpec::Connect { ws_url } => BrowserState::connected(ws_url)
                .await
                .map_err(|e| e.to_string())?,
            LaunchSpec::Launch {
                headless,
                from_profile,
                clone_state_from,
                no_stealth,
            } => {
                let mut s = BrowserState::launched(*headless, from_profile.as_deref(), *no_stealth)
                    .await
                    .map_err(|e| e.to_string())?;

                // If --clone-state-from was set, capture from the live Chrome,
                // open a blank tab, restore. The state is loaded into the launched
                // browser before any user command runs.
                if let Some(source) = clone_state_from {
                    let saved = persist::clone_state_from_source(source).await?;
                    let url = saved.url.clone();
                    let tab = if !url.is_empty() {
                        s.ensure_tab(&url).await.map_err(|e| e.to_string())?
                    } else {
                        s.ensure_blank_tab().await.map_err(|e| e.to_string())?
                    };
                    restore_state(&tab.page, &saved).await?;
                    tab.invalidate();
                }
                s
            }
        };

        // In live mode, auto-attach to the most recent user tab so the first
        // command (snapshot, click, etc.) operates on what the user is looking
        // at rather than refusing with "no tab open".
        if state.is_live && state.current_tab_id.is_none() {
            if let Ok(tabs) = state.browser.tabs().await {
                if let Some(t) = tabs.into_iter().find(|t| !t.url.starts_with("devtools://")) {
                    let _ = state.attach_existing_tab(&t.id).await;
                }
            }
        }

        self.state = Some(state);
        Ok(())
    }

    fn require_tab(&self) -> Result<&TabState, String> {
        self.state
            .as_ref()
            .ok_or("No browser open. Use 'open' first.")?
            .current_tab()
            .ok_or_else(|| "No tab open. Use 'open' first.".to_string())
    }

    fn require_tab_mut(&mut self) -> Result<&mut TabState, String> {
        self.state
            .as_mut()
            .ok_or("No browser open. Use 'open' first.")?
            .current_tab_mut()
            .ok_or_else(|| "No tab open. Use 'open' first.".to_string())
    }

    fn require_state_mut(&mut self) -> Result<&mut BrowserState, String> {
        self.state
            .as_mut()
            .ok_or_else(|| "No browser open. Use 'open' first.".to_string())
    }

    pub async fn handle(&mut self, cmd: &str, args: &Value) -> Response {
        match self.dispatch(cmd, args).await {
            Ok(resp) => resp,
            Err(e) => Response::err(e),
        }
    }

    async fn dispatch(&mut self, cmd: &str, args: &Value) -> Result<Response, String> {
        match cmd {
            "open" => self.cmd_open(args).await,
            "back" => {
                self.cmd_nav(|p| Box::pin(async { p.back().await }), "Back")
                    .await
            }
            "forward" => {
                self.cmd_nav(|p| Box::pin(async { p.forward().await }), "Forward")
                    .await
            }
            "reload" => self.cmd_reload().await,
            "snapshot" => self.cmd_snapshot(args).await,
            "observe" => self.cmd_observe(args).await,
            "screenshot" => self.cmd_screenshot(args).await,
            "info" => self.cmd_info().await,
            "text" => self.cmd_text().await,
            "find" => self.cmd_find(args).await,
            "click" => self.cmd_click(args).await,
            "dblclick" => self.cmd_dblclick(args).await,
            "fill" => self.cmd_fill(args).await,
            "select" => self.cmd_select(args).await,
            "hover" => self.cmd_hover(args).await,
            "key" => self.cmd_key(args).await,
            "scroll" => self.cmd_scroll(args).await,
            "eval" => self.cmd_eval(args).await,
            "exec" => self.cmd_exec(args).await,
            "fetch" => self.cmd_fetch(args).await,
            "cookies" => self.cmd_cookies().await,
            "set_cookie" => self.cmd_set_cookie(args).await,
            "delete_cookie" => self.cmd_delete_cookie(args).await,
            "clear_cookies" => self.cmd_clear_cookies().await,
            "storage" => self.cmd_storage(args).await,
            "set_storage" => self.cmd_set_storage(args).await,
            "dump_storage" => self.cmd_dump_storage().await,
            "save_state" => self.cmd_save_state(args).await,
            "load_state" => self.cmd_load_state(args).await,
            "headers" => self.cmd_headers(args).await,
            "console" => self.cmd_console(args).await,
            "errors" => self.cmd_errors(args).await,
            "tab_list" => self.cmd_tab_list().await,
            "tab_new" => self.cmd_tab_new(args).await,
            "tab_switch" => self.cmd_tab_switch(args).await,
            "tab_close" => self.cmd_tab_close(args).await,
            "tab_attach" => self.cmd_tab_attach(args).await,
            "clone_from" => self.cmd_clone_from(args).await,
            "wait" => self.cmd_wait(args).await,
            "spa_info" => self.cmd_spa_info().await,
            "spa_navigate" => self.cmd_spa_navigate(args).await,
            "fake_camera" => self.cmd_fake_camera(args).await,
            "wasm_info" => self.cmd_wasm_info().await,
            "wasm_read" => self.cmd_wasm_read(args).await,
            "wasm_write" => self.cmd_wasm_write(args).await,
            "wasm_find" => self.cmd_wasm_find(args).await,
            "intercept_add" => self.cmd_intercept_add(args).await,
            "intercept_list" => self.cmd_intercept_list().await,
            "intercept_remove" => self.cmd_intercept_remove(args).await,
            "intercept_log" => self.cmd_intercept_log(args).await,
            "close" => self.cmd_close().await,
            other => Err(format!("Unknown command: {}", other)),
        }
    }

    // ── Helpers ─────────────────────────────────────────────────────────

    fn arg_str<'a>(&self, args: &'a Value, key: &str) -> Result<&'a str, String> {
        args[key]
            .as_str()
            .ok_or_else(|| format!("Missing '{}'", key))
    }

    /// Get tab + viewport_only config together (avoids repeated boilerplate).
    fn tab_with_config(&mut self) -> Result<(&mut TabState, bool), String> {
        let state = self.require_state_mut()?;
        let vp = state.config.viewport_only;
        let tab = state
            .current_tab_mut()
            .ok_or("No tab open. Use 'open' first.")?;
        Ok((tab, vp))
    }

    // ── Navigation ──────────────────────────────────────────────────────

    async fn cmd_open(&mut self, args: &Value) -> Result<Response, String> {
        let url = self.arg_str(args, "url")?;
        self.ensure_browser().await?;

        let headers: Option<HashMap<String, String>> = args
            .get("headers")
            .filter(|v| !v.is_null())
            .and_then(|v| serde_json::from_value(v.clone()).ok());
        let user_agent = args["user_agent"].as_str();
        let bypass_csp = args["bypass_csp"].as_bool().unwrap_or(false);
        // --inject-js: inject a script via addScriptToEvaluateOnNewDocument before navigation
        let inject_js = args["inject_js"].as_str();
        let has_extras =
            headers.is_some() || user_agent.is_some() || bypass_csp || inject_js.is_some();

        let state = self.require_state_mut()?;

        // In live mode, never navigate the user's currently-attached tab.
        // Always pop a fresh tab in their browser.
        if state.is_live {
            state.new_tab(None).await.map_err(|e| e.to_string())?;
        }

        if has_extras {
            let tab = if state.current_tab_id.is_some() {
                state.current_tab_mut().ok_or("No tab")?
            } else {
                state.ensure_blank_tab().await.map_err(|e| e.to_string())?
            };
            if let Some(ua) = user_agent {
                tab.page
                    .set_user_agent(ua)
                    .await
                    .map_err(|e| e.to_string())?;
            }
            if bypass_csp {
                tab.page
                    .set_bypass_csp(true)
                    .await
                    .map_err(|e| e.to_string())?;
            }
            if let Some(ref h) = headers {
                tab.page
                    .set_extra_headers(h.clone())
                    .await
                    .map_err(|e| e.to_string())?;
            }
            if let Some(js) = inject_js {
                let source = if js.ends_with(".js") {
                    std::fs::read_to_string(js)
                        .map_err(|e| format!("inject_js read error: {}", e))?
                } else {
                    js.to_string()
                };
                tab.page
                    .session()
                    .send::<_, serde_json::Value>(
                        "Page.addScriptToEvaluateOnNewDocument",
                        &serde_json::json!({ "source": source }),
                    )
                    .await
                    .map_err(|e| e.to_string())?;
            }
            tab.invalidate();
            let nav_result = tab.page.goto(url).await;
            if headers.is_some() {
                // Best-effort clear; if it fails the browser is likely broken
                let _ = tab.page.clear_extra_headers().await;
            }
            nav_result.map_err(|e| e.to_string())?;
        } else {
            state.ensure_tab(url).await.map_err(|e| e.to_string())?;
        }

        let tab = state.current_tab_mut().ok_or("No tab after navigate")?;
        let _ = wait_for_stable(&tab.page).await;
        let url = tab.page.url().await.map_err(|e| e.to_string())?;
        let title = title_nonblocking(&tab.page).await;
        Ok(Response::ok_text(format!(
            "Navigated to: {}\nTitle: {}",
            url, title
        )))
    }

    async fn cmd_nav<F>(&mut self, f: F, label: &str) -> Result<Response, String>
    where
        F: FnOnce(
            &eoka::Page,
        )
            -> std::pin::Pin<Box<dyn std::future::Future<Output = eoka::Result<()>> + '_>>,
    {
        let tab = self.require_tab()?;
        f(&tab.page).await.map_err(|e| e.to_string())?;
        let _ = wait_for_stable(&tab.page).await;
        let url = tab.page.url().await.map_err(|e| e.to_string())?;
        Ok(Response::ok_text(format!("{} to: {}", label, url)))
    }

    async fn cmd_reload(&mut self) -> Result<Response, String> {
        let tab = self.require_tab_mut()?;
        tab.page.reload().await.map_err(|e| e.to_string())?;
        tab.invalidate();
        let _ = wait_for_stable(&tab.page).await;
        let url = tab.page.url().await.map_err(|e| e.to_string())?;
        Ok(Response::ok_text(format!("Reloaded: {}", url)))
    }

    // ── Observation ─────────────────────────────────────────────────────

    async fn cmd_snapshot(&mut self, args: &Value) -> Result<Response, String> {
        self.ensure_browser().await?;
        let include_all = args["all"].as_bool().unwrap_or(false);
        let tab = self.require_tab_mut()?;

        let result = snapshot::snapshot(&tab.page, include_all)
            .await
            .map_err(|e| e.to_string())?;

        tab.snapshot_refs.clear();
        for r in &result.refs {
            tab.snapshot_refs
                .insert(r.ref_label.clone(), r.backend_node_id);
        }

        Ok(Response::ok_text(result.tree_text))
    }

    async fn cmd_observe(&mut self, args: &Value) -> Result<Response, String> {
        let filter = args["filter"].as_str();
        let max = args["max"]
            .as_u64()
            .map(|v| v as usize)
            .unwrap_or(usize::MAX);

        if let Some(f) = filter {
            if !matches!(f, "inputs" | "buttons" | "all") {
                return Err(format!(
                    "Unknown filter '{}'. Valid: inputs, buttons, all",
                    f
                ));
            }
        }

        let (tab, viewport_only) = self.tab_with_config()?;
        tab.elements = observe::observe(&tab.page, viewport_only)
            .await
            .map_err(|e| e.to_string())?;

        let filter_fn: fn(&&eoka_agent::InteractiveElement) -> bool = match filter {
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
        let mut list = String::new();
        for el in tab.elements.iter().filter(filter_fn).take(max) {
            let _ = writeln!(list, "{}", el);
        }
        Ok(Response::ok_text(if list.is_empty() {
            "No interactive elements found.".into()
        } else {
            list
        }))
    }

    async fn cmd_screenshot(&mut self, args: &Value) -> Result<Response, String> {
        let annotate_flag = args["annotate"].as_bool().unwrap_or(false);
        let output = args["output"].as_str();

        let (tab, viewport_only) = self.tab_with_config()?;

        let png = if annotate_flag {
            if tab.elements.is_empty() {
                tab.elements = observe::observe(&tab.page, viewport_only)
                    .await
                    .map_err(|e| e.to_string())?;
            }
            annotate::annotated_screenshot(&tab.page, &tab.elements)
                .await
                .map_err(|e| e.to_string())?
        } else {
            tab.page.screenshot().await.map_err(|e| e.to_string())?
        };

        let path = match output {
            Some(p) => std::path::PathBuf::from(p),
            None => {
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis();
                std::env::temp_dir().join(format!("eoka-screenshot-{}.png", ts))
            }
        };

        std::fs::write(&path, &png).map_err(|e| e.to_string())?;

        let mut result = json!({ "path": path.to_string_lossy(), "size": png.len() });
        if annotate_flag {
            let mut list = String::new();
            for el in &tab.elements {
                let _ = writeln!(list, "{}", el);
            }
            result["elements"] = Value::String(list);
        }
        Ok(Response::ok(result))
    }

    async fn cmd_info(&mut self) -> Result<Response, String> {
        let tab = self.require_tab()?;
        let url = tab.page.url().await.map_err(|e| e.to_string())?;
        let title = title_nonblocking(&tab.page).await;
        Ok(Response::ok(json!({ "url": url, "title": title })))
    }

    async fn cmd_text(&mut self) -> Result<Response, String> {
        let tab = self.require_tab()?;
        let text = tab.page.text().await.map_err(|e| e.to_string())?;
        Ok(Response::ok_text(text))
    }

    async fn cmd_find(&mut self, args: &Value) -> Result<Response, String> {
        let text = self.arg_str(args, "text")?;
        let (tab, viewport_only) = self.tab_with_config()?;

        if tab.elements.is_empty() {
            tab.elements = observe::observe(&tab.page, viewport_only)
                .await
                .map_err(|e| e.to_string())?;
        }

        let needle = text.to_lowercase();
        let matches: Vec<_> = tab
            .elements
            .iter()
            .filter(|e| {
                e.text.to_lowercase().contains(&needle)
                    || e.placeholder
                        .as_ref()
                        .is_some_and(|p| p.to_lowercase().contains(&needle))
            })
            .collect();

        if matches.is_empty() {
            Ok(Response::ok_text(format!(
                "No elements found matching \"{}\"",
                text
            )))
        } else {
            let out: String = matches.iter().map(|e| format!("{}\n", e)).collect();
            Ok(Response::ok_text(out))
        }
    }

    // ── Actions ─────────────────────────────────────────────────────────

    async fn cmd_click(&mut self, args: &Value) -> Result<Response, String> {
        self.ensure_browser().await?;
        let target_str = self.arg_str(args, "target")?;
        let (tab, vp) = self.tab_with_config()?;

        auto_observe_if_needed(tab, target_str, vp).await?;
        let desc = click_with_retry(tab, target_str, vp).await?;
        let _ = wait_for_stable(&tab.page).await;
        tab.elements.clear();
        Ok(Response::ok_text(format!("Clicked {}", desc)))
    }

    async fn cmd_dblclick(&mut self, args: &Value) -> Result<Response, String> {
        let target_str = self.arg_str(args, "target")?;
        let (tab, vp) = self.tab_with_config()?;

        auto_observe_if_needed(tab, target_str, vp).await?;
        let resolved = resolve_target(tab, target_str).await?;
        let js = format!(
            "document.querySelector({})?.dispatchEvent(new MouseEvent('dblclick', {{bubbles:true}}))",
            json_str(&resolved.selector)
        );
        tab.page
            .execute_sync(&js)
            .await
            .map_err(|e| e.to_string())?;
        tab.elements.clear();
        Ok(Response::ok_text(format!(
            "Double-clicked {}",
            resolved.desc
        )))
    }

    async fn cmd_fill(&mut self, args: &Value) -> Result<Response, String> {
        self.ensure_browser().await?;
        let target_str = self.arg_str(args, "target")?;
        let text = self.arg_str(args, "text")?;
        let (tab, vp) = self.tab_with_config()?;

        auto_observe_if_needed(tab, target_str, vp).await?;
        let desc = fill_with_retry(tab, target_str, text, vp).await?;
        tab.elements.clear();
        Ok(Response::ok_text(format!(
            "Filled {} with \"{}\"",
            desc, text
        )))
    }

    async fn cmd_select(&mut self, args: &Value) -> Result<Response, String> {
        let target_str = self.arg_str(args, "target")?;
        let value = self.arg_str(args, "value")?;
        let (tab, vp) = self.tab_with_config()?;

        auto_observe_if_needed(tab, target_str, vp).await?;
        let resolved = resolve_target(tab, target_str).await?;
        let arg = json!({ "sel": resolved.selector, "val": value });
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
            arg = serde_json::to_string(&arg).map_err(|e| e.to_string())?
        );
        let selected: bool = tab.page.evaluate(&js).await.map_err(|e| e.to_string())?;
        if !selected {
            return Err(format!(
                "Option \"{}\" not found in {}",
                value, resolved.desc
            ));
        }
        let _ = wait_for_stable(&tab.page).await;
        tab.elements.clear();
        Ok(Response::ok_text(format!(
            "Selected \"{}\" in {}",
            value, resolved.desc
        )))
    }

    async fn cmd_hover(&mut self, args: &Value) -> Result<Response, String> {
        let target_str = self.arg_str(args, "target")?;
        let (tab, vp) = self.tab_with_config()?;

        auto_observe_if_needed(tab, target_str, vp).await?;
        let resolved = resolve_target(tab, target_str).await?;
        let cx = resolved.bbox.x + resolved.bbox.width / 2.0;
        let cy = resolved.bbox.y + resolved.bbox.height / 2.0;
        tab.page
            .session()
            .dispatch_mouse_event(eoka::cdp::MouseEventType::MouseMoved, cx, cy, None, None)
            .await
            .map_err(|e| e.to_string())?;
        Ok(Response::ok_text(format!("Hovered {}", resolved.desc)))
    }

    async fn cmd_key(&mut self, args: &Value) -> Result<Response, String> {
        let key = self.arg_str(args, "key")?;
        let tab = self.require_tab()?;
        tab.page
            .human()
            .press_key(key)
            .await
            .map_err(|e| e.to_string())?;
        Ok(Response::ok_text(format!("Pressed {}", key)))
    }

    async fn cmd_scroll(&mut self, args: &Value) -> Result<Response, String> {
        let target_str = self.arg_str(args, "target")?;
        let (tab, vp) = self.tab_with_config()?;

        match target_str {
            "up" => tab.page.execute_sync("window.scrollBy(0, -window.innerHeight * 0.8)").await,
            "down" => tab.page.execute_sync("window.scrollBy(0, window.innerHeight * 0.8)").await,
            "top" => tab.page.execute_sync("window.scrollTo(0, 0)").await,
            "bottom" => tab.page.execute_sync("window.scrollTo(0, document.body.scrollHeight)").await,
            _ => {
                auto_observe_if_needed(tab, target_str, vp).await?;
                let resolved = resolve_target(tab, target_str).await?;
                let js = format!(
                    "document.querySelector({})?.scrollIntoView({{behavior:'smooth',block:'center'}})",
                    json_str(&resolved.selector)
                );
                tab.page.execute_sync(&js).await
            }
        }
        .map_err(|e| e.to_string())?;
        Ok(Response::ok_text(format!("Scrolled {}", target_str)))
    }

    // ── JavaScript ──────────────────────────────────────────────────────

    async fn cmd_eval(&mut self, args: &Value) -> Result<Response, String> {
        let code = resolve_js(args)?;
        let max_size = args["max_size"].as_u64().map(|v| v as usize);
        let tab = self.require_tab()?;

        // Truncate server-side in JS to avoid CDP message size crashes
        let js = match max_size {
            Some(max) => format!(
                "(() => {{ const r = JSON.stringify(eval({})); return r.length > {} ? r.slice(0, {}) + '...(truncated ' + r.length + ' chars)' : r; }})()",
                json_str(&code), max, max
            ),
            None => format!("JSON.stringify(eval({}))", json_str(&code)),
        };
        let result: String = tab
            .page
            .evaluate_sync(&js)
            .await
            .map_err(|e| e.to_string())?;
        Ok(Response::ok_text(result))
    }

    async fn cmd_exec(&mut self, args: &Value) -> Result<Response, String> {
        let code = resolve_js(args)?;
        let tab = self.require_tab()?;
        let _: String = tab.page.evaluate_sync(&code).await.unwrap_or_default();
        Ok(Response::ok_text("Executed successfully"))
    }

    // ── Network ─────────────────────────────────────────────────────────

    async fn cmd_fetch(&mut self, args: &Value) -> Result<Response, String> {
        let url = self.arg_str(args, "url")?;
        let method = args["method"].as_str().unwrap_or("GET");
        let redirect = args["redirect"].as_str().unwrap_or("follow");
        let max_body = args["max_body"].as_u64().unwrap_or(8192) as usize;

        let headers_json = args
            .get("headers")
            .filter(|v| !v.is_null())
            .map(|h| serde_json::to_string(h).unwrap_or_else(|_| "null".into()))
            .unwrap_or_else(|| "null".into());
        let body_json = args
            .get("body")
            .filter(|v| !v.is_null())
            .map(|b| serde_json::to_string(b).unwrap_or_else(|_| "null".into()))
            .unwrap_or_else(|| "null".into());

        let js = format!(
            r#"(async () => {{
                try {{
                    const opts = {{ method: {method}, credentials: 'include', redirect: {redirect} }};
                    const h = {headers};
                    if (h) opts.headers = h;
                    const b = {body};
                    if (b) opts.body = b;
                    const r = await fetch({url}, opts);
                    const hdrs = {{}};
                    r.headers.forEach((v,k) => hdrs[k] = v);
                    let text = '';
                    try {{ text = await r.text(); }} catch(e) {{}}
                    if ({max_body} > 0 && text.length > {max_body}) text = text.slice(0, {max_body}) + '...(truncated)';
                    return JSON.stringify({{ status: r.status, statusText: r.statusText, type: r.type, url: r.url, headers: hdrs, body: text }});
                }} catch(e) {{
                    return JSON.stringify({{ error: e.message }});
                }}
            }})()"#,
            method = json_str(method),
            redirect = json_str(redirect),
            headers = headers_json,
            body = body_json,
            url = json_str(url),
            max_body = max_body,
        );

        let tab = self.require_tab()?;
        let result: String = tab.page.evaluate(&js).await.map_err(|e| e.to_string())?;
        let parsed: Value = serde_json::from_str(&result).unwrap_or(Value::String(result));
        Ok(Response::ok(parsed))
    }

    // ── Cookies ─────────────────────────────────────────────────────────

    async fn cmd_cookies(&mut self) -> Result<Response, String> {
        let tab = self.require_tab()?;
        let cookies = tab.page.cookies().await.map_err(|e| e.to_string())?;
        let json: Vec<Value> = cookies
            .into_iter()
            .map(|c| {
                json!({
                    "name": c.name, "value": c.value, "domain": c.domain,
                    "path": c.path, "httpOnly": c.http_only, "secure": c.secure,
                    "sameSite": c.same_site, "expires": c.expires,
                })
            })
            .collect();
        Ok(Response::ok(Value::Array(json)))
    }

    async fn cmd_set_cookie(&mut self, args: &Value) -> Result<Response, String> {
        let name = self.arg_str(args, "name")?;
        let value = self.arg_str(args, "value")?;
        let path = args["path"].as_str().unwrap_or("/");

        let tab = self.require_tab()?;
        let domain = match args["domain"].as_str() {
            Some(d) => d.to_string(),
            None => tab
                .page
                .evaluate_sync("location.hostname")
                .await
                .unwrap_or_default(),
        };

        let cookie = eoka::cdp::types::NetworkSetCookie {
            name: name.to_string(),
            value: value.to_string(),
            url: None,
            domain: Some(domain),
            path: Some(path.to_string()),
            secure: None,
            http_only: None,
            same_site: None,
            expires: None,
        };
        tab.page
            .set_cookies_bulk(vec![cookie])
            .await
            .map_err(|e| e.to_string())?;
        Ok(Response::ok_text(format!("Set cookie: {}={}", name, value)))
    }

    async fn cmd_delete_cookie(&mut self, args: &Value) -> Result<Response, String> {
        let name = self.arg_str(args, "name")?;
        let tab = self.require_tab()?;
        let domain = match args["domain"].as_str() {
            Some(d) => d.to_string(),
            None => tab
                .page
                .evaluate_sync::<String>("location.hostname")
                .await
                .unwrap_or_default(),
        };
        tab.page
            .session()
            .send::<_, Value>(
                "Network.deleteCookies",
                &json!({ "name": name, "domain": domain }),
            )
            .await
            .map_err(|e| e.to_string())?;
        Ok(Response::ok_text(format!("Deleted cookie: {}", name)))
    }

    async fn cmd_clear_cookies(&mut self) -> Result<Response, String> {
        let tab = self.require_tab()?;
        tab.page
            .clear_all_cookies()
            .await
            .map_err(|e| e.to_string())?;
        Ok(Response::ok_text("Cleared all cookies"))
    }

    // ── Storage ─────────────────────────────────────────────────────────

    async fn cmd_storage(&mut self, args: &Value) -> Result<Response, String> {
        let key = args["key"].as_str();
        let session = args["session_storage"].as_bool().unwrap_or(false);
        let s = if session {
            "sessionStorage"
        } else {
            "localStorage"
        };
        let tab = self.require_tab()?;

        if let Some(k) = key {
            let js = format!("{}.getItem({})", s, json_str(k));
            let val: Value = tab.page.evaluate(&js).await.map_err(|e| e.to_string())?;
            Ok(Response::ok(val))
        } else {
            let js = format!("JSON.stringify(Object.fromEntries(Object.entries({})))", s);
            let json_str: String = tab.page.evaluate_sync(&js).await.unwrap_or_default();
            let parsed: Value = serde_json::from_str(&json_str).unwrap_or(Value::String(json_str));
            Ok(Response::ok(parsed))
        }
    }

    async fn cmd_set_storage(&mut self, args: &Value) -> Result<Response, String> {
        let key = self.arg_str(args, "key")?;
        let value = self.arg_str(args, "value")?;
        let session = args["session_storage"].as_bool().unwrap_or(false);
        let s = if session {
            "sessionStorage"
        } else {
            "localStorage"
        };
        let tab = self.require_tab()?;
        let js = format!("{}.setItem({}, {})", s, json_str(key), json_str(value));
        tab.page
            .execute_sync(&js)
            .await
            .map_err(|e| e.to_string())?;
        Ok(Response::ok_text(format!(
            "Set {}.{} = \"{}\"",
            s, key, value
        )))
    }

    async fn cmd_dump_storage(&mut self) -> Result<Response, String> {
        let tab = self.require_tab()?;
        let combined: String = tab
            .page
            .evaluate_sync(
                "JSON.stringify({\
                    localStorage: Object.fromEntries(Object.entries(localStorage)),\
                    sessionStorage: Object.fromEntries(Object.entries(sessionStorage))\
                })",
            )
            .await
            .unwrap_or_else(|_| "{}".to_string());
        let parsed: Value = serde_json::from_str(&combined).unwrap_or_else(|_| json!({}));
        Ok(Response::ok(parsed))
    }

    // ── State save/load ─────────────────────────────────────────────────

    async fn cmd_save_state(&mut self, args: &Value) -> Result<Response, String> {
        let path = self.arg_str(args, "path")?;
        let tab = self.require_tab()?;
        let state = capture_state(&tab.page).await?;
        let json = serde_json::to_string_pretty(&state).map_err(|e| e.to_string())?;
        std::fs::write(path, &json).map_err(|e| e.to_string())?;
        Ok(Response::ok_text(format!(
            "Saved state to {} ({} cookies, {} localStorage, {} sessionStorage)",
            path,
            state.cookies.len(),
            state.local_storage.len(),
            state.session_storage.len()
        )))
    }

    async fn cmd_load_state(&mut self, args: &Value) -> Result<Response, String> {
        let path = self.arg_str(args, "path")?;
        let no_navigate = args["no_navigate"].as_bool().unwrap_or(false);

        let json = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let saved: SavedState = serde_json::from_str(&json).map_err(|e| e.to_string())?;

        self.ensure_browser().await?;
        let state = self.require_state_mut()?;

        if !no_navigate && !saved.url.is_empty() {
            state
                .ensure_tab(&saved.url)
                .await
                .map_err(|e| e.to_string())?;
        }

        let tab = state.current_tab_mut().ok_or("No tab open.")?;
        restore_state(&tab.page, &saved).await?;
        tab.invalidate();

        Ok(Response::ok_text(format!(
            "Loaded state from {} ({} cookies, {} localStorage, {} sessionStorage)",
            path,
            saved.cookies.len(),
            saved.local_storage.len(),
            saved.session_storage.len()
        )))
    }

    // ── Headers ─────────────────────────────────────────────────────────

    async fn cmd_headers(&mut self, args: &Value) -> Result<Response, String> {
        let raw = &args["headers_json"];
        let headers: HashMap<String, String> = if let Some(s) = raw.as_str() {
            serde_json::from_str(s).map_err(|_| "Invalid headers JSON")?
        } else {
            serde_json::from_value(raw.clone()).map_err(|_| "Invalid headers JSON")?
        };

        let tab = self.require_tab()?;
        tab.page
            .set_extra_headers(headers.clone())
            .await
            .map_err(|e| e.to_string())?;
        Ok(Response::ok_text(format!(
            "Set {} extra headers",
            headers.len()
        )))
    }

    // ── Console / Errors ────────────────────────────────────────────────

    async fn cmd_console(&mut self, args: &Value) -> Result<Response, String> {
        let clear = args["clear"].as_bool().unwrap_or(false);
        let level = args["level"].as_str();

        let tab = self.require_tab_mut()?;
        ensure_console_capture(tab).await?;

        let js = match level {
            Some(l) => format!(
                "JSON.stringify((__eoka_console || []).filter(e => e.level === {}))",
                json_str(l)
            ),
            None => "JSON.stringify(__eoka_console || [])".into(),
        };
        let result: String = tab
            .page
            .evaluate_sync(&js)
            .await
            .unwrap_or_else(|_| "[]".into());
        if clear {
            let _: String = tab
                .page
                .evaluate_sync("__eoka_console = []")
                .await
                .unwrap_or_default();
        }
        let parsed: Value = serde_json::from_str(&result).unwrap_or(Value::Array(vec![]));
        Ok(Response::ok(parsed))
    }

    async fn cmd_errors(&mut self, args: &Value) -> Result<Response, String> {
        let clear = args["clear"].as_bool().unwrap_or(false);
        let tab = self.require_tab_mut()?;
        ensure_console_capture(tab).await?;

        let result: String = tab
            .page
            .evaluate_sync("JSON.stringify(__eoka_errors || [])")
            .await
            .unwrap_or_else(|_| "[]".into());
        if clear {
            let _: String = tab
                .page
                .evaluate_sync("__eoka_errors = []")
                .await
                .unwrap_or_default();
        }
        let parsed: Value = serde_json::from_str(&result).unwrap_or(Value::Array(vec![]));
        Ok(Response::ok(parsed))
    }

    // ── Tabs ────────────────────────────────────────────────────────────

    async fn cmd_tab_list(&mut self) -> Result<Response, String> {
        let state = self.state.as_ref().ok_or("No browser open.")?;
        let tabs = state.browser.tabs().await.map_err(|e| e.to_string())?;
        let current_id = state.current_tab_id.as_deref();

        let mut out = String::new();
        for tab in tabs {
            let marker = if Some(tab.id.as_str()) == current_id {
                " *"
            } else {
                ""
            };
            let _ = writeln!(out, "[{}]{} {}\n  {}", tab.id, marker, tab.title, tab.url);
        }
        Ok(Response::ok_text(if out.is_empty() {
            "No tabs open.".into()
        } else {
            out
        }))
    }

    async fn cmd_tab_new(&mut self, args: &Value) -> Result<Response, String> {
        self.ensure_browser().await?;
        let url = args["url"].as_str();
        let state = self.require_state_mut()?;
        let (tab_id, tab) = state.new_tab(url).await.map_err(|e| e.to_string())?;
        let url = tab.page.url().await.map_err(|e| e.to_string())?;
        let title = title_nonblocking(&tab.page).await;
        Ok(Response::ok_text(format!(
            "Opened new tab [{}]\nURL: {}\nTitle: {}",
            tab_id, url, title
        )))
    }

    async fn cmd_tab_switch(&mut self, args: &Value) -> Result<Response, String> {
        let tab_id = self.arg_str(args, "tab_id")?;
        let state = self.require_state_mut()?;
        state.switch_tab(tab_id).await.map_err(|e| e.to_string())?;
        let tab = state.current_tab().ok_or("Tab switch failed")?;
        let url = tab.page.url().await.map_err(|e| e.to_string())?;
        let title = title_nonblocking(&tab.page).await;
        Ok(Response::ok_text(format!(
            "Switched to tab [{}]\nURL: {}\nTitle: {}",
            tab_id, url, title
        )))
    }

    async fn cmd_tab_attach(&mut self, args: &Value) -> Result<Response, String> {
        let tab_id = self.arg_str(args, "tab_id")?.to_string();
        self.ensure_browser().await?;
        let state = self.require_state_mut()?;
        state
            .attach_existing_tab(&tab_id)
            .await
            .map_err(|e| e.to_string())?;
        let tab = state.current_tab().ok_or("Attach failed")?;
        let url = tab.page.url().await.map_err(|e| e.to_string())?;
        let title = title_nonblocking(&tab.page).await;
        Ok(Response::ok_text(format!(
            "Attached to tab [{}]\nURL: {}\nTitle: {}",
            tab_id, url, title
        )))
    }

    async fn cmd_clone_from(&mut self, args: &Value) -> Result<Response, String> {
        let source = self.arg_str(args, "source")?.to_string();
        let to = args["to"].as_str().map(std::path::PathBuf::from);

        let saved = persist::clone_state_from_source(&source).await?;

        if let Some(path) = to {
            let json = serde_json::to_string_pretty(&saved).map_err(|e| e.to_string())?;
            std::fs::write(&path, &json).map_err(|e| e.to_string())?;
            return Ok(Response::ok_text(format!(
                "Saved state from {} to {} ({} cookies, {} localStorage, {} sessionStorage)",
                source,
                path.display(),
                saved.cookies.len(),
                saved.local_storage.len(),
                saved.session_storage.len()
            )));
        }

        // No --to: load directly into the active session.
        self.ensure_browser().await?;
        let state = self.require_state_mut()?;
        let url = saved.url.clone();
        let tab = if !url.is_empty() {
            state.ensure_tab(&url).await.map_err(|e| e.to_string())?
        } else {
            state.ensure_blank_tab().await.map_err(|e| e.to_string())?
        };
        restore_state(&tab.page, &saved).await?;
        tab.invalidate();
        Ok(Response::ok_text(format!(
            "Hydrated session from {} ({} cookies, {} localStorage, {} sessionStorage)",
            source,
            saved.cookies.len(),
            saved.local_storage.len(),
            saved.session_storage.len()
        )))
    }

    async fn cmd_tab_close(&mut self, args: &Value) -> Result<Response, String> {
        let tab_id = self.arg_str(args, "tab_id")?;
        let state = self.require_state_mut()?;
        state.close_tab(tab_id).await.map_err(|e| e.to_string())?;
        Ok(Response::ok_text(format!("Closed tab [{}]", tab_id)))
    }

    // ── Wait ────────────────────────────────────────────────────────────

    async fn cmd_wait(&mut self, args: &Value) -> Result<Response, String> {
        if let Some(ms) = args["ms"].as_u64() {
            let ms = ms.min(30_000);
            tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
            return Ok(Response::ok_text(format!("Waited {}ms", ms)));
        }
        let timeout = args["timeout"].as_u64().unwrap_or(10_000);
        let tab = self.require_tab()?;

        if let Some(text) = args["text"].as_str() {
            tab.page
                .wait_for_text(text, timeout)
                .await
                .map_err(|e| e.to_string())?;
            return Ok(Response::ok_text(format!("Text \"{}\" appeared", text)));
        }
        if let Some(url_pat) = args["url"].as_str() {
            tab.page
                .wait_for_url_contains(url_pat, timeout)
                .await
                .map_err(|e| e.to_string())?;
            return Ok(Response::ok_text(format!("URL matched \"{}\"", url_pat)));
        }
        if args["load"].as_str().is_some() {
            tab.page
                .wait_for_network_idle(500, timeout)
                .await
                .map_err(|e| e.to_string())?;
            return Ok(Response::ok_text("Network idle"));
        }
        Err("Provide ms, --text, --url, or --load".into())
    }

    // ── SPA ─────────────────────────────────────────────────────────────

    async fn cmd_spa_info(&mut self) -> Result<Response, String> {
        let tab = self.require_tab()?;
        let info = eoka_agent::spa::detect_router(&tab.page)
            .await
            .map_err(|e: eoka::Error| e.to_string())?;
        Ok(Response::ok(json!({
            "router_type": format!("{:?}", info.router_type),
            "current_path": info.current_path,
            "can_navigate": info.can_navigate,
        })))
    }

    async fn cmd_spa_navigate(&mut self, args: &Value) -> Result<Response, String> {
        let path = self.arg_str(args, "path")?;
        let tab = self.require_tab_mut()?;
        let info = eoka_agent::spa::detect_router(&tab.page)
            .await
            .map_err(|e: eoka::Error| e.to_string())?;
        eoka_agent::spa::spa_navigate(&tab.page, &info.router_type, path)
            .await
            .map_err(|e: eoka::Error| e.to_string())?;
        let _ = wait_for_stable(&tab.page).await;
        tab.invalidate();
        let url = tab.page.url().await.map_err(|e| e.to_string())?;
        Ok(Response::ok_text(format!("SPA navigated to: {}", url)))
    }

    // ── Fake Camera ──────────────────────────────────────────────────────

    async fn cmd_fake_camera(&mut self, args: &Value) -> Result<Response, String> {
        let file_path = self.arg_str(args, "file")?;
        let loop_video = args["loop_video"].as_bool().unwrap_or(false);

        let data = std::fs::read(file_path)
            .map_err(|e| format!("Failed to read video file '{}': {}", file_path, e))?;

        let mime = if file_path.ends_with(".webm") {
            "video/webm"
        } else {
            "video/mp4"
        };

        let b64 = base64::engine::general_purpose::STANDARD.encode(&data);

        let template = include_str!("../js/fake_camera.js");
        let js = template
            .replace("/*VIDEO_DATA*/", &b64)
            .replace("/*MIME_TYPE*/", mime)
            .replace("/*LOOP*/", if loop_video { "true" } else { "false" });

        self.ensure_browser().await?;
        let tab = self.require_tab_mut()?;

        // Register for all future navigations
        tab.page
            .session()
            .send::<_, Value>(
                "Page.addScriptToEvaluateOnNewDocument",
                &json!({ "source": js }),
            )
            .await
            .map_err(|e| e.to_string())?;

        // Inject into current page too
        let _: String = tab.page.evaluate_sync(&js).await.unwrap_or_default();

        // Grant camera permission
        let _ = tab
            .page
            .session()
            .send::<_, Value>(
                "Browser.grantPermissions",
                &json!({ "permissions": ["videoCapture", "audioCapture"] }),
            )
            .await;

        Ok(Response::ok_text(format!(
            "Fake camera injected ({}KB {}, loop={})",
            data.len() / 1024,
            mime,
            loop_video
        )))
    }

    // ── WASM ────────────────────────────────────────────────────────────

    async fn cmd_wasm_info(&mut self) -> Result<Response, String> {
        let tab = self.require_tab()?;
        let js = include_str!("../js/wasm_info.js");
        let result: String = tab
            .page
            .evaluate_sync(js)
            .await
            .map_err(|e| e.to_string())?;
        let parsed: Value = serde_json::from_str(&result).unwrap_or(Value::Array(vec![]));
        Ok(Response::ok(parsed))
    }

    async fn cmd_wasm_read(&mut self, args: &Value) -> Result<Response, String> {
        let addr = parse_addr(self.arg_str(args, "addr")?)?;
        let len = args["len"].as_u64().ok_or("Missing 'len'")? as usize;
        let memory = args["memory"].as_str();

        let tab = self.require_tab()?;
        let mem_expr = wasm_memory_expr(memory);

        // Read in 64KB chunks to avoid CDP message size issues
        let chunk_size = 65536;
        let mut hex_output = String::new();
        let mut offset = 0;

        while offset < len {
            let chunk_len = (len - offset).min(chunk_size);
            let js = format!(
                r#"(() => {{
                    const mem = {mem};
                    if (!mem || !mem.buffer) return null;
                    const buf = new Uint8Array(mem.buffer, {addr} + {offset}, {chunk_len});
                    return Array.from(buf).map(b => b.toString(16).padStart(2, '0')).join('');
                }})()"#,
                mem = mem_expr,
                addr = addr,
                offset = offset,
                chunk_len = chunk_len,
            );
            let chunk: String = tab
                .page
                .evaluate_sync(&js)
                .await
                .map_err(|e| e.to_string())?;
            if chunk == "null" || chunk.is_empty() {
                return Err(format!(
                    "WASM memory not found or address out of bounds (tried {})",
                    mem_expr
                ));
            }
            hex_output.push_str(&chunk);
            offset += chunk_len;
        }

        // Format as hex dump
        let mut formatted = String::new();
        let bytes_per_line = 16;
        for (i, chunk) in hex_output.as_bytes().chunks(bytes_per_line * 2).enumerate() {
            let line_addr = addr + i * bytes_per_line;
            use std::fmt::Write;
            let _ = write!(formatted, "{:08x}  ", line_addr);
            for (j, pair) in chunk.chunks(2).enumerate() {
                if j == 8 {
                    formatted.push(' ');
                }
                formatted.push(pair[0] as char);
                formatted.push(pair[1] as char);
                formatted.push(' ');
            }
            formatted.push('\n');
        }

        Ok(Response::ok_text(formatted))
    }

    async fn cmd_wasm_write(&mut self, args: &Value) -> Result<Response, String> {
        let addr = parse_addr(self.arg_str(args, "addr")?)?;
        let hex = self.arg_str(args, "hex")?;
        let memory = args["memory"].as_str();

        // Normalize hex: remove spaces, ensure even length
        let hex_clean: String = hex.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        if !hex_clean.len().is_multiple_of(2) {
            return Err("Hex string must have even length".into());
        }

        let mem_expr = wasm_memory_expr(memory);
        let js = format!(
            r#"(() => {{
                const mem = {mem};
                if (!mem || !mem.buffer) return 'error: WASM memory not found';
                const bytes = new Uint8Array([{byte_array}]);
                new Uint8Array(mem.buffer).set(bytes, {addr});
                return 'ok';
            }})()"#,
            mem = mem_expr,
            addr = addr,
            byte_array = hex_to_byte_array(&hex_clean),
        );

        let tab = self.require_tab()?;
        let result: String = tab
            .page
            .evaluate_sync(&js)
            .await
            .map_err(|e| e.to_string())?;
        if result.starts_with("error:") {
            return Err(result);
        }

        Ok(Response::ok_text(format!(
            "Wrote {} bytes at 0x{:x}",
            hex_clean.len() / 2,
            addr
        )))
    }

    async fn cmd_wasm_find(&mut self, args: &Value) -> Result<Response, String> {
        let pattern = self.arg_str(args, "pattern")?;
        let memory = args["memory"].as_str();
        let max = args["max"].as_u64().unwrap_or(20) as usize;
        let start = args["start"]
            .as_str()
            .map(parse_addr)
            .transpose()?
            .unwrap_or(0);
        let end = args["end"].as_str().map(parse_addr).transpose()?;

        let pattern_clean: String = pattern.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        if !pattern_clean.len().is_multiple_of(2) || pattern_clean.is_empty() {
            return Err("Pattern must be non-empty hex with even length".into());
        }

        let mem_expr = wasm_memory_expr(memory);
        let end_expr = match end {
            Some(e) => format!("{}", e),
            None => "buf.length".to_string(),
        };

        let js = format!(
            r#"(() => {{
                const mem = {mem};
                if (!mem || !mem.buffer) return JSON.stringify({{error: 'WASM memory not found'}});
                const buf = new Uint8Array(mem.buffer);
                const pattern = [{byte_array}];
                const results = [];
                const end = Math.min({end_expr}, buf.length);
                for (let i = {start}; i <= end - pattern.length; i++) {{
                    let match = true;
                    for (let j = 0; j < pattern.length; j++) {{
                        if (buf[i + j] !== pattern[j]) {{ match = false; break; }}
                    }}
                    if (match) {{
                        results.push(i);
                        if (results.length >= {max}) break;
                    }}
                }}
                return JSON.stringify({{matches: results, searched: end - {start}}});
            }})()"#,
            mem = mem_expr,
            byte_array = hex_to_byte_array(&pattern_clean),
            start = start,
            end_expr = end_expr,
            max = max,
        );

        let tab = self.require_tab()?;
        let result: String = tab
            .page
            .evaluate_sync(&js)
            .await
            .map_err(|e| e.to_string())?;
        let parsed: Value = serde_json::from_str(&result).unwrap_or(Value::Null);

        if let Some(err) = parsed.get("error").and_then(|v| v.as_str()) {
            return Err(err.to_string());
        }

        let matches = parsed.get("matches").and_then(|v| v.as_array());
        let searched = parsed.get("searched").and_then(|v| v.as_u64()).unwrap_or(0);

        match matches {
            Some(addrs) if !addrs.is_empty() => {
                let mut out = format!(
                    "Found {} matches (searched {} bytes):\n",
                    addrs.len(),
                    searched
                );
                for a in addrs {
                    if let Some(addr) = a.as_u64() {
                        use std::fmt::Write;
                        let _ = writeln!(out, "  0x{:08x}", addr);
                    }
                }
                Ok(Response::ok_text(out))
            }
            _ => Ok(Response::ok_text(format!(
                "No matches found (searched {} bytes)",
                searched
            ))),
        }
    }

    // ── Intercept ───────────────────────────────────────────────────────

    async fn cmd_intercept_add(&mut self, args: &Value) -> Result<Response, String> {
        let url_pattern = self.arg_str(args, "url_pattern")?.to_string();
        let capture = args["capture"].as_str().map(std::path::PathBuf::from);
        let respond = args["respond"].as_str().map(std::path::PathBuf::from);
        let status = args["status"].as_u64().unwrap_or(200) as u16;

        let id = self
            .intercept
            .add_rule(url_pattern.clone(), capture, respond, status);

        // Enable Fetch interception if not already enabled
        if !self.intercept.enabled {
            self.ensure_browser().await?;
            let tab = self.require_tab()?;
            tab.page
                .session()
                .fetch_enable(false)
                .await
                .map_err(|e| e.to_string())?;
            self.intercept.enabled = true;
        }

        Ok(Response::ok_text(format!(
            "Added intercept rule #{} for \"{}\"",
            id, url_pattern
        )))
    }

    async fn cmd_intercept_list(&mut self) -> Result<Response, String> {
        // Drain any pending Fetch events before listing
        self.drain_fetch_events().await;
        Ok(Response::ok(self.intercept.list_json()))
    }

    async fn cmd_intercept_remove(&mut self, args: &Value) -> Result<Response, String> {
        let id = self.arg_str(args, "id")?;
        if id == "all" {
            self.intercept.remove_all();
            if self.intercept.enabled {
                if let Some(ref state) = self.state {
                    if let Some(tab) = state.current_tab() {
                        let _ = tab
                            .page
                            .session()
                            .send::<_, Value>("Fetch.disable", &json!({}))
                            .await;
                    }
                }
                self.intercept.enabled = false;
            }
            Ok(Response::ok_text("Removed all intercept rules"))
        } else {
            let id_num: usize = id.parse().map_err(|_| "Invalid rule ID")?;
            if self.intercept.remove_rule(id_num) {
                Ok(Response::ok_text(format!("Removed rule #{}", id_num)))
            } else {
                Err(format!("Rule #{} not found", id_num))
            }
        }
    }

    async fn cmd_intercept_log(&mut self, args: &Value) -> Result<Response, String> {
        // Drain pending events first
        self.drain_fetch_events().await;
        let clear = args["clear"].as_bool().unwrap_or(false);
        let result = self.intercept.log_json();
        if clear {
            self.intercept.clear_log();
        }
        Ok(Response::ok(result))
    }

    /// Drain pending Fetch.requestPaused CDP events and process them.
    async fn drain_fetch_events(&mut self) {
        if !self.intercept.enabled {
            return;
        }

        // Collect events first to avoid borrow conflict
        let events = {
            let state = match self.state.as_ref() {
                Some(s) => s,
                None => return,
            };
            let tab = match state.current_tab() {
                Some(t) => t,
                None => return,
            };
            let transport = tab.page.session().transport();
            let mut events = Vec::new();
            loop {
                match transport.try_recv_event().await {
                    Some(eoka::cdp::transport::CdpMessage::Event { method, params, .. })
                        if method == "Fetch.requestPaused" =>
                    {
                        events.push(params);
                    }
                    Some(_) => {} // discard non-Fetch events
                    None => break,
                }
            }
            events
        };

        // Process collected events
        for params in events {
            let request_id = match params.get("requestId").and_then(|v| v.as_str()) {
                Some(id) => id.to_string(),
                None => continue,
            };
            let req_field = |name: &str| -> Option<String> {
                params.get("request")?.get(name)?.as_str().map(String::from)
            };
            let url = req_field("url").unwrap_or_default();
            let method = req_field("method").unwrap_or_else(|| "GET".to_string());
            let post_data = req_field("postData");

            // Match rule and extract what we need before borrowing session
            let matched = self.intercept.match_url(&url).map(|r| {
                (
                    r.id,
                    r.capture_path.clone(),
                    r.respond_path.clone(),
                    r.respond_status,
                )
            });

            let session = match self
                .state
                .as_ref()
                .and_then(|s| s.current_tab())
                .map(|t| t.page.session())
            {
                Some(s) => s,
                None => continue,
            };

            if let Some((rule_id, capture_path, respond_path, respond_status)) = matched {
                if let Some(ref path) = capture_path {
                    let body = json!({
                        "url": &url,
                        "method": &method,
                        "postData": &post_data,
                        "headers": params.get("request").and_then(|r| r.get("headers")),
                    });
                    let _ = std::fs::write(
                        path,
                        serde_json::to_string_pretty(&body).unwrap_or_default(),
                    );
                }

                let action = if let Some(ref path) = respond_path {
                    if let Ok(body) = std::fs::read(path) {
                        let body_str = String::from_utf8_lossy(&body);
                        let _ = session
                            .fetch_fulfill(&request_id, respond_status, None, Some(&body_str))
                            .await;
                        "responded"
                    } else {
                        let _ = session
                            .fetch_continue(&request_id, None, None, None, None)
                            .await;
                        "continue (respond file not found)"
                    }
                } else {
                    let _ = session
                        .fetch_continue(&request_id, None, None, None, None)
                        .await;
                    "continue (captured)"
                };

                self.intercept.add_log(InterceptLogEntry {
                    rule_id,
                    url: url.clone(),
                    method: method.clone(),
                    has_body: post_data.is_some(),
                    action: action.to_string(),
                });
            } else {
                let _ = session
                    .fetch_continue(&request_id, None, None, None, None)
                    .await;
            }
        }
    }

    // ── Close ───────────────────────────────────────────────────────────

    async fn cmd_close(&mut self) -> Result<Response, String> {
        if let Some(state) = self.state.take() {
            state.close().await.map_err(|e| e.to_string())?;
        }
        Ok(Response::ok_text("Browser closed"))
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn resolve_js(args: &Value) -> Result<String, String> {
    if let Some(path) = args["file"].as_str() {
        std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read JS file '{}': {}", path, e))
    } else if let Some(code) = args["code"].as_str() {
        Ok(code.to_string())
    } else {
        Err("Either 'code' or '--file' must be provided".into())
    }
}

/// Parse an address string (supports 0x hex prefix or decimal).
fn parse_addr(s: &str) -> Result<usize, String> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        usize::from_str_radix(hex, 16).map_err(|e| format!("Invalid hex address '{}': {}", s, e))
    } else {
        s.parse()
            .map_err(|e| format!("Invalid address '{}': {}", s, e))
    }
}

/// Build JS expression to access WASM memory. Auto-detects if not specified.
fn wasm_memory_expr(memory: Option<&str>) -> String {
    match memory {
        Some(expr) => expr.to_string(),
        None => {
            // Auto-detect: try common paths
            r#"(window.__ft?.mem || window.Module?.wasmMemory || window.Module?.HEAPU8?.buffer && {buffer: window.Module.HEAPU8.buffer} || (() => { for (const k of Object.keys(window)) { try { if (window[k] instanceof WebAssembly.Memory) return window[k]; } catch(e){} } return null; })())"#.to_string()
        }
    }
}

/// Convert hex string to JS byte array literal: "deadbeef" → "0xde,0xad,0xbe,0xef"
fn hex_to_byte_array(hex: &str) -> String {
    hex.as_bytes()
        .chunks(2)
        .map(|pair| format!("0x{}{}", pair[0] as char, pair[1] as char))
        .collect::<Vec<_>>()
        .join(",")
}
