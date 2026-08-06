use base64::Engine;
use captcha::{build_captcha_inject_js, AntiCaptcha, CaptchaInjectionKind};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

use super::browser::close_tab_impl;
use super::{parse_params, PageIdParams};
use crate::protocol::ServerError;
use crate::state::AppState;

fn single(key: &'static str, value: impl Into<Value>) -> Value {
    let mut map = Map::with_capacity(1);
    map.insert(key.to_string(), value.into());
    Value::Object(map)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SelectorParams {
    page_id: String,
    selector: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TextParams {
    page_id: String,
    text: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GotoParams {
    page_id: String,
    url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FillParams {
    page_id: String,
    selector: String,
    value: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TypeIntoParams {
    page_id: String,
    selector: String,
    text: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetAttributeParams {
    page_id: String,
    selector: String,
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WaitForParams {
    page_id: String,
    selector: String,
    timeout_ms: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WaitForTextParams {
    page_id: String,
    text: String,
    timeout_ms: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsParams {
    page_id: String,
    js: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FetchParams {
    page_id: String,
    url: String,
    #[serde(default = "default_fetch_method")]
    method: String,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default = "default_fetch_redirect")]
    redirect: String,
}

fn default_fetch_method() -> String {
    "GET".into()
}

fn default_fetch_redirect() -> String {
    "follow".into()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PressKeyParams {
    page_id: String,
    key: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RestoreStateParams {
    page_id: String,
    state: eoka::BrowserState,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SolveCaptchaParams {
    page_id: String,
    api_key: String,
    captcha_type: String,
    captcha_mode: String,
    #[serde(rename = "websiteURL")]
    website_url: String,
    website_key: String,
    #[serde(default)]
    enterprise_payload: Option<Value>,
    #[serde(default)]
    api_domain: Option<String>,
    #[serde(default)]
    callback: Option<String>,
}

pub async fn goto(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: GotoParams = parse_params(params)?;
    state.page(&params.page_id)?.goto(&params.url).await?;
    Ok(json!({}))
}

pub async fn click(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: SelectorParams = parse_params(params)?;
    state.page(&params.page_id)?.click(&params.selector).await?;
    Ok(json!({}))
}

pub async fn click_text(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: TextParams = parse_params(params)?;
    state
        .page(&params.page_id)?
        .click_by_text(&params.text)
        .await?;
    Ok(json!({}))
}

pub async fn human_click(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: SelectorParams = parse_params(params)?;
    state
        .page(&params.page_id)?
        .human_click(&params.selector)
        .await?;
    Ok(json!({}))
}

pub async fn human_click_text(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: TextParams = parse_params(params)?;
    state
        .page(&params.page_id)?
        .human_click_by_text(&params.text)
        .await?;
    Ok(json!({}))
}

pub async fn fill(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: FillParams = parse_params(params)?;
    state
        .page(&params.page_id)?
        .fill(&params.selector, &params.value)
        .await?;
    Ok(json!({}))
}

pub async fn human_fill(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: FillParams = parse_params(params)?;
    state
        .page(&params.page_id)?
        .human_fill(&params.selector, &params.value)
        .await?;
    Ok(json!({}))
}

pub async fn type_into(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: TypeIntoParams = parse_params(params)?;
    state
        .page(&params.page_id)?
        .type_into(&params.selector, &params.text)
        .await?;
    Ok(json!({}))
}

pub async fn text(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: PageIdParams = parse_params(params)?;
    let text = state.page(&params.page_id)?.text().await?;
    Ok(single("text", text))
}

pub async fn content(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: PageIdParams = parse_params(params)?;
    let html = state.page(&params.page_id)?.content().await?;
    Ok(single("html", html))
}

pub async fn title(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: PageIdParams = parse_params(params)?;
    let title = state.page(&params.page_id)?.title().await?;
    Ok(single("title", title))
}

pub async fn url(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: PageIdParams = parse_params(params)?;
    let url = state.page(&params.page_id)?.url().await?;
    Ok(single("url", url))
}

pub async fn get_text(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: SelectorParams = parse_params(params)?;
    let page = state.page(&params.page_id)?;
    let element = page.find(&params.selector).await?;
    let text = element.text().await?;
    Ok(single("text", text))
}

pub async fn get_attribute(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: GetAttributeParams = parse_params(params)?;
    let page = state.page(&params.page_id)?;
    let element = page.find(&params.selector).await?;
    let value = element.get_attribute(&params.name).await?;
    Ok(single("value", value))
}

pub async fn exists(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: SelectorParams = parse_params(params)?;
    let exists = state.page(&params.page_id)?.exists(&params.selector).await;
    Ok(json!({ "exists": exists }))
}

pub async fn wait_for(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: WaitForParams = parse_params(params)?;
    state
        .page(&params.page_id)?
        .wait_for(&params.selector, params.timeout_ms)
        .await?;
    Ok(json!({}))
}

pub async fn wait_for_visible(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: WaitForParams = parse_params(params)?;
    state
        .page(&params.page_id)?
        .wait_for_visible(&params.selector, params.timeout_ms)
        .await?;
    Ok(json!({}))
}

pub async fn wait_for_text(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: WaitForTextParams = parse_params(params)?;
    state
        .page(&params.page_id)?
        .wait_for_text(&params.text, params.timeout_ms)
        .await?;
    Ok(json!({}))
}

pub async fn evaluate(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: JsParams = parse_params(params)?;
    let result: Value = state.page(&params.page_id)?.evaluate(&params.js).await?;
    Ok(single("result", result))
}

pub async fn execute(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: JsParams = parse_params(params)?;
    state.page(&params.page_id)?.execute(&params.js).await?;
    Ok(json!({}))
}

/// Performs a request from the current page's browser context.
///
/// Unlike an external HTTP client, this uses the page's live cookies,
/// browser fingerprint, and same-origin policy. The response body is returned
/// verbatim so callers can decode JSON or another API format themselves.
pub async fn fetch(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: FetchParams = parse_params(params)?;
    let script = build_browser_fetch_js(&params)?;
    let body: String = state.page(&params.page_id)?.evaluate(&script).await?;
    serde_json::from_str(&body)
        .map_err(|error| ServerError::internal(format!("invalid browser fetch response: {error}")))
}

fn build_browser_fetch_js(params: &FetchParams) -> Result<String, ServerError> {
    if !matches!(params.redirect.as_str(), "follow" | "error" | "manual") {
        return Err(ServerError::invalid_params(
            "redirect must be follow, error, or manual",
        ));
    }
    let url = serde_json::to_string(&params.url)
        .map_err(|error| ServerError::internal(error.to_string()))?;
    let method = serde_json::to_string(&params.method)
        .map_err(|error| ServerError::internal(error.to_string()))?;
    let headers = serde_json::to_string(&params.headers)
        .map_err(|error| ServerError::internal(error.to_string()))?;
    let body = serde_json::to_string(&params.body)
        .map_err(|error| ServerError::internal(error.to_string()))?;
    let redirect = serde_json::to_string(&params.redirect)
        .map_err(|error| ServerError::internal(error.to_string()))?;

    Ok(format!(
        r#"(async () => {{
            const response = await fetch({url}, {{
                method: {method},
                headers: {headers},
                body: {body},
                credentials: "include",
                redirect: {redirect},
            }});
            const body = await response.text();
            return JSON.stringify({{
                url: response.url,
                status: response.status,
                ok: response.ok,
                headers: Object.fromEntries(response.headers.entries()),
                body,
            }});
        }})()"#
    ))
}

pub async fn capture_state(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: PageIdParams = parse_params(params)?;
    let state = state.page(&params.page_id)?.capture_state().await?;
    let state =
        serde_json::to_value(state).map_err(|error| ServerError::internal(error.to_string()))?;
    Ok(single("state", state))
}

pub async fn restore_state(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: RestoreStateParams = parse_params(params)?;
    state
        .page(&params.page_id)?
        .restore_state(&params.state)
        .await?;
    Ok(json!({}))
}

pub async fn screenshot(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: PageIdParams = parse_params(params)?;
    let png = state.page(&params.page_id)?.screenshot().await?;
    let data_base64 = base64::engine::general_purpose::STANDARD.encode(png);
    Ok(single("dataBase64", data_base64))
}

pub async fn select(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: FillParams = parse_params(params)?;
    state
        .page(&params.page_id)?
        .select(&params.selector, &params.value)
        .await?;
    Ok(json!({}))
}

pub async fn hover(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: SelectorParams = parse_params(params)?;
    state.page(&params.page_id)?.hover(&params.selector).await?;
    Ok(json!({}))
}

pub async fn press_key(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: PressKeyParams = parse_params(params)?;
    state.page(&params.page_id)?.press_key(&params.key).await?;
    Ok(json!({}))
}

/// Solve a supported CAPTCHA and apply its token inside the current page.
///
/// The token intentionally never crosses the server protocol boundary. It is
/// both sensitive and only useful in the browser session that requested it.
pub async fn solve_captcha(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: SolveCaptchaParams = parse_params(params)?;
    if params.captcha_mode != "anti_captcha_proxyless" {
        return Err(ServerError::invalid_params(
            "captchaMode must explicitly be anti_captcha_proxyless; proxy-backed and manual modes belong to the caller",
        ));
    }
    let solution = match params.captcha_type.as_str() {
        "recaptcha_v2_enterprise" => {
            AntiCaptcha::new(params.api_key)
                .solve_recaptcha_v2_enterprise(
                    &params.website_url,
                    &params.website_key,
                    params.enterprise_payload,
                    params.api_domain.as_deref(),
                )
                .await
        }
        other => {
            return Err(ServerError::invalid_params(format!(
                "unsupported captcha type {other:?}; supported: recaptcha_v2_enterprise"
            )));
        }
    }
    .map_err(|error| ServerError::internal(format!("captcha solve failed: {error}")))?;

    let token = solution
        .token()
        .ok_or_else(|| ServerError::internal("captcha solver returned no token"))?;
    let solver_user_agent = solution.user_agent.clone();
    let script = build_captcha_inject_js(
        token,
        CaptchaInjectionKind::Recaptcha,
        params.callback.as_deref(),
    );
    let page = state.page(&params.page_id)?;
    let mut result = captcha_injection_result(page, &script).await?;
    let callback_count = result
        .get("callbacks")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    // reCAPTCHA widgets can attach their callback client just after a token is
    // first delivered. Re-scan once; the injection script de-duplicates
    // already-delivered callbacks by token and path.
    if callback_count <= 1 {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let settled = captcha_injection_result(page, &script).await?;
        if settled
            .get("callbacks")
            .and_then(Value::as_array)
            .is_some_and(|callbacks| !callbacks.is_empty())
        {
            result = settled;
        }
    }
    // Preserve non-secret solver metadata for callers that need to diagnose
    // identity consistency. The CAPTCHA token never leaves this process.
    if let Some(user_agent) = solver_user_agent {
        result["solverUserAgent"] = Value::String(user_agent);
    }
    Ok(single("injection", result))
}

async fn captcha_injection_result(page: &eoka::Page, script: &str) -> Result<Value, ServerError> {
    let result: String = page.evaluate(script).await?;
    serde_json::from_str(&result).map_err(|error| {
        ServerError::internal(format!("invalid CAPTCHA injection result: {error}"))
    })
}

pub async fn close(state: &mut AppState, params: Value) -> Result<Value, ServerError> {
    let params: PageIdParams = parse_params(params)?;
    close_tab_impl(state, &params.page_id).await?;
    Ok(json!({}))
}

#[cfg(test)]
mod tests {
    use super::{build_browser_fetch_js, FetchParams};
    use std::collections::BTreeMap;

    #[test]
    fn browser_fetch_uses_page_credentials_and_encodes_values() {
        let script = build_browser_fetch_js(&FetchParams {
            page_id: "page-1".into(),
            url: "https://example.test/api?x=\"quoted\"".into(),
            method: "POST".into(),
            headers: BTreeMap::from([("Content-Type".into(), "application/json".into())]),
            body: Some(r#"{"value":"quoted"}"#.into()),
            redirect: "follow".into(),
        })
        .unwrap();

        assert!(script.contains("credentials: \"include\""));
        assert!(script.contains("\"POST\""));
        assert!(script.contains("\\\"quoted\\\""));
    }

    #[test]
    fn browser_fetch_rejects_unknown_redirect_mode() {
        let error = build_browser_fetch_js(&FetchParams {
            page_id: "page-1".into(),
            url: "https://example.test".into(),
            method: "GET".into(),
            headers: BTreeMap::new(),
            body: None,
            redirect: "somewhere".into(),
        })
        .unwrap_err();

        assert!(error.message.contains("redirect"));
    }
}
