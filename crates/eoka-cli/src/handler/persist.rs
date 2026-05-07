//! State save/load and console capture.

use std::collections::HashMap;

use eoka::Page;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::state::TabState;

// ---------------------------------------------------------------------------
// Saved state types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct SavedState {
    pub url: String,
    pub cookies: Vec<SavedCookie>,
    pub local_storage: HashMap<String, String>,
    pub session_storage: HashMap<String, String>,
    pub user_agent: String,
    pub saved_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SavedCookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub expires: f64,
    pub http_only: bool,
    pub secure: bool,
    pub same_site: Option<String>,
}

impl From<eoka::cdp::types::Cookie> for SavedCookie {
    fn from(c: eoka::cdp::types::Cookie) -> Self {
        Self {
            name: c.name,
            value: c.value,
            domain: c.domain,
            path: c.path,
            expires: c.expires,
            http_only: c.http_only,
            secure: c.secure,
            same_site: c.same_site,
        }
    }
}

impl SavedCookie {
    fn to_network_set_cookie(&self) -> eoka::cdp::types::NetworkSetCookie {
        eoka::cdp::types::NetworkSetCookie {
            name: self.name.clone(),
            value: self.value.clone(),
            url: None,
            domain: Some(self.domain.clone()),
            path: Some(self.path.clone()),
            secure: Some(self.secure),
            http_only: Some(self.http_only),
            same_site: self.same_site.clone(),
            expires: if self.expires > 0.0 {
                Some(self.expires)
            } else {
                None
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Capture / Restore
// ---------------------------------------------------------------------------

/// Capture full browser state: cookies (via CDP), localStorage, sessionStorage, URL, UA.
pub async fn capture_state(page: &Page) -> Result<SavedState, String> {
    let cookies: Vec<SavedCookie> = page
        .cookies()
        .await
        .map_err(|e| e.to_string())?
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

/// Restore browser state: clear + set cookies, clear + set localStorage/sessionStorage.
pub async fn restore_state(page: &Page, state: &SavedState) -> Result<(), String> {
    page.clear_all_cookies().await.map_err(|e| e.to_string())?;
    let set_cookies: Vec<eoka::cdp::types::NetworkSetCookie> = state
        .cookies
        .iter()
        .map(|c| c.to_network_set_cookie())
        .collect();
    if !set_cookies.is_empty() {
        page.set_cookies_bulk(set_cookies)
            .await
            .map_err(|e| e.to_string())?;
    }

    restore_storage(page, "localStorage", &state.local_storage).await;
    restore_storage(page, "sessionStorage", &state.session_storage).await;
    Ok(())
}

async fn restore_storage(page: &Page, storage_name: &str, data: &HashMap<String, String>) {
    if data.is_empty() {
        return;
    }
    if let Ok(json) = serde_json::to_string(data) {
        let js = format!(
            "(() => {{ {s}.clear(); const d = {j}; for (const [k,v] of Object.entries(d)) {s}.setItem(k,v); }})()",
            s = storage_name,
            j = json
        );
        let _: String = page.evaluate_sync(&js).await.unwrap_or_default();
    }
}

/// Connect to a running Chrome (port number or ws:// URL), capture its
/// front-tab state via CDP, and disconnect. The `Browser::disconnect()` call
/// leaves the user's Chrome running.
pub async fn clone_state_from_source(source: &str) -> Result<SavedState, String> {
    use crate::launch_spec::resolve_cdp_spec;

    let ws_url = resolve_cdp_spec(source)?;
    let browser = eoka::Browser::connect(&ws_url)
        .await
        .map_err(|e| format!("connect: {}", e))?;

    let tabs = browser.tabs().await.map_err(|e| e.to_string())?;
    // Skip DevTools/extension targets — we want a real user page.
    let target = tabs
        .iter()
        .find(|t| !t.url.starts_with("devtools://") && !t.url.starts_with("chrome-extension://"))
        .ok_or_else(|| "No user-page tab found in the running Chrome".to_string())?;

    let page = browser
        .attach_page(&target.id)
        .await
        .map_err(|e| e.to_string())?;

    let state = capture_state(&page).await?;
    let _ = browser.disconnect().await;
    Ok(state)
}

// ---------------------------------------------------------------------------
// Console capture
// ---------------------------------------------------------------------------

const CONSOLE_CAPTURE_JS: &str = include_str!("../js/console_capture.js");

/// Inject console capture into the current page and register for future navigations.
pub async fn ensure_console_capture(tab: &mut TabState) -> Result<(), String> {
    if tab.console_injected {
        return Ok(());
    }
    tab.page
        .session()
        .send::<_, serde_json::Value>(
            "Page.addScriptToEvaluateOnNewDocument",
            &json!({ "source": CONSOLE_CAPTURE_JS }),
        )
        .await
        .map_err(|e| e.to_string())?;
    let _: String = tab
        .page
        .evaluate_sync(CONSOLE_CAPTURE_JS)
        .await
        .unwrap_or_default();
    tab.console_injected = true;
    Ok(())
}
