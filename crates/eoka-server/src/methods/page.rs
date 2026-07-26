use base64::Engine;
use serde::Deserialize;
use serde_json::{json, Map, Value};

use super::browser::close_tab_impl;
use super::{parse_params, PageIdParams};
use crate::protocol::ServerError;
use crate::state::AppState;

/// Builds a single-field JSON result object, moving `value` in directly
/// instead of `json!({"key": value})`'s re-serialize-through-`to_value`
/// behavior — avoids an extra deep copy of large payloads like page HTML or
/// screenshot base64 data.
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
struct PressKeyParams {
    page_id: String,
    key: String,
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

pub async fn human_click(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: SelectorParams = parse_params(params)?;
    state
        .page(&params.page_id)?
        .human_click(&params.selector)
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

pub async fn close(state: &mut AppState, params: Value) -> Result<Value, ServerError> {
    let params: PageIdParams = parse_params(params)?;
    close_tab_impl(state, &params.page_id).await?;
    Ok(json!({}))
}
