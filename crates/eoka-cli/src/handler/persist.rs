//! State save/load and console capture.

use std::collections::HashMap;

use eoka::Page;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::json;

use super::state::TabState;

// ---------------------------------------------------------------------------
// Saved state types
// ---------------------------------------------------------------------------

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

/// Browser-state exports from the Go client represent session-cookie expiry as
/// `null`; the CLI format historically used zero. Accept both forms while
/// continuing to write the stable numeric representation.
fn expiry_or_zero<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<f64>::deserialize(deserializer)?.unwrap_or(0.0))
}

impl From<eoka::SessionCookie> for SavedCookie {
    fn from(c: eoka::SessionCookie) -> Self {
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
    fn to_session_cookie(&self) -> eoka::SessionCookie {
        eoka::SessionCookie {
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
        .map_err(|e| e.to_string())?;

    let session_storage: HashMap<String, String> = page
        .evaluate_sync(
            "(() => { try { return Object.fromEntries(Object.entries(sessionStorage)) } catch(e) { return {} } })()",
        )
        .await
        .map_err(|e| e.to_string())?;

    let url: String = page
        .evaluate_sync("location.href")
        .await
        .map_err(|e| e.to_string())?;

    let user_agent: String = page
        .evaluate_sync("navigator.userAgent")
        .await
        .map_err(|e| e.to_string())?;

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
    restore_cookies(page, state).await?;
    restore_storage(page, "localStorage", &state.local_storage).await?;
    restore_storage(page, "sessionStorage", &state.session_storage).await?;
    Ok(())
}

/// Restore cookies only. Storage must be restored in the destination origin.
pub async fn restore_cookies(page: &Page, state: &SavedState) -> Result<(), String> {
    page.clear_all_cookies().await.map_err(|e| e.to_string())?;
    let set_cookies: Vec<eoka::SessionCookie> = state
        .cookies
        .iter()
        .map(|c| c.to_session_cookie())
        .collect();
    if !set_cookies.is_empty() {
        page.set_cookies_bulk(set_cookies)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

async fn restore_storage(
    page: &Page,
    storage_name: &str,
    data: &HashMap<String, String>,
) -> Result<(), String> {
    if data.is_empty() {
        return Ok(());
    }
    let json = serde_json::to_string(data).map_err(|e| e.to_string())?;
    let js = format!(
        "(() => {{ {s}.clear(); const d = {j}; for (const [k,v] of Object.entries(d)) {s}.setItem(k,v); return 'ok'; }})()",
        s = storage_name,
        j = json
    );
    let _: String = page.evaluate_sync(&js).await.map_err(|e| e.to_string())?;
    Ok(())
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
    if let Err(e) = browser.disconnect().await {
        eprintln!(
            "[eoka] warning: failed to disconnect from cloned Chrome session: {}",
            e
        );
    }
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
    // Best-effort: this IIFE has no return value, so evaluate_sync::<String>
    // routinely "fails" to deserialize a String even when the injection
    // itself succeeded. The addScriptToEvaluateOnNewDocument call above is
    // what actually needs to succeed; this just seeds the *current* page too.
    let _: String = tab
        .page
        .evaluate_sync(CONSOLE_CAPTURE_JS)
        .await
        .unwrap_or_default();
    tab.console_injected = true;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::SavedState;

    #[test]
    fn loads_go_client_browser_state_shape() {
        let state: SavedState = serde_json::from_str(
            r#"{
                "url": "https://www.recreation.gov/cart",
                "cookies": [{
                    "name": "session", "value": "redacted", "domain": ".recreation.gov",
                    "path": "/", "expires": null, "http_only": true,
                    "secure": true, "same_site": null
                }],
                "localStorage": {"recaccount": "value"},
                "sessionStorage": {"key": "value"},
                "userAgent": "Mozilla/5.0"
            }"#,
        )
        .expect("Go client state should be accepted");

        assert_eq!(state.cookies[0].expires, 0.0);
        assert_eq!(state.local_storage["recaccount"], "value");
        assert_eq!(state.session_storage["key"], "value");
        assert_eq!(state.user_agent, "Mozilla/5.0");
        assert!(state.saved_at.is_empty());
    }
}
