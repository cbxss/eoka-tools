use std::collections::HashMap;
use std::fmt::Write;

use eoka_server::eoka::Page;
use rmcp::model::{CallToolResult, ContentBlock, ErrorData};
use serde::{Deserialize, Deserializer, Serialize};

use eoka_mcp::{observe, target, InteractiveElement, Target};

use super::error::{internal, invalid};
use super::state::TabState;
use super::types::JsRequest;

pub(crate) struct ResolvedTarget {
    pub selector: String,
    pub desc: String,
}

const SELECTOR_JS: &str = include_str!("../js/selector.js");

async fn resolve_ref(
    page: &Page,
    snapshot_refs: &HashMap<String, i64>,
    label: &str,
) -> Result<String, ErrorData> {
    let stale = |detail: &str| invalid(format!("Ref {} {}. Take a new snapshot.", label, detail));

    let backend_id = snapshot_refs.get(label).ok_or_else(|| stale("not found"))?;

    let resolve_result: serde_json::Value = page
        .session()
        .send(
            "DOM.resolveNode",
            &serde_json::json!({ "backendNodeId": backend_id }),
        )
        .await
        .map_err(|_| stale("no longer exists"))?;

    let object_id = resolve_result
        .get("object")
        .and_then(|o| o.get("objectId"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| stale("could not be resolved to DOM node"))?;

    let call_result: serde_json::Value = page
        .session()
        .send(
            "Runtime.callFunctionOn",
            &serde_json::json!({
                "objectId": object_id,
                "functionDeclaration": SELECTOR_JS,
                "arguments": [{ "objectId": object_id }],
                "returnByValue": true
            }),
        )
        .await
        .map_err(|_| stale("failed to generate selector"))?;

    call_result
        .get("result")
        .and_then(|r| r.get("value"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| stale("selector generation returned null"))
}

pub(crate) async fn resolve_target(
    tab: &TabState,
    target_str: &str,
) -> Result<ResolvedTarget, ErrorData> {
    match Target::parse(target_str) {
        Target::Index(idx) => {
            let el = tab.elements.get(idx).ok_or_else(|| {
                invalid(format!(
                    "Index {} out of range (have {})",
                    idx,
                    tab.elements.len()
                ))
            })?;
            Ok(ResolvedTarget {
                selector: el.selector.clone(),
                desc: el.to_string(),
            })
        }
        Target::Ref(label) => {
            let selector = resolve_ref(&tab.page, &tab.snapshot_refs, &label).await?;
            Ok(ResolvedTarget {
                desc: format!("ref {}", label),
                selector,
            })
        }
        Target::Live(pattern) => {
            let r = target::resolve(&tab.page, &pattern)
                .await
                .map_err(internal)?;
            if !r.found {
                return Err(invalid(
                    r.error
                        .unwrap_or_else(|| format!("{} not found", target_str)),
                ));
            }
            Ok(ResolvedTarget {
                selector: r.selector,
                desc: format!("<{}> \"{}\"", r.tag, r.text),
            })
        }
    }
}

pub(crate) async fn title_nonblocking(page: &Page) -> String {
    page.evaluate_sync("document.title || ''")
        .await
        .unwrap_or_default()
}

pub(crate) async fn wait_for_stable(page: &Page) -> eoka_server::eoka::Result<()> {
    let start = std::time::Instant::now();
    let max_wait = std::time::Duration::from_secs(10);
    loop {
        let ready: String = page
            .evaluate_sync("document.readyState || 'loading'")
            .await
            .unwrap_or_else(|_| "loading".to_string());
        if ready == "interactive" || ready == "complete" {
            return Ok(());
        }
        if start.elapsed() > max_wait {
            return Err(eoka_server::eoka::Error::cdp_msg(
                "Page did not reach interactive state within 10s",
            ));
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

pub(crate) async fn auto_observe_if_needed(
    tab: &mut TabState,
    target_str: &str,
    viewport_only: bool,
) -> Result<(), ErrorData> {
    if matches!(Target::parse(target_str), Target::Index(_)) && tab.elements.is_empty() {
        tab.elements = observe::observe(&tab.page, viewport_only)
            .await
            .map_err(internal)?;
    }
    Ok(())
}

pub(crate) fn is_stale_element_error(msg: &str) -> bool {
    const NEEDLES: &[&str] = &[
        "not found",
        "not visible",
        "node not connected",
        "stale element",
        "detached",
    ];
    NEEDLES.iter().any(|n| msg.contains(n))
}

enum Action<'a> {
    Click,
    Fill(&'a str),
}

impl Action<'_> {
    async fn run(&self, page: &Page, selector: &str) -> eoka_server::eoka::Result<()> {
        match self {
            Action::Click => page.click(selector).await,
            Action::Fill(text) => page.fill(selector, text).await,
        }
    }
}

async fn act_with_retry(
    tab: &mut TabState,
    target_str: &str,
    viewport_only: bool,
    action: Action<'_>,
) -> Result<String, ErrorData> {
    let resolved = resolve_target(tab, target_str).await?;
    match action.run(&tab.page, &resolved.selector).await {
        Ok(_) => Ok(resolved.desc),
        Err(e) if is_stale_element_error(&e.to_string()) => {
            tab.elements = observe::observe(&tab.page, viewport_only)
                .await
                .map_err(internal)?;
            let resolved = resolve_target(tab, target_str).await?;
            action
                .run(&tab.page, &resolved.selector)
                .await
                .map_err(internal)?;
            Ok(resolved.desc)
        }
        Err(e) => Err(internal(e)),
    }
}

pub(crate) async fn click_with_retry(
    tab: &mut TabState,
    target_str: &str,
    viewport_only: bool,
) -> Result<String, ErrorData> {
    act_with_retry(tab, target_str, viewport_only, Action::Click).await
}

pub(crate) async fn fill_with_retry(
    tab: &mut TabState,
    target_str: &str,
    text: &str,
    viewport_only: bool,
) -> Result<String, ErrorData> {
    act_with_retry(tab, target_str, viewport_only, Action::Fill(text)).await
}

pub(crate) fn resolve_js(req: &JsRequest) -> Result<String, ErrorData> {
    if let Some(path) = &req.file {
        std::fs::read_to_string(path)
            .map_err(|e| invalid(format!("Failed to read JS file '{}': {}", path, e)))
    } else if let Some(js) = &req.js {
        Ok(js.clone())
    } else {
        Err(invalid("Either 'js' or 'file' must be provided"))
    }
}

pub(crate) fn text_ok(s: impl Into<String>) -> Result<CallToolResult, ErrorData> {
    Ok(CallToolResult::success(vec![ContentBlock::text(s)]))
}

pub(crate) fn element_list(elements: &[InteractiveElement]) -> String {
    let mut out = String::with_capacity(elements.len() * 40);
    for el in elements {
        let _ = writeln!(out, "{}", el);
    }
    out
}

pub(crate) const VALID_OBSERVE_FILTERS: &[&str] = &["inputs", "buttons", "all"];

const CONSOLE_CAPTURE_JS: &str = include_str!("../js/console_capture.js");

pub(crate) async fn ensure_console_capture(tab: &mut TabState) -> Result<(), ErrorData> {
    if tab.console_injected {
        return Ok(());
    }
    tab.page
        .session()
        .send::<_, serde_json::Value>(
            "Page.addScriptToEvaluateOnNewDocument",
            &serde_json::json!({ "source": CONSOLE_CAPTURE_JS }),
        )
        .await
        .map_err(internal)?;
    let _: String = tab
        .page
        .evaluate_sync(CONSOLE_CAPTURE_JS)
        .await
        .unwrap_or_default();
    tab.console_injected = true;
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SavedState {
    pub url: String,
    pub cookies: Vec<SavedCookie>,
    #[serde(alias = "localStorage")]
    pub local_storage: HashMap<String, String>,
    #[serde(alias = "sessionStorage")]
    pub session_storage: HashMap<String, String>,
    #[serde(alias = "userAgent")]
    pub user_agent: String,
    #[serde(default, alias = "savedAt")]
    pub saved_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SavedCookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    #[serde(default, deserialize_with = "expiry_or_zero")]
    pub expires: f64,
    #[serde(alias = "httpOnly")]
    pub http_only: bool,
    pub secure: bool,
    #[serde(alias = "sameSite")]
    pub same_site: Option<String>,
}

fn expiry_or_zero<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<f64>::deserialize(deserializer)?.unwrap_or(0.0))
}

impl From<eoka_server::eoka::SessionCookie> for SavedCookie {
    fn from(c: eoka_server::eoka::SessionCookie) -> Self {
        Self {
            name: c.name,
            value: c.value,
            domain: c.domain,
            path: c.path,
            expires: c.expires.unwrap_or(0.0),
            http_only: c.http_only,
            secure: c.secure,
            same_site: c.same_site,
        }
    }
}

impl SavedCookie {
    pub fn to_session_cookie(&self) -> eoka_server::eoka::SessionCookie {
        eoka_server::eoka::SessionCookie {
            name: self.name.clone(),
            value: self.value.clone(),
            domain: self.domain.clone(),
            path: self.path.clone(),
            secure: self.secure,
            http_only: self.http_only,
            same_site: self.same_site.clone(),
            expires: if self.expires > 0.0 {
                Some(self.expires)
            } else {
                None
            },
        }
    }
}

pub(crate) async fn capture_state(page: &Page) -> Result<SavedState, ErrorData> {
    let cookies: Vec<SavedCookie> = page
        .cookies()
        .await
        .map_err(internal)?
        .into_iter()
        .map(SavedCookie::from)
        .collect();

    let local_storage: HashMap<String, String> = page
        .evaluate_sync(
            "(() => { try { return Object.fromEntries(Object.entries(localStorage)) } catch(e) { return {} } })()",
        )
        .await
        .unwrap_or_default();

    let session_storage: HashMap<String, String> = page
        .evaluate_sync(
            "(() => { try { return Object.fromEntries(Object.entries(sessionStorage)) } catch(e) { return {} } })()",
        )
        .await
        .unwrap_or_default();

    let url: String = page
        .evaluate_sync("location.href")
        .await
        .unwrap_or_default();

    let user_agent: String = page
        .evaluate_sync("navigator.userAgent")
        .await
        .unwrap_or_default();

    let saved_at = {
        let d = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        format!("{}", d.as_secs())
    };

    Ok(SavedState {
        url,
        cookies,
        local_storage,
        session_storage,
        user_agent,
        saved_at,
    })
}

pub(crate) async fn restore_state(page: &Page, state: &SavedState) -> Result<(), ErrorData> {
    page.clear_all_cookies().await.map_err(internal)?;
    let set_cookies: Vec<eoka_server::eoka::SessionCookie> = state
        .cookies
        .iter()
        .map(|c| c.to_session_cookie())
        .collect();
    if !set_cookies.is_empty() {
        page.set_cookies_bulk(set_cookies).await.map_err(internal)?;
    }

    if !state.local_storage.is_empty() {
        let json = serde_json::to_string(&state.local_storage).map_err(internal)?;
        let js = format!(
            "(() => {{ localStorage.clear(); const d = {}; for (const [k,v] of Object.entries(d)) localStorage.setItem(k,v); }})()",
            json
        );
        let _: String = page.evaluate_sync(&js).await.unwrap_or_default();
    }

    if !state.session_storage.is_empty() {
        let json = serde_json::to_string(&state.session_storage).map_err(internal)?;
        let js = format!(
            "(() => {{ sessionStorage.clear(); const d = {}; for (const [k,v] of Object.entries(d)) sessionStorage.setItem(k,v); }})()",
            json
        );
        let _: String = page.evaluate_sync(&js).await.unwrap_or_default();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use eoka_mcp::InteractiveElement;

    fn make_test_element(index: usize, tag: &str, text: &str) -> InteractiveElement {
        InteractiveElement {
            index,
            tag: tag.to_string(),
            role: None,
            text: text.to_string(),
            placeholder: None,
            input_type: None,
            selector: format!("[data-idx=\"{}\"]", index),
            checked: false,
            value: None,
            bbox: eoka_server::eoka::BoundingBox {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 30.0,
            },
            fingerprint: 0,
        }
    }

    #[test]
    fn resolve_js_inline_ok() {
        let req = JsRequest {
            js: Some("console.log('hi')".into()),
            file: None,
        };
        assert_eq!(resolve_js(&req).unwrap(), "console.log('hi')");
    }

    #[test]
    fn resolve_js_neither_field_errors() {
        let req = JsRequest {
            js: None,
            file: None,
        };
        assert!(resolve_js(&req).is_err());
    }

    #[test]
    fn resolve_js_nonexistent_file_errors() {
        let req = JsRequest {
            js: None,
            file: Some("/nonexistent/path/to/script.js".into()),
        };
        assert!(resolve_js(&req).is_err());
    }

    #[test]
    fn element_list_empty() {
        assert_eq!(element_list(&[]), "");
    }

    #[test]
    fn element_list_with_elements() {
        let els = vec![
            make_test_element(0, "button", "Submit"),
            make_test_element(1, "a", "Link"),
        ];
        let result = element_list(&els);
        assert!(result.contains("[0]"));
        assert!(result.contains("[1]"));
        assert!(result.contains("Submit"));
        assert!(result.contains("Link"));
        assert!(result.ends_with('\n'));
    }

    #[test]
    fn text_ok_returns_success() {
        let result = text_ok("hello").unwrap();
        assert!(!result.is_error.unwrap_or(false));
        assert_eq!(result.content.len(), 1);
    }

    #[test]
    fn is_stale_element_error_matches() {
        assert!(is_stale_element_error("element not found in DOM"));
        assert!(is_stale_element_error("element not visible"));
        assert!(is_stale_element_error("node not connected"));
        assert!(is_stale_element_error("stale element reference"));
        assert!(is_stale_element_error("element detached from DOM"));
    }

    #[test]
    fn is_stale_element_error_rejects_unrelated() {
        assert!(!is_stale_element_error("invalid selector"));
        assert!(!is_stale_element_error("timeout"));
        assert!(!is_stale_element_error(""));
    }

    #[test]
    fn saved_cookie_from_cdp_cookie() {
        let cdp = eoka_server::eoka::SessionCookie {
            name: "sid".into(),
            value: "abc123".into(),
            domain: ".example.com".into(),
            path: "/".into(),
            secure: true,
            http_only: true,
            same_site: Some("Lax".into()),
            expires: Some(1700000000.0),
        };
        let saved = SavedCookie::from(cdp);
        assert_eq!(saved.name, "sid");
        assert_eq!(saved.domain, ".example.com");
        assert!(saved.http_only);
        assert!(saved.secure);
        assert_eq!(saved.same_site.as_deref(), Some("Lax"));

        let sc = saved.to_session_cookie();
        assert_eq!(sc.name, "sid");
        assert_eq!(sc.domain, ".example.com");
        assert!(sc.http_only);
        assert_eq!(sc.expires, Some(1700000000.0));
    }

    #[test]
    fn saved_cookie_session_cookie_no_expires() {
        let saved = SavedCookie {
            name: "tmp".into(),
            value: "v".into(),
            domain: "example.com".into(),
            path: "/".into(),
            expires: -1.0,
            http_only: false,
            secure: false,
            same_site: None,
        };
        let sc = saved.to_session_cookie();
        assert_eq!(sc.expires, None);
    }

    #[test]
    fn saved_state_roundtrip_json() {
        let state = SavedState {
            url: "https://example.com".into(),
            cookies: vec![SavedCookie {
                name: "a".into(),
                value: "b".into(),
                domain: ".example.com".into(),
                path: "/".into(),
                expires: 0.0,
                http_only: false,
                secure: false,
                same_site: None,
            }],
            local_storage: [("key".into(), "val".into())].into(),
            session_storage: HashMap::new(),
            user_agent: "test".into(),
            saved_at: "12345".into(),
        };
        let json = serde_json::to_string(&state).unwrap();
        let restored: SavedState = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.url, "https://example.com");
        assert_eq!(restored.cookies.len(), 1);
        assert_eq!(restored.cookies[0].name, "a");
        assert_eq!(restored.cookies[0].value, "b");
        assert_eq!(restored.cookies[0].domain, ".example.com");
        assert_eq!(restored.local_storage.get("key").unwrap(), "val");
        assert!(restored.session_storage.is_empty());
        assert_eq!(restored.user_agent, "test");
        assert_eq!(restored.saved_at, "12345");
    }

    #[test]
    fn saved_state_empty_roundtrip() {
        let state = SavedState {
            url: "about:blank".into(),
            cookies: vec![],
            local_storage: HashMap::new(),
            session_storage: HashMap::new(),
            user_agent: "".into(),
            saved_at: "0".into(),
        };
        let json = serde_json::to_string(&state).unwrap();
        let restored: SavedState = serde_json::from_str(&json).unwrap();
        assert!(restored.cookies.is_empty());
        assert!(restored.local_storage.is_empty());
        assert!(restored.session_storage.is_empty());
    }

    #[test]
    fn saved_cookie_all_fields_survive_conversion() {
        let cdp = eoka_server::eoka::SessionCookie {
            name: "token".into(),
            value: "eyJhbGciOiJSUzI1NiJ9".into(),
            domain: ".app.example.com".into(),
            path: "/api".into(),
            secure: true,
            http_only: true,
            same_site: Some("Strict".into()),
            expires: Some(1893456000.0),
        };
        let saved = SavedCookie::from(cdp);
        let sc = saved.to_session_cookie();

        assert_eq!(sc.name, "token");
        assert_eq!(sc.value, "eyJhbGciOiJSUzI1NiJ9");
        assert_eq!(sc.domain, ".app.example.com");
        assert_eq!(sc.path, "/api");
        assert_eq!(sc.expires, Some(1893456000.0));
        assert!(sc.http_only);
        assert!(sc.secure);
        assert_eq!(sc.same_site.as_deref(), Some("Strict"));
    }

    #[test]
    fn saved_cookie_unicode_values() {
        let saved = SavedCookie {
            name: "lang".into(),
            value: "日本語".into(),
            domain: ".example.com".into(),
            path: "/".into(),
            expires: 0.0,
            http_only: false,
            secure: false,
            same_site: None,
        };
        let json = serde_json::to_string(&saved).unwrap();
        let restored: SavedCookie = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.value, "日本語");
    }

    #[test]
    fn saved_state_many_storage_keys() {
        let mut ls = HashMap::new();
        for i in 0..100 {
            ls.insert(format!("key_{}", i), format!("value_{}", i));
        }
        let state = SavedState {
            url: "https://example.com".into(),
            cookies: vec![],
            local_storage: ls,
            session_storage: HashMap::new(),
            user_agent: "test".into(),
            saved_at: "0".into(),
        };
        let json = serde_json::to_string(&state).unwrap();
        let restored: SavedState = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.local_storage.len(), 100);
        assert_eq!(restored.local_storage.get("key_42").unwrap(), "value_42");
    }

    #[test]
    fn saved_state_file_write_and_read() {
        let state = SavedState {
            url: "https://example.com/dashboard".into(),
            cookies: vec![
                SavedCookie {
                    name: "sid".into(),
                    value: "abc".into(),
                    domain: ".example.com".into(),
                    path: "/".into(),
                    expires: 1893456000.0,
                    http_only: true,
                    secure: true,
                    same_site: Some("Lax".into()),
                },
                SavedCookie {
                    name: "theme".into(),
                    value: "dark".into(),
                    domain: "example.com".into(),
                    path: "/".into(),
                    expires: 0.0,
                    http_only: false,
                    secure: false,
                    same_site: None,
                },
            ],
            local_storage: [("token".into(), "jwt.xyz".into())].into(),
            session_storage: [("tab_id".into(), "t1".into())].into(),
            user_agent: "Mozilla/5.0 Test".into(),
            saved_at: "1700000000".into(),
        };

        let dir = std::env::temp_dir().join("eoka_test_state");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test_state.json");

        let json = serde_json::to_string_pretty(&state).unwrap();
        std::fs::write(&path, &json).unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        let restored: SavedState = serde_json::from_str(&contents).unwrap();

        assert_eq!(restored.url, "https://example.com/dashboard");
        assert_eq!(restored.cookies.len(), 2);
        assert_eq!(restored.cookies[0].name, "sid");
        assert!(restored.cookies[0].http_only);
        assert_eq!(restored.cookies[1].name, "theme");
        assert_eq!(restored.local_storage.get("token").unwrap(), "jwt.xyz");
        assert_eq!(restored.session_storage.get("tab_id").unwrap(), "t1");

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn saved_state_invalid_json_fails() {
        let result = serde_json::from_str::<SavedState>("not json");
        assert!(result.is_err());
    }

    #[test]
    fn saved_state_missing_field_fails() {
        let result = serde_json::from_str::<SavedState>(r#"{"cookies":[]}"#);
        assert!(result.is_err());
    }
}
