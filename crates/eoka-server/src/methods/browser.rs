use serde::Deserialize;
use serde_json::{json, Value};

use eoka::Browser;

use super::{parse_params, PageIdParams};
use crate::protocol::ServerError;
use crate::state::AppState;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LaunchParams {
    #[serde(default = "default_headless")]
    headless: bool,
}

fn default_headless() -> bool {
    true
}

pub async fn launch(state: &mut AppState, params: Value) -> Result<Value, ServerError> {
    if state.is_launched() {
        return Err(ServerError::internal("browser already launched"));
    }
    let params: LaunchParams = parse_params(params)?;
    let browser = Browser::launch_with(|config| {
        config.headless = params.headless;
    })
    .await?;
    state.set_browser(browser);
    Ok(json!({}))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NewPageParams {
    url: Option<String>,
}

pub async fn new_page(state: &mut AppState, params: Value) -> Result<Value, ServerError> {
    let params: NewPageParams = parse_params(params)?;
    let browser = state.browser()?;
    let page = match params.url {
        Some(url) => browser.new_page(&url).await?,
        None => browser.new_blank_page().await?,
    };
    let page_id = page.target_id().to_string();
    state.insert_page(page_id.clone(), page);
    Ok(json!({ "pageId": page_id }))
}

pub async fn tabs(state: &AppState, _params: Value) -> Result<Value, ServerError> {
    let browser = state.browser()?;
    let tabs = browser.tabs().await?;
    let tabs: Vec<Value> = tabs
        .into_iter()
        .map(|t| json!({ "id": t.id, "title": t.title, "url": t.url }))
        .collect();
    Ok(json!({ "tabs": tabs }))
}

pub(crate) async fn close_tab_impl(state: &mut AppState, page_id: &str) -> Result<(), ServerError> {
    state.page(page_id)?;
    let browser = state.browser()?;
    browser.close_tab(page_id).await?;
    state.remove_page(page_id);
    Ok(())
}

pub async fn close_tab(state: &mut AppState, params: Value) -> Result<Value, ServerError> {
    let params: PageIdParams = parse_params(params)?;
    close_tab_impl(state, &params.page_id).await?;
    Ok(json!({}))
}

pub async fn close(state: &mut AppState, _params: Value) -> Result<Value, ServerError> {
    let browser = state
        .take_browser()
        .ok_or_else(|| ServerError::internal("browser not launched"))?;
    state.clear_pages();
    browser.close().await?;
    state.mark_shutdown();
    Ok(json!({}))
}
