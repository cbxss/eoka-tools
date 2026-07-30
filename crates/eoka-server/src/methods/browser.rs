use serde::Deserialize;
use serde_json::{json, Value};
use std::path::Path;

use eoka::Browser;
use eoka_proxy::ProxyConfig;

use super::{parse_params, PageIdParams};
use crate::protocol::ServerError;
use crate::state::AppState;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LaunchParams {
    #[serde(default = "default_headless")]
    headless: bool,
    user_agent: Option<String>,
    timezone: Option<String>,
    viewport_width: Option<u32>,
    viewport_height: Option<u32>,
    user_data_dir: Option<String>,
    proxy: Option<ProxyParams>,
}

#[derive(Deserialize)]
struct ProxyParams {
    server: String,
    username: Option<String>,
    password: Option<String>,
}

fn default_headless() -> bool {
    true
}

pub async fn launch(state: &mut AppState, params: Value) -> Result<Value, ServerError> {
    if state.is_launched() {
        return Err(ServerError::internal("browser already launched"));
    }
    let params: LaunchParams = parse_params(params)?;
    if params.viewport_width.is_some() != params.viewport_height.is_some() {
        return Err(ServerError::invalid_params(
            "viewportWidth and viewportHeight must be provided together",
        ));
    }
    if params.viewport_width.is_some_and(|width| width == 0)
        || params.viewport_height.is_some_and(|height| height == 0)
    {
        return Err(ServerError::invalid_params(
            "viewport dimensions must be positive",
        ));
    }
    if let Some(path) = params.user_data_dir.as_deref() {
        if !Path::new(path).is_absolute() {
            return Err(ServerError::invalid_params(
                "userDataDir must be an absolute path",
            ));
        }
    }
    let proxy = resolve_proxy(params.proxy)?;
    let browser = Browser::launch_with(|config| {
        config.headless = params.headless;
        config.user_agent = params.user_agent;
        config.timezone = params.timezone;
        config.user_data_dir = params.user_data_dir;
        if let Some(width) = params.viewport_width {
            config.viewport_width = width;
        }
        if let Some(height) = params.viewport_height {
            config.viewport_height = height;
        }
        if let Some(proxy) = proxy {
            config.proxy = Some(proxy.server);
            config.proxy_username = proxy.username;
            config.proxy_password = proxy.password;
            config.cdp_timeout = 90;
        }
    })
    .await?;
    state.set_browser(browser);
    Ok(json!({}))
}

fn resolve_proxy(params: Option<ProxyParams>) -> Result<Option<ProxyConfig>, ServerError> {
    params
        .map(|params| {
            eoka_proxy::parse_server(&params.server, params.username, params.password)
                .map_err(|error| ServerError::invalid_params(error.to_string()))
        })
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_authenticated_socks5_proxy() {
        let proxy = resolve_proxy(Some(ProxyParams {
            server: "socks5://127.0.0.1:1080".to_owned(),
            username: Some("user".to_owned()),
            password: Some("password".to_owned()),
        }))
        .unwrap()
        .unwrap();
        assert_eq!(proxy.server, "socks5://127.0.0.1:1080");
        assert_eq!(proxy.username.as_deref(), Some("user"));
        assert_eq!(proxy.password.as_deref(), Some("password"));
    }

    #[test]
    fn rejects_proxy_credentials_in_server_url() {
        let error = resolve_proxy(Some(ProxyParams {
            server: "socks5://user:supersensitive@127.0.0.1:1080".to_owned(),
            username: None,
            password: None,
        }))
        .unwrap_err();
        assert!(error.message.contains("credentials"));
        assert!(!error.message.contains("supersensitive"));
    }
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
