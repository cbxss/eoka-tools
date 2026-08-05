mod eval;
pub mod intercept;
pub mod network;
mod persist;
pub mod profile;
mod proxy_forward;
pub mod script_policy;
pub mod state;
mod target;
mod wasm;

use std::collections::HashMap;
use std::fmt::Write as FmtWrite;

use base64::Engine;
use captcha::{build_captcha_inject_js, parse_captcha_inject_kind};
use eoka::cdp::{transport::CdpMessage, Session as CdpSession};
use eoka_mcp::{annotate, observe, snapshot};
use serde_json::{json, Value};

use crate::launch_spec::LaunchSpec;
use crate::protocol::{Request, Response};
use intercept::{InterceptLogEntry, InterceptRule, InterceptState};
use network::{NetworkConfig, NetworkRecorder};
use persist::{capture_state, ensure_console_capture, restore_cookies, restore_state, SavedState};
use script_policy::{ScriptPolicyMode, ScriptPolicyState};
use state::{BrowserState, TabState};
use target::{
    auto_observe_if_needed, click_with_retry, fill_with_retry, json_str, resolve_target,
    title_nonblocking, wait_for_stable,
};

pub struct Handler {
    state: Option<BrowserState>,
    spec: LaunchSpec,
    session_name: String,
    intercept: InterceptState,
    script_policy: ScriptPolicyState,
    network: Option<NetworkRecorder>,
    fetch_events: Option<tokio::sync::broadcast::Receiver<CdpMessage>>,
    fetch_sessions: HashMap<String, CdpSession>,
}

impl Handler {
    pub fn new(session_name: impl Into<String>, spec: LaunchSpec) -> Self {
        let script_policy = match &spec {
            LaunchSpec::Launch {
                no_js,
                js_allow,
                js_block,
                ..
            } => ScriptPolicyState::new(
                if *no_js {
                    ScriptPolicyMode::BlockAll
                } else {
                    ScriptPolicyMode::AllowAll
                },
                js_allow.clone(),
                js_block.clone(),
            ),
            LaunchSpec::Connect { .. } => ScriptPolicyState::default(),
        };
        Self {
            state: None,
            spec,
            session_name: session_name.into(),
            intercept: InterceptState::new(),
            script_policy,
            network: None,
            fetch_events: None,
            fetch_sessions: HashMap::new(),
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
                proxy,
                ..
            } => {
                let mut s = BrowserState::launched(
                    *headless,
                    from_profile.as_deref(),
                    *no_stealth,
                    proxy.clone(),
                )
                .await
                .map_err(|e| e.to_string())?;

                if let Some(source) = clone_state_from {
                    let saved = persist::clone_state_from_source(source).await?;
                    let url = saved.url.clone();
                    let tab = if !url.is_empty() {
                        s.ensure_tab(&url).await.map_err(|e| e.to_string())?
                    } else {
                        s.ensure_blank_tab().await.map_err(|e| e.to_string())?
                    };
                    let _ = wait_for_stable(&tab.page).await;
                    restore_state_and_maybe_reload(tab, &saved, state_url_can_reload(&url)).await?;
                }
                s
            }
        };

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
        self.drain_fetch_events().await;
        let response = match self.dispatch(cmd, args).await {
            Ok(resp) => resp,
            Err(e) => Response::err(e),
        };
        self.drain_fetch_events().await;
        response
    }

    pub async fn handle_request(&mut self, request: Request) -> Response {
        let cmd = request.cmd();
        let args = request.args_json();
        self.handle(cmd, &args).await
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
            "emulate" => self.cmd_emulate(args).await,
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
            "captcha_inject" => self.cmd_captcha_inject(args).await,
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
            "js_mode" => self.cmd_js_mode(args).await,
            "js_allow" => self.cmd_js_allow(args).await,
            "js_block" => self.cmd_js_block(args).await,
            "js_remove" => self.cmd_js_remove(args).await,
            "js_list" => self.cmd_js_list().await,
            "network_record_start" => self.cmd_network_record_start(args).await,
            "network_record_stop" => self.cmd_network_record_stop().await,
            "network_record_status" => self.cmd_network_record_status().await,
            "network_log" => self.cmd_network_log(args).await,
            "network_show" => self.cmd_network_show(args).await,
            "network_save_har" => self.cmd_network_save_har(args).await,
            "network_export" => self.cmd_network_export(args).await,
            "network_wait" => self.cmd_network_wait(args).await,
            "network_clear" => self.cmd_network_clear().await,
            "close" => self.cmd_close().await,
            other => Err(format!("Unknown command: {}", other)),
        }
    }

    fn arg_str<'a>(&self, args: &'a Value, key: &str) -> Result<&'a str, String> {
        args[key]
            .as_str()
            .ok_or_else(|| format!("Missing '{}'", key))
    }

    fn tab_with_config(&mut self) -> Result<(&mut TabState, bool), String> {
        let state = self.require_state_mut()?;
        let vp = state.config.viewport_only;
        let tab = state
            .current_tab_mut()
            .ok_or("No tab open. Use 'open' first.")?;
        Ok((tab, vp))
    }

    async fn cmd_open(&mut self, args: &Value) -> Result<Response, String> {
        let requested_url = self.arg_str(args, "url")?.to_string();
        let open_state = args["load_state"]
            .as_str()
            .map(read_saved_state_file)
            .transpose()?;
        let url = resolve_open_url_for_state(&requested_url, open_state.as_ref())?;
        let state_init_js = open_state
            .as_ref()
            .filter(|saved| state_should_prime_open(saved, &url))
            .and_then(build_storage_seed_js);
        self.ensure_browser().await?;

        let headers: Option<HashMap<String, String>> = args
            .get("headers")
            .filter(|v| !v.is_null())
            .and_then(|v| serde_json::from_value(v.clone()).ok());
        let user_agent = args["user_agent"].as_str();
        let bypass_csp = args["bypass_csp"].as_bool().unwrap_or(false);
        let inject_js = args["inject_js"].as_str();
        let script_enabled = self.script_policy.is_active().then(|| {
            // No host (data:, about:blank, ...) can't match any allow/block
            // entry, so this falls through to the mode's own default —
            // never silently allows JS under block-all.
            let host = url::Url::parse(&url)
                .ok()
                .and_then(|parsed| parsed.host_str().map(str::to_owned));
            self.script_policy.resolve(host.as_deref().unwrap_or(""))
        });
        let has_extras = headers.is_some()
            || user_agent.is_some()
            || bypass_csp
            || inject_js.is_some()
            || open_state.is_some()
            || state_init_js.is_some()
            || script_enabled.is_some();

        let drain = self.start_fetch_drain();
        let active_recorder = self.network.clone();
        let result = async {
            let state = self.require_state_mut()?;

            if state.is_live {
                state.new_tab(None).await.map_err(|e| e.to_string())?;
                if let Some(recorder) = active_recorder.as_ref() {
                    if recorder.is_active().await {
                        let session = state.current_tab().ok_or("No tab")?.page.session().clone();
                        recorder.update_session(&session).await?;
                    }
                }
            }

            if has_extras {
                let tab = if state.current_tab_id.is_some() {
                    state.current_tab_mut().ok_or("No tab")?
                } else {
                    state.ensure_blank_tab().await.map_err(|e| e.to_string())?
                };
                if let Some(saved) = open_state.as_ref() {
                    restore_cookies(&tab.page, saved).await?;
                }
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
                let state_script_id = if let Some(source) = state_init_js.as_deref() {
                    let response: Value = tab
                        .page
                        .session()
                        .send(
                            "Page.addScriptToEvaluateOnNewDocument",
                            &json!({ "source": source }),
                        )
                        .await
                        .map_err(|e| e.to_string())?;
                    response
                        .get("identifier")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                } else {
                    None
                };
                if let Some(enabled) = script_enabled {
                    tab.page
                        .set_javascript_enabled(enabled)
                        .await
                        .map_err(|e| e.to_string())?;
                }
                tab.invalidate();
                let nav_result = tab.page.goto(&url).await;
                if let Some(identifier) = state_script_id {
                    let _ = tab
                        .page
                        .session()
                        .send::<_, Value>(
                            "Page.removeScriptToEvaluateOnNewDocument",
                            &json!({ "identifier": identifier }),
                        )
                        .await;
                }
                if headers.is_some() {
                    let _ = tab.page.clear_extra_headers().await;
                }
                nav_result.map_err(|e| e.to_string())?;
            } else {
                state.ensure_tab(&url).await.map_err(|e| e.to_string())?;
            }

            let tab = state.current_tab_mut().ok_or("No tab after navigate")?;
            let _ = wait_for_stable(&tab.page).await;
            let url = tab.page.url().await.map_err(|e| e.to_string())?;
            let title = title_nonblocking(&tab.page).await;
            Ok::<_, String>((url, title))
        }
        .await;
        self.stop_fetch_drain(drain).await;
        if let Some(enabled) = script_enabled {
            self.script_policy.note_applied(enabled);
        }
        let (url, title) = result?;
        self.sync_current_target_features().await?;
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
        let page = self.require_tab()?.page.clone();
        let drain = self.start_fetch_drain();
        let result = async {
            f(&page).await.map_err(|e| e.to_string())?;
            let _ = wait_for_stable(&page).await;
            page.url().await.map_err(|e| e.to_string())
        }
        .await;
        self.stop_fetch_drain(drain).await;
        let url = result?;
        Ok(Response::ok_text(format!("{} to: {}", label, url)))
    }

    async fn cmd_reload(&mut self) -> Result<Response, String> {
        let page = self.require_tab()?.page.clone();
        // Re-resolve the script policy against the current URL before
        // reloading, so a policy change (e.g. `eoka js allow`) since the
        // last navigation takes effect without a full re-open.
        let script_enabled = if self.script_policy.is_active() {
            let current_url = page.url().await.map_err(|e| e.to_string())?;
            let host = url::Url::parse(&current_url)
                .ok()
                .and_then(|parsed| parsed.host_str().map(str::to_owned));
            Some(self.script_policy.resolve(host.as_deref().unwrap_or("")))
        } else {
            None
        };
        let drain = self.start_fetch_drain();
        let result = async {
            if let Some(enabled) = script_enabled {
                page.set_javascript_enabled(enabled)
                    .await
                    .map_err(|e| e.to_string())?;
            }
            page.reload().await.map_err(|e| e.to_string())?;
            let _ = wait_for_stable(&page).await;
            page.url().await.map_err(|e| e.to_string())
        }
        .await;
        self.stop_fetch_drain(drain).await;
        if let Some(enabled) = script_enabled {
            self.script_policy.note_applied(enabled);
        }
        let url = result?;
        if let Ok(tab) = self.require_tab_mut() {
            tab.invalidate();
        }
        Ok(Response::ok_text(format!("Reloaded: {}", url)))
    }

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

        let filter_fn: fn(&&eoka_mcp::InteractiveElement) -> bool = match filter {
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

    async fn cmd_emulate(&mut self, args: &Value) -> Result<Response, String> {
        self.ensure_browser().await?;
        let reset = args["reset"].as_bool().unwrap_or(false);
        let tab = self.require_tab_mut()?;
        let session = tab.page.session();

        if reset {
            session
                .send::<_, Value>("Emulation.clearDeviceMetricsOverride", &json!({}))
                .await
                .map_err(|e| e.to_string())?;
            let _ = session
                .send::<_, Value>(
                    "Emulation.setTouchEmulationEnabled",
                    &json!({ "enabled": false }),
                )
                .await;
            return Ok(Response::ok_text("Device emulation cleared"));
        }

        let width = args["width"].as_u64().unwrap_or(390);
        let height = args["height"].as_u64().unwrap_or(844);
        let dpr = args["dpr"].as_f64().unwrap_or(2.0);
        let mobile = !args["desktop"].as_bool().unwrap_or(false);

        session
            .send::<_, Value>(
                "Emulation.setDeviceMetricsOverride",
                &json!({
                    "width": width,
                    "height": height,
                    "deviceScaleFactor": dpr,
                    "mobile": mobile,
                }),
            )
            .await
            .map_err(|e| e.to_string())?;

        let _ = session
            .send::<_, Value>(
                "Emulation.setTouchEmulationEnabled",
                &json!({ "enabled": mobile, "maxTouchPoints": 5 }),
            )
            .await;

        Ok(Response::ok(json!({
            "width": width,
            "height": height,
            "dpr": dpr,
            "mobile": mobile,
        })))
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

    async fn cmd_click(&mut self, args: &Value) -> Result<Response, String> {
        self.ensure_browser().await?;
        let target_str = self.arg_str(args, "target")?;
        let drain = self.start_fetch_drain();
        let result = async {
            let (tab, vp) = self.tab_with_config()?;
            auto_observe_if_needed(tab, target_str, vp).await?;
            let desc = click_with_retry(tab, target_str, vp).await?;
            let _ = wait_for_stable(&tab.page).await;
            tab.elements.clear();
            Ok::<_, String>(desc)
        }
        .await;
        self.stop_fetch_drain(drain).await;
        let desc = result?;
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
        let drain = self.start_fetch_drain();
        let result = async {
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
            Ok::<_, String>(resolved.desc)
        }
        .await;
        self.stop_fetch_drain(drain).await;
        let desc = result?;
        Ok(Response::ok_text(format!(
            "Selected \"{}\" in {}",
            value, desc
        )))
    }

    async fn cmd_hover(&mut self, args: &Value) -> Result<Response, String> {
        let target_str = self.arg_str(args, "target")?;
        let (tab, vp) = self.tab_with_config()?;

        auto_observe_if_needed(tab, target_str, vp).await?;
        let resolved = resolve_target(tab, target_str).await?;
        tab.page
            .hover(&resolved.selector)
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

    async fn cmd_captcha_inject(&mut self, args: &Value) -> Result<Response, String> {
        self.ensure_browser().await?;
        let token = self.arg_str(args, "token")?;
        let captcha_type = args["captcha_type"].as_str().unwrap_or("auto");
        let callback = args["callback"].as_str();
        let click_after = args["click_after"].as_str().map(str::to_string);
        let kind = parse_captcha_inject_kind(captcha_type).map_err(|error| error.to_string())?;
        let js = build_captcha_inject_js(token, kind, callback);
        let result: String = {
            let tab = self.require_tab()?;
            tab.page.evaluate(&js).await.map_err(|e| e.to_string())?
        };
        let mut parsed: Value = serde_json::from_str(&result).map_err(|e| e.to_string())?;
        if let Some(target) = click_after {
            let (tab, viewport_only) = self.tab_with_config()?;
            let desc = click_with_retry(tab, &target, viewport_only).await?;
            if let Value::Object(ref mut object) = parsed {
                object.insert(
                    "click_after".into(),
                    json!({ "target": target, "clicked": desc }),
                );
            }
        } else if let Value::Object(ref mut object) = parsed {
            object.insert(
                "next_action".into(),
                Value::String(
                    "If the page does not advance after callbacks run, click the continuation control again or pass --click-after.".into(),
                ),
            );
        }
        Ok(Response::ok(parsed))
    }

    async fn cmd_fetch(&mut self, args: &Value) -> Result<Response, String> {
        let url = self.arg_str(args, "url")?;
        let method = args["method"].as_str().unwrap_or("GET");
        let redirect = args["redirect"].as_str().unwrap_or("follow");
        let body_only = args["body_only"].as_bool().unwrap_or(false);
        let max_body = args["max_body"]
            .as_u64()
            .map(|v| v as usize)
            .unwrap_or(if body_only { 0 } else { 8192 });

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
        if body_only {
            return Ok(Response::ok_text(fetch_body_only_text(&parsed)?));
        }
        Ok(Response::ok(parsed))
    }

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
                .map_err(|e| e.to_string())?,
        };

        let cookie = eoka::SessionCookie {
            name: name.to_string(),
            value: value.to_string(),
            domain,
            path: path.to_string(),
            secure: false,
            http_only: false,
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
                .map_err(|e| e.to_string())?,
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
            Ok(Response::ok(eval_json_or_string(&tab.page, &js).await?))
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
        let js = "JSON.stringify({\
                    localStorage: Object.fromEntries(Object.entries(localStorage)),\
                    sessionStorage: Object.fromEntries(Object.entries(sessionStorage))\
                })";
        Ok(Response::ok(eval_json_or_string(&tab.page, js).await?))
    }

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

        let saved = read_saved_state_file(path)?;

        self.ensure_browser().await?;
        let state = self.require_state_mut()?;

        if !no_navigate && !saved.url.is_empty() {
            let tab = state
                .ensure_tab(&saved.url)
                .await
                .map_err(|e| e.to_string())?;
            let _ = wait_for_stable(&tab.page).await;
        }

        let tab = state.current_tab_mut().ok_or("No tab open.")?;
        let reload_after_restore = if !no_navigate && !saved.url.is_empty() {
            state_url_can_reload(&saved.url)
        } else {
            page_url_can_reload_state(&tab.page).await
        };
        restore_state_and_maybe_reload(tab, &saved, reload_after_restore).await?;

        Ok(Response::ok_text(format!(
            "Loaded state from {} ({} cookies, {} localStorage, {} sessionStorage)",
            path,
            saved.cookies.len(),
            saved.local_storage.len(),
            saved.session_storage.len()
        )))
    }

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
        let parsed = eval_json_or_string(&tab.page, &js).await?;
        if clear {
            // `x = []` evaluates to the array, not a string, so this
            // routinely fails to deserialize as String even though the
            // clear itself succeeded — best-effort by design, not a real
            // failure to surface.
            let _: String = tab
                .page
                .evaluate_sync("__eoka_console = []")
                .await
                .unwrap_or_default();
        }
        Ok(Response::ok(parsed))
    }

    async fn cmd_errors(&mut self, args: &Value) -> Result<Response, String> {
        let clear = args["clear"].as_bool().unwrap_or(false);
        let tab = self.require_tab_mut()?;
        ensure_console_capture(tab).await?;

        let parsed = eval_json_or_string(&tab.page, "JSON.stringify(__eoka_errors || [])").await?;
        if clear {
            let _: String = tab
                .page
                .evaluate_sync("__eoka_errors = []")
                .await
                .unwrap_or_default();
        }
        Ok(Response::ok(parsed))
    }

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
        let active_recorder = self.network.clone();
        let state = self.require_state_mut()?;
        let (tab_id, url, title) = {
            let record_first_navigation = match active_recorder.as_ref() {
                Some(recorder) => recorder.is_active().await,
                None => false,
            };
            let (tab_id, tab) = if let (Some(url), true) = (url, record_first_navigation) {
                let (tab_id, tab) = state.new_tab(None).await.map_err(|e| e.to_string())?;
                let session = tab.page.session().clone();
                if let Some(recorder) = active_recorder.as_ref() {
                    recorder.update_session(&session).await?;
                }
                tab.page.goto(url).await.map_err(|e| e.to_string())?;
                (tab_id, tab)
            } else {
                state.new_tab(url).await.map_err(|e| e.to_string())?
            };
            let (url, title) = tab_summary(tab).await?;
            (tab_id, url, title)
        };
        self.sync_current_target_features().await?;
        Ok(Response::ok_text(format!(
            "Opened new tab [{}]\nURL: {}\nTitle: {}",
            tab_id, url, title
        )))
    }

    async fn cmd_tab_switch(&mut self, args: &Value) -> Result<Response, String> {
        let tab_id = self.arg_str(args, "tab_id")?;
        let state = self.require_state_mut()?;
        state.switch_tab(tab_id).await.map_err(|e| e.to_string())?;
        let (url, title) = {
            let tab = state.current_tab().ok_or("Tab switch failed")?;
            tab_summary(tab).await?
        };
        self.sync_current_target_features().await?;
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
        let (url, title) = {
            let tab = state.current_tab().ok_or("Attach failed")?;
            tab_summary(tab).await?
        };
        self.sync_current_target_features().await?;
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

        self.ensure_browser().await?;
        let state = self.require_state_mut()?;
        let url = saved.url.clone();
        let tab = if !url.is_empty() {
            state.ensure_tab(&url).await.map_err(|e| e.to_string())?
        } else {
            state.ensure_blank_tab().await.map_err(|e| e.to_string())?
        };
        let _ = wait_for_stable(&tab.page).await;
        restore_state_and_maybe_reload(tab, &saved, state_url_can_reload(&url)).await?;
        Ok(Response::ok_text(format!(
            "Hydrated session from {} ({} cookies, {} localStorage, {} sessionStorage)",
            source,
            saved.cookies.len(),
            saved.local_storage.len(),
            saved.session_storage.len()
        )))
    }

    async fn cmd_tab_close(&mut self, args: &Value) -> Result<Response, String> {
        let tab_id = self.arg_str(args, "tab_id")?.to_string();
        let closing_session = self
            .state
            .as_ref()
            .and_then(|s| s.tabs.get(&tab_id))
            .map(|t| t.page.session().clone());
        let state = self.require_state_mut()?;
        state.close_tab(&tab_id).await.map_err(|e| e.to_string())?;
        if let Some(session) = closing_session {
            self.disable_fetch_session(&session).await;
        }
        self.sync_current_target_features().await?;
        Ok(Response::ok_text(format!("Closed tab [{}]", tab_id)))
    }

    async fn cmd_wait(&mut self, args: &Value) -> Result<Response, String> {
        if let Some(ms) = args["ms"].as_u64() {
            let ms = ms.min(30_000);
            tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
            return Ok(Response::ok_text(format!("Waited {}ms", ms)));
        }
        let timeout = args["timeout"].as_u64().unwrap_or(10_000);
        let page = self.require_tab()?.page.clone();
        let drain = self.start_fetch_drain();

        let result = async {
            if let Some(text) = args["text"].as_str() {
                page.wait_for_text(text, timeout)
                    .await
                    .map_err(|e| e.to_string())?;
                return Ok(Response::ok_text(format!("Text \"{}\" appeared", text)));
            }
            if let Some(url_pat) = args["url"].as_str() {
                page.wait_for_url_contains(url_pat, timeout)
                    .await
                    .map_err(|e| e.to_string())?;
                return Ok(Response::ok_text(format!("URL matched \"{}\"", url_pat)));
            }
            if args["load"].as_str().is_some() {
                page.wait_for_network_idle(500, timeout)
                    .await
                    .map_err(|e| e.to_string())?;
                return Ok(Response::ok_text("Network idle"));
            }
            Err("Provide ms, --text, --url, or --load".into())
        }
        .await;
        self.stop_fetch_drain(drain).await;
        result
    }

    async fn cmd_spa_info(&mut self) -> Result<Response, String> {
        let tab = self.require_tab()?;
        let info = eoka_mcp::spa::detect_router(&tab.page)
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
        let drain = self.start_fetch_drain();
        let result = async {
            let tab = self.require_tab_mut()?;
            let info = eoka_mcp::spa::detect_router(&tab.page)
                .await
                .map_err(|e: eoka::Error| e.to_string())?;
            eoka_mcp::spa::spa_navigate(&tab.page, &info.router_type, path)
                .await
                .map_err(|e: eoka::Error| e.to_string())?;
            let _ = wait_for_stable(&tab.page).await;
            tab.invalidate();
            tab.page.url().await.map_err(|e| e.to_string())
        }
        .await;
        self.stop_fetch_drain(drain).await;
        let url = result?;
        Ok(Response::ok_text(format!("SPA navigated to: {}", url)))
    }

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

        tab.page
            .session()
            .send::<_, Value>(
                "Page.addScriptToEvaluateOnNewDocument",
                &json!({ "source": js }),
            )
            .await
            .map_err(|e| e.to_string())?;

        // fake_camera.js is a side-effecting IIFE with no return value, so
        // evaluate_sync::<String> routinely "fails" to deserialize a String
        // even when the injection succeeded. addScriptToEvaluateOnNewDocument
        // above is what needs to succeed; this just seeds the current page too.
        let _: String = tab.page.evaluate_sync(&js).await.unwrap_or_default();

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

    async fn disable_fetch_session(&mut self, session: &CdpSession) {
        let session_id = session.session_id().to_string();
        let _ = session.send::<_, Value>("Fetch.disable", &json!({})).await;
        self.fetch_sessions.remove(&session_id);
        if self.fetch_sessions.is_empty() {
            self.intercept.enabled = false;
            self.fetch_events = None;
        }
    }

    async fn disable_stale_fetch_sessions(&mut self, keep_session_id: Option<&str>) {
        let stale: Vec<CdpSession> = self
            .fetch_sessions
            .iter()
            .filter(|(id, _)| Some(id.as_str()) != keep_session_id)
            .map(|(_, session)| session.clone())
            .collect();

        for session in stale {
            self.disable_fetch_session(&session).await;
        }
    }

    async fn disable_all_fetch_sessions(&mut self) {
        self.disable_stale_fetch_sessions(None).await;
        self.fetch_sessions.clear();
        self.intercept.enabled = false;
        self.fetch_events = None;
    }

    fn fetch_command_transport(&self) -> Option<std::sync::Arc<eoka::cdp::Transport>> {
        self.fetch_sessions
            .values()
            .next()
            .map(|session| session.transport().clone())
            .or_else(|| {
                self.state
                    .as_ref()
                    .and_then(|state| state.current_tab())
                    .map(|tab| tab.page.session().transport().clone())
            })
    }

    fn current_fetch_session_id(&self) -> Option<&str> {
        self.fetch_sessions.keys().next().map(String::as_str)
    }

    fn fetch_drain_config(&self) -> Option<FetchDrainConfig> {
        Some(FetchDrainConfig {
            transport: self.fetch_command_transport()?,
            fallback_session_id: self.current_fetch_session_id().map(str::to_string),
            rules: self.intercept.rules_snapshot(),
        })
    }

    fn start_fetch_drain(&mut self) -> Option<FetchDrainHandle> {
        if !self.intercept.enabled {
            return None;
        }
        let config = self.fetch_drain_config()?;
        let rx = self.fetch_events.take()?;
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
        let join = tokio::spawn(fetch_drain_until_stopped(rx, config, stop_rx));
        Some(FetchDrainHandle {
            stop: stop_tx,
            join,
        })
    }

    async fn stop_fetch_drain(&mut self, handle: Option<FetchDrainHandle>) {
        let Some(handle) = handle else {
            return;
        };
        let _ = handle.stop.send(());
        match handle.join.await {
            Ok((rx, logs)) => {
                if self.fetch_events.is_none() && self.intercept.enabled {
                    self.fetch_events = Some(rx);
                }
                for log in logs {
                    self.intercept.add_log(log);
                }
            }
            Err(_) => {
                self.fetch_events = None;
            }
        }
    }

    async fn sync_fetch_interception(&mut self) -> Result<(), String> {
        if self.intercept.is_empty() {
            self.disable_all_fetch_sessions().await;
            return Ok(());
        }

        self.ensure_browser().await?;
        let patterns = self.intercept.fetch_patterns_json();
        let session = {
            let tab = self.require_tab()?;
            tab.page.session().clone()
        };
        let session_id = session.session_id().to_string();
        self.disable_stale_fetch_sessions(Some(&session_id)).await;
        session
            .send::<_, serde_json::Value>(
                "Fetch.enable",
                &json!({
                    "patterns": patterns,
                    "handleAuthRequests": false,
                }),
            )
            .await
            .map_err(|e| e.to_string())?;
        self.intercept.enabled = true;
        if self.fetch_events.is_none() {
            self.fetch_events = Some(session.transport().subscribe());
        }
        self.fetch_sessions.insert(session_id, session);
        Ok(())
    }

    fn network_recorder(&mut self) -> Result<NetworkRecorder, String> {
        if let Some(recorder) = self.network.clone() {
            return Ok(recorder);
        }
        let session = self.require_tab()?.page.session().clone();
        let recorder = NetworkRecorder::spawn(
            format!("session:{}", self.session_name),
            session.transport().clone(),
        );
        self.network = Some(recorder.clone());
        Ok(recorder)
    }

    async fn sync_network_recording(&mut self) -> Result<(), String> {
        let Some(recorder) = self.network.clone() else {
            return Ok(());
        };
        if !recorder.is_active().await {
            return Ok(());
        }
        let session = self.require_tab()?.page.session().clone();
        recorder.update_session(&session).await
    }

    async fn sync_current_target_features(&mut self) -> Result<(), String> {
        self.sync_fetch_interception().await?;
        self.sync_network_recording().await
    }

    pub async fn is_network_recording(&self) -> bool {
        match self.network.as_ref() {
            Some(recorder) => recorder.is_active().await,
            None => false,
        }
    }

    async fn cmd_network_record_start(&mut self, args: &Value) -> Result<Response, String> {
        self.ensure_browser().await?;
        if self
            .state
            .as_ref()
            .and_then(|state| state.current_tab())
            .is_none()
        {
            self.require_state_mut()?
                .ensure_blank_tab()
                .await
                .map_err(|e| e.to_string())?;
        }
        let patterns = args
            .get("patterns")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .filter(|patterns| !patterns.is_empty())
            .unwrap_or_else(|| NetworkConfig::default().patterns);
        let max_body_bytes = args["max_body_bytes"]
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or_else(|| NetworkConfig::default().max_body_bytes);
        let clear = args["clear"].as_bool().unwrap_or(false);
        let config = NetworkConfig {
            patterns,
            capture_bodies: !args["no_bodies"].as_bool().unwrap_or(false),
            max_body_bytes,
            ..NetworkConfig::default()
        };
        let session = self.require_tab()?.page.session().clone();
        let recorder = self.network_recorder()?;
        if clear {
            let _ = recorder.clear().await?;
        }
        let status = recorder.start(&session, config).await?;
        Ok(Response::ok(status))
    }

    async fn cmd_network_record_stop(&mut self) -> Result<Response, String> {
        let Some(recorder) = self.network.clone() else {
            return Ok(Response::ok(json!({ "active": false })));
        };
        let session = self
            .state
            .as_ref()
            .and_then(|state| state.current_tab())
            .map(|tab| tab.page.session().clone());
        Ok(Response::ok(recorder.stop(session).await?))
    }

    async fn cmd_network_record_status(&mut self) -> Result<Response, String> {
        match self.network.as_ref() {
            Some(recorder) => Ok(Response::ok(recorder.status().await)),
            None => Ok(Response::ok(json!({
                "meta": self.network_empty_meta(),
                "status": {
                    "active": false,
                    "entry_count": 0,
                    "in_flight": 0,
                },
            }))),
        }
    }

    async fn cmd_network_log(&mut self, args: &Value) -> Result<Response, String> {
        let Some(recorder) = self.network.as_ref() else {
            return Ok(Response::ok(json!({
                "meta": self.network_empty_meta(),
                "entries": [],
                "count": 0,
                "filters": args,
            })));
        };
        let limit = args["limit"]
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(100)
            .min(1000);
        let pattern = args["pattern"].as_str();
        let method = args["method"].as_str();
        let status = args["status"]
            .as_u64()
            .and_then(|value| u16::try_from(value).ok());
        let since = args["since"].as_u64();
        let compact = args["compact"].as_bool().unwrap_or(false);
        Ok(Response::ok(
            recorder
                .log(limit, pattern, method, status, since, compact)
                .await,
        ))
    }

    async fn cmd_network_show(&mut self, args: &Value) -> Result<Response, String> {
        let id = args["id"].as_u64().ok_or("Missing 'id'")?;
        let body = args["body"].as_bool().unwrap_or(false);
        let max_body = args["max_body"]
            .as_u64()
            .and_then(|value| usize::try_from(value).ok());
        let Some(recorder) = self.network.as_ref() else {
            return Err(format!("Network entry #{} not found", id));
        };
        if body {
            recorder.settle(5000).await;
        }
        recorder
            .show(id, body, max_body)
            .await
            .map(Response::ok)
            .ok_or_else(|| format!("Network entry #{} not found", id))
    }

    async fn cmd_network_save_har(&mut self, args: &Value) -> Result<Response, String> {
        self.cmd_network_export(args).await
    }

    async fn cmd_network_export(&mut self, args: &Value) -> Result<Response, String> {
        let path = self.arg_str(args, "path")?;
        let format = args["format"].as_str().unwrap_or("har");
        let settle_ms = args["settle_ms"].as_u64();
        let Some(recorder) = self.network.as_ref() else {
            return Err("No network recorder has been started".into());
        };
        Ok(Response::ok(
            recorder
                .export(std::path::Path::new(path), format, settle_ms)
                .await?,
        ))
    }

    async fn cmd_network_wait(&mut self, args: &Value) -> Result<Response, String> {
        let Some(recorder) = self.network.as_ref() else {
            return Err("No network recorder has been started".into());
        };
        let timeout = args["timeout"].as_u64().unwrap_or(10_000);
        let pattern = args["pattern"].as_str();
        let method = args["method"].as_str();
        let status = args["status"]
            .as_u64()
            .and_then(|value| u16::try_from(value).ok());
        let since = args["since"].as_u64();
        let include_existing = args["include_existing"].as_bool().unwrap_or(false);
        recorder
            .wait(pattern, method, status, since, include_existing, timeout)
            .await
            .map(Response::ok)
            .ok_or_else(|| format!("Timed out waiting {}ms for matching network entry", timeout))
    }

    async fn cmd_network_clear(&mut self) -> Result<Response, String> {
        match self.network.as_ref() {
            Some(recorder) => Ok(Response::ok(recorder.clear().await?)),
            None => Ok(Response::ok(json!({
                "meta": self.network_empty_meta(),
                "cleared": true,
            }))),
        }
    }

    fn network_empty_meta(&self) -> Value {
        json!({
            "namespace": format!("session:{}", self.session_name),
            "active": false,
            "entry_count": 0,
            "in_flight": 0,
            "last_id": 0,
            "next_since": 0,
            "body_pending": 0,
            "body_failed": 0,
            "warnings": [],
            "suggested_commands": [
                "eoka network record start",
                "eoka network log --since 0 --compact",
            ],
        })
    }

    async fn cmd_intercept_add(&mut self, args: &Value) -> Result<Response, String> {
        let url_pattern = self.arg_str(args, "url_pattern")?.to_string();
        let capture = args["capture"].as_str().map(std::path::PathBuf::from);
        let respond = args["respond"].as_str().map(std::path::PathBuf::from);
        let status = args["status"].as_u64().unwrap_or(200) as u16;

        let id = self
            .intercept
            .add_rule(url_pattern.clone(), capture, respond, status);
        self.sync_fetch_interception().await?;

        Ok(Response::ok_text(format!(
            "Added intercept rule #{} for \"{}\"",
            id, url_pattern
        )))
    }

    async fn cmd_intercept_list(&mut self) -> Result<Response, String> {
        self.drain_fetch_events().await;
        Ok(Response::ok(self.intercept.list_json()))
    }

    async fn cmd_intercept_remove(&mut self, args: &Value) -> Result<Response, String> {
        let id = self.arg_str(args, "id")?;
        if id == "all" {
            self.intercept.remove_all();
            self.sync_fetch_interception().await?;
            Ok(Response::ok_text("Removed all intercept rules"))
        } else {
            let id_num: usize = id.parse().map_err(|_| "Invalid rule ID")?;
            if self.intercept.remove_rule(id_num) {
                self.sync_fetch_interception().await?;
                Ok(Response::ok_text(format!("Removed rule #{}", id_num)))
            } else {
                Err(format!("Rule #{} not found", id_num))
            }
        }
    }

    async fn cmd_intercept_log(&mut self, args: &Value) -> Result<Response, String> {
        self.drain_fetch_events().await;
        if let Some(recorder) = self.network.clone() {
            self.intercept
                .resolve_network_links(|session_id, network_id, url, method| {
                    let recorder = recorder.clone();
                    let session_id = session_id.map(str::to_string);
                    let network_id = network_id.map(str::to_string);
                    let url = url.to_string();
                    let method = method.to_string();
                    async move {
                        recorder
                            .entry_id_for_network(
                                session_id.as_deref(),
                                network_id.as_deref(),
                                &url,
                                &method,
                            )
                            .await
                    }
                })
                .await;
        }
        let clear = args["clear"].as_bool().unwrap_or(false);
        let result = self.intercept.log_json();
        if clear {
            self.intercept.clear_log();
        }
        Ok(Response::ok(result))
    }

    async fn drain_fetch_events(&mut self) {
        if !self.intercept.enabled {
            return;
        }

        let config = match self.fetch_drain_config() {
            Some(config) => config,
            None => return,
        };
        let mut logs = Vec::new();
        let closed = {
            let rx = match self.fetch_events.as_mut() {
                Some(rx) => rx,
                None => return,
            };
            drain_fetch_receiver(rx, &config, &mut logs).await
        };
        if closed {
            self.fetch_events = None;
            return;
        }

        for log in logs {
            self.intercept.add_log(log);
        }
    }

    async fn cmd_close(&mut self) -> Result<Response, String> {
        if let Some(recorder) = self.network.clone() {
            let session = self
                .state
                .as_ref()
                .and_then(|state| state.current_tab())
                .map(|tab| tab.page.session().clone());
            let _ = recorder.stop(session).await;
        }
        self.disable_all_fetch_sessions().await;
        if let Some(state) = self.state.take() {
            state.close().await.map_err(|e| e.to_string())?;
        }
        Ok(Response::ok_text("Browser closed"))
    }
}

/// Evaluate `js` and parse its string result as JSON, falling back to a JSON
/// string value if the result isn't valid JSON (e.g. a raw storage value).
async fn eval_json_or_string(page: &eoka::Page, js: &str) -> Result<Value, String> {
    let result: String = page.evaluate_sync(js).await.map_err(|e| e.to_string())?;
    Ok(serde_json::from_str(&result).unwrap_or(Value::String(result)))
}

/// Current URL and title of a tab, for tab-switching command responses.
async fn tab_summary(tab: &TabState) -> Result<(String, String), String> {
    let url = tab.page.url().await.map_err(|e| e.to_string())?;
    let title = title_nonblocking(&tab.page).await;
    Ok((url, title))
}

struct FetchPausedEvent {
    session_id: Option<String>,
    params: Value,
}

struct FetchDrainConfig {
    transport: std::sync::Arc<eoka::cdp::Transport>,
    fallback_session_id: Option<String>,
    rules: Vec<InterceptRule>,
}

struct FetchDrainHandle {
    stop: tokio::sync::oneshot::Sender<()>,
    join: tokio::task::JoinHandle<(
        tokio::sync::broadcast::Receiver<CdpMessage>,
        Vec<InterceptLogEntry>,
    )>,
}

async fn fetch_drain_until_stopped(
    mut rx: tokio::sync::broadcast::Receiver<CdpMessage>,
    config: FetchDrainConfig,
    mut stop_rx: tokio::sync::oneshot::Receiver<()>,
) -> (
    tokio::sync::broadcast::Receiver<CdpMessage>,
    Vec<InterceptLogEntry>,
) {
    let mut logs = Vec::new();
    loop {
        tokio::select! {
            _ = &mut stop_rx => {
                let _ = drain_fetch_receiver(&mut rx, &config, &mut logs).await;
                break;
            }
            _ = tokio::time::sleep(std::time::Duration::from_millis(25)) => {
                if drain_fetch_receiver(&mut rx, &config, &mut logs).await {
                    break;
                }
            }
        }
    }
    (rx, logs)
}

async fn drain_fetch_receiver(
    rx: &mut tokio::sync::broadcast::Receiver<CdpMessage>,
    config: &FetchDrainConfig,
    logs: &mut Vec<InterceptLogEntry>,
) -> bool {
    loop {
        match rx.try_recv() {
            Ok(message) if is_fetch_request_paused(&message) => {
                if let CdpMessage::Event {
                    params, session_id, ..
                } = message
                {
                    if let Some(log) =
                        process_fetch_paused_event(config, FetchPausedEvent { session_id, params })
                            .await
                    {
                        logs.push(log);
                    }
                }
            }
            Ok(_) => {}
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => continue,
            Err(tokio::sync::broadcast::error::TryRecvError::Empty) => return false,
            Err(tokio::sync::broadcast::error::TryRecvError::Closed) => return true,
        }
    }
}

async fn process_fetch_paused_event(
    config: &FetchDrainConfig,
    event: FetchPausedEvent,
) -> Option<InterceptLogEntry> {
    let params = event.params;
    let request_id = params
        .get("requestId")
        .and_then(|v| v.as_str())?
        .to_string();
    let req_field = |name: &str| -> Option<String> {
        params.get("request")?.get(name)?.as_str().map(String::from)
    };
    let url = req_field("url").unwrap_or_default();
    let method = req_field("method").unwrap_or_else(|| "GET".to_string());
    let post_data = req_field("postData");
    let network_id = params
        .get("networkId")
        .and_then(Value::as_str)
        .map(str::to_string);
    let session_id = fetch_command_session_id(
        event.session_id.as_deref(),
        config.fallback_session_id.as_deref(),
    )?;

    let matched = config.rules.iter().find(|rule| rule.matches_url(&url));
    let Some(rule) = matched else {
        let _ = config
            .transport
            .send_to_session::<_, serde_json::Value>(
                &session_id,
                "Fetch.continueRequest",
                &json!({ "requestId": request_id }),
            )
            .await;
        return None;
    };

    if let Some(ref path) = rule.capture_path {
        let body = json!({
            "url": &url,
            "method": &method,
            "postData": &post_data,
            "headers": params.get("request").and_then(|r| r.get("headers")),
        });
        match serde_json::to_string_pretty(&body) {
            Ok(json) => {
                if let Err(e) = std::fs::write(path, json) {
                    eprintln!(
                        "[eoka] intercept capture write failed ({}): {}",
                        path.display(),
                        e
                    );
                }
            }
            Err(e) => eprintln!("[eoka] intercept capture serialize failed: {}", e),
        }
    }

    let action = if let Some(ref path) = rule.respond_path {
        if let Ok(body) = std::fs::read(path) {
            let body_str = String::from_utf8_lossy(&body);
            let _ = config
                .transport
                .send_to_session::<_, serde_json::Value>(
                    &session_id,
                    "Fetch.fulfillRequest",
                    &json!({
                        "requestId": request_id,
                        "responseCode": rule.respond_status,
                        "body": base64::engine::general_purpose::STANDARD
                            .encode(body_str.as_bytes()),
                    }),
                )
                .await;
            "responded"
        } else {
            let _ = config
                .transport
                .send_to_session::<_, serde_json::Value>(
                    &session_id,
                    "Fetch.continueRequest",
                    &json!({ "requestId": request_id }),
                )
                .await;
            "continue (respond file not found)"
        }
    } else {
        let _ = config
            .transport
            .send_to_session::<_, serde_json::Value>(
                &session_id,
                "Fetch.continueRequest",
                &json!({ "requestId": request_id }),
            )
            .await;
        "continue (captured)"
    };

    Some(InterceptLogEntry {
        rule_id: rule.id,
        url,
        method,
        has_body: post_data.is_some(),
        action: action.to_string(),
        session_id: Some(session_id),
        network_id,
        network_entry_id: None,
    })
}

fn is_fetch_request_paused(message: &CdpMessage) -> bool {
    matches!(
        message,
        CdpMessage::Event { method, .. } if method == "Fetch.requestPaused"
    )
}

fn fetch_command_session_id(
    event_session_id: Option<&str>,
    fallback_session_id: Option<&str>,
) -> Option<String> {
    event_session_id.or(fallback_session_id).map(str::to_string)
}

fn fetch_body_only_text(parsed: &Value) -> Result<String, String> {
    if let Some(error) = parsed.get("error").and_then(Value::as_str) {
        return Err(error.to_string());
    }
    Ok(parsed
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string())
}

async fn restore_state_and_maybe_reload(
    tab: &mut TabState,
    saved: &SavedState,
    reload_after_restore: bool,
) -> Result<(), String> {
    restore_state(&tab.page, saved).await?;
    tab.invalidate();

    if reload_after_restore {
        tab.page.reload().await.map_err(|e| e.to_string())?;
        let _ = wait_for_stable(&tab.page).await;
        tab.invalidate();
    }

    Ok(())
}

async fn page_url_can_reload_state(page: &eoka::Page) -> bool {
    page.url()
        .await
        .map(|url| state_url_can_reload(&url))
        .unwrap_or(false)
}

fn state_url_can_reload(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

fn web_origin(url: &str) -> Option<String> {
    let scheme_end = url.find("://")?;
    let scheme = url[..scheme_end].to_lowercase();
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let rest = &url[scheme_end + 3..];
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    if authority_end == 0 {
        return None;
    }
    Some(format!(
        "{}://{}",
        scheme,
        rest[..authority_end].to_lowercase()
    ))
}

fn same_web_origin(left: &str, right: &str) -> bool {
    match (web_origin(left), web_origin(right)) {
        (Some(left_origin), Some(right_origin)) => left_origin == right_origin,
        _ => false,
    }
}

fn read_saved_state_file(path: &str) -> Result<SavedState, String> {
    let json = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    serde_json::from_str(&json).map_err(|e| format!("{path}: {e}"))
}

fn resolve_open_url_for_state(url: &str, saved: Option<&SavedState>) -> Result<String, String> {
    if url.starts_with('/') && !url.starts_with("//") {
        let saved = saved.ok_or_else(|| {
            "Relative URLs require --load-state with a saved http(s) URL.".to_string()
        })?;
        let origin = web_origin(&saved.url).ok_or_else(|| {
            "Relative URLs require --load-state with a saved http(s) URL.".to_string()
        })?;
        return Ok(format!("{origin}{url}"));
    }
    Ok(url.to_string())
}

fn state_should_prime_open(saved: &SavedState, url: &str) -> bool {
    same_web_origin(&saved.url, url)
        && (!saved.local_storage.is_empty() || !saved.session_storage.is_empty())
}

fn build_storage_seed_js(saved: &SavedState) -> Option<String> {
    if saved.local_storage.is_empty() && saved.session_storage.is_empty() {
        return None;
    }

    let origin = web_origin(&saved.url)?;
    let local = serde_json::to_string(&saved.local_storage).ok()?;
    let session = serde_json::to_string(&saved.session_storage).ok()?;
    Some(format!(
        "(() => {{\
            if (location.origin !== {origin}) return;\
            try {{ const d = {local}; for (const [k,v] of Object.entries(d)) localStorage.setItem(k,v); }} catch (_e) {{}}\
            try {{ const d = {session}; for (const [k,v] of Object.entries(d)) sessionStorage.setItem(k,v); }} catch (_e) {{}}\
        }})();",
        origin = json_str(&origin)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saved_state_with_storage(url: &str) -> SavedState {
        SavedState {
            url: url.into(),
            cookies: Vec::new(),
            local_storage: HashMap::from([("auth".into(), "ok".into())]),
            session_storage: HashMap::from([("flow".into(), "booking".into())]),
            user_agent: String::new(),
            saved_at: String::new(),
        }
    }

    #[test]
    fn state_restore_reload_only_for_web_origins() {
        assert!(state_url_can_reload("https://www.recreation.gov/"));
        assert!(state_url_can_reload("http://127.0.0.1:3000/"));
        assert!(!state_url_can_reload(""));
        assert!(!state_url_can_reload("about:blank"));
        assert!(!state_url_can_reload("data:text/html,hello"));
    }

    #[test]
    fn atomic_open_state_resolves_relative_url_to_saved_origin() {
        let saved = saved_state_with_storage("https://www.recreation.gov/");

        assert_eq!(
            resolve_open_url_for_state("/camping/campsites/71576", Some(&saved)).unwrap(),
            "https://www.recreation.gov/camping/campsites/71576"
        );
        assert_eq!(
            resolve_open_url_for_state("https://other.test/path", Some(&saved)).unwrap(),
            "https://other.test/path"
        );
    }

    #[test]
    fn atomic_open_state_rejects_relative_url_without_saved_origin() {
        let saved = saved_state_with_storage("about:blank");

        assert!(resolve_open_url_for_state("/deep", None).is_err());
        assert!(resolve_open_url_for_state("/deep", Some(&saved)).is_err());
    }

    #[test]
    fn atomic_open_state_primes_only_same_origin_open() {
        let saved = saved_state_with_storage("https://www.recreation.gov/cart");

        assert!(state_should_prime_open(
            &saved,
            "https://www.recreation.gov/camping/campsites/71576"
        ));
        assert!(!state_should_prime_open(
            &saved,
            "https://www.example.com/camping/campsites/71576"
        ));
    }

    #[test]
    fn storage_seed_js_sets_saved_storage_before_navigation_scripts() {
        let saved = saved_state_with_storage("https://www.recreation.gov/");
        let js = build_storage_seed_js(&saved).unwrap();

        assert!(js.contains("location.origin !== \"https://www.recreation.gov\""));
        assert!(js.contains("localStorage.setItem(k,v)"));
        assert!(js.contains("sessionStorage.setItem(k,v)"));
        assert!(js.contains("\"auth\":\"ok\""));
        assert!(js.contains("\"flow\":\"booking\""));
    }

    #[test]
    fn fetch_body_only_surfaces_fetch_errors() {
        let err = fetch_body_only_text(&json!({ "error": "Failed to fetch" })).unwrap_err();

        assert_eq!(err, "Failed to fetch");
    }

    #[test]
    fn fetch_body_only_extracts_body() {
        let body = fetch_body_only_text(&json!({ "status": 200, "body": "ok" })).unwrap();

        assert_eq!(body, "ok");
    }

    #[test]
    fn fetch_command_session_prefers_event_session() {
        assert_eq!(
            fetch_command_session_id(Some("event-session"), Some("current-session")),
            Some("event-session".to_string())
        );
    }

    #[test]
    fn fetch_request_paused_detection_keeps_session_id_available() {
        let message = CdpMessage::Event {
            method: "Fetch.requestPaused".into(),
            params: json!({ "requestId": "req-1" }),
            session_id: Some("session-1".into()),
        };

        assert!(is_fetch_request_paused(&message));
        match message {
            CdpMessage::Event { session_id, .. } => {
                assert_eq!(session_id.as_deref(), Some("session-1"))
            }
            _ => panic!("expected event"),
        }
    }
}
