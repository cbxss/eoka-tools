use std::collections::HashMap;
use std::sync::Arc;

use eoka_mcp::{InteractiveElement, ObserveConfig};
use eoka_server::eoka::{Browser, Page, TabInfo};
use eoka_server::{dispatch::dispatch, state::AppState};
use serde_json::json;

pub(crate) struct TabState {
    pub page: Page,
    pub elements: Vec<InteractiveElement>,
    pub snapshot_refs: HashMap<String, i64>,
    pub console_injected: bool,
}

impl TabState {
    pub fn new(page: Page) -> Self {
        Self {
            page,
            elements: Vec::new(),
            snapshot_refs: HashMap::new(),
            console_injected: false,
        }
    }

    pub fn invalidate(&mut self) {
        self.elements.clear();
        self.snapshot_refs.clear();
    }
}

pub(crate) struct BrowserState {
    pub server: AppState,
    pub browser: Arc<Browser>,
    pub tabs: HashMap<String, TabState>,
    pub current_tab_id: Option<String>,
    pub config: ObserveConfig,
    pub unhealthy: bool,
}

impl BrowserState {
    pub async fn new(headless: bool) -> eoka_server::eoka::Result<Self> {
        let proxy = parse_proxy_env()?
            .map(|value| {
                eoka_proxy::parse(&value)
                    .map_err(|error| eoka_server::eoka::Error::Launch(error.to_string()))
            })
            .transpose()?;

        let (proxy, proxy_username, proxy_password) = if let Some(proxy) = proxy {
            (Some(proxy.server), proxy.username, proxy.password)
        } else {
            (None, None, None)
        };

        if let Some(ref p) = proxy {
            eprintln!("[eoka-mcp] using proxy: {}", p);
        }

        let cdp_timeout = if proxy.is_some() { 90 } else { 30 };

        eprintln!(
            "[eoka-mcp] launching browser (headless={}, cdp_timeout={}s, proxy={})",
            headless,
            cdp_timeout,
            proxy.is_some()
        );
        let mut server = AppState::new();
        let params = json!({
            "headless": headless,
            "proxy": proxy.map(|server| json!({
                "server": server,
                "username": proxy_username,
                "password": proxy_password,
            })),
        });
        dispatch(&mut server, "browser.launch", params)
            .await
            .map_err(|error| eoka_server::eoka::Error::Launch(error.message))?;
        let browser = server
            .browser()
            .map_err(|error| eoka_server::eoka::Error::Launch(error.message))?
            .clone();
        Ok(Self {
            server,
            browser,
            tabs: HashMap::new(),
            current_tab_id: None,
            config: ObserveConfig::default(),
            unhealthy: false,
        })
    }

    pub async fn ensure_tab(&mut self, url: &str) -> eoka_server::eoka::Result<&mut TabState> {
        let tab_id = if let Some(existing_id) = &self.current_tab_id {
            if let Some(tab) = self.tabs.get_mut(existing_id) {
                tab.invalidate();
                dispatch(
                    &mut self.server,
                    "page.goto",
                    json!({ "pageId": existing_id, "url": url }),
                )
                .await
                .map_err(|error| eoka_server::eoka::Error::cdp_msg(error.message))?;
            }
            existing_id.clone()
        } else {
            let (new_id, _) = self.new_tab(Some(url)).await?;
            new_id
        };
        Ok(self.tabs.get_mut(&tab_id).unwrap())
    }

    pub fn current_tab(&self) -> Option<&TabState> {
        self.current_tab_id
            .as_ref()
            .and_then(|id| self.tabs.get(id))
    }

    pub fn current_tab_mut(&mut self) -> Option<&mut TabState> {
        self.current_tab_id
            .as_ref()
            .and_then(|id| self.tabs.get_mut(id))
    }

    pub async fn new_tab(
        &mut self,
        url: Option<&str>,
    ) -> eoka_server::eoka::Result<(String, &mut TabState)> {
        let result = dispatch(&mut self.server, "browser.new_page", json!({ "url": url }))
            .await
            .map_err(|error| eoka_server::eoka::Error::cdp_msg(error.message))?;
        let tab_id = result["pageId"]
            .as_str()
            .ok_or_else(|| eoka_server::eoka::Error::cdp_msg("server omitted pageId"))?
            .to_owned();
        let page = self
            .server
            .page(&tab_id)
            .map_err(|error| eoka_server::eoka::Error::cdp_msg(error.message))?
            .clone();
        self.tabs.insert(tab_id.clone(), TabState::new(page));
        self.browser.activate_tab(&tab_id).await?;
        self.current_tab_id = Some(tab_id.clone());
        Ok((
            tab_id,
            self.tabs
                .get_mut(self.current_tab_id.as_ref().unwrap())
                .unwrap(),
        ))
    }

    pub async fn switch_tab(&mut self, tab_id: &str) -> eoka_server::eoka::Result<()> {
        if !self.tabs.contains_key(tab_id) {
            let page = self.browser.attach_page(tab_id).await?;
            self.server.insert_page(tab_id.to_string(), page.clone());
            self.tabs.insert(tab_id.to_string(), TabState::new(page));
        }
        self.browser.activate_tab(tab_id).await?;
        self.current_tab_id = Some(tab_id.to_string());
        Ok(())
    }

    pub async fn close_tab(&mut self, tab_id: &str) -> eoka_server::eoka::Result<()> {
        if self.tabs.len() <= 1 {
            return Err(eoka_server::eoka::Error::cdp_msg(
                "Cannot close the last tab",
            ));
        }
        if !self.tabs.contains_key(tab_id) {
            return Err(eoka_server::eoka::Error::ElementNotFound(format!(
                "Tab {} not found",
                tab_id
            )));
        }

        dispatch(
            &mut self.server,
            "browser.close_tab",
            json!({ "pageId": tab_id }),
        )
        .await
        .map_err(|error| eoka_server::eoka::Error::cdp_msg(error.message))?;
        self.tabs.remove(tab_id);

        if self.current_tab_id.as_deref() == Some(tab_id) {
            if let Some(new_id) = self.tabs.keys().next().cloned() {
                self.current_tab_id = Some(new_id.clone());
                self.browser.activate_tab(&new_id).await?;
            } else {
                self.current_tab_id = None;
            }
        }
        Ok(())
    }

    pub async fn list_tabs(&self) -> eoka_server::eoka::Result<Vec<TabInfo>> {
        self.browser.tabs().await
    }

    pub async fn close(self) -> eoka_server::eoka::Result<()> {
        let BrowserState {
            mut server,
            browser,
            ..
        } = self;
        drop(browser);
        dispatch(&mut server, "browser.close", json!({}))
            .await
            .map_err(|error| eoka_server::eoka::Error::cdp_msg(error.message))?;
        Ok(())
    }
}

fn parse_proxy_env() -> eoka_server::eoka::Result<Option<String>> {
    if let Ok(path) = std::env::var("EOKA_PROXY_FILE") {
        let contents = std::fs::read_to_string(path)?;
        let lines: Vec<&str> = contents
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .collect();
        if lines.is_empty() {
            return Err(eoka_server::eoka::Error::Launch(
                "proxy file does not contain a proxy".to_owned(),
            ));
        }
        use std::time::SystemTime;
        let index = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as usize % lines.len())
            .unwrap_or(0);
        Ok(Some(lines[index].to_owned()))
    } else {
        Ok(std::env::var("EOKA_PROXY").ok())
    }
}
