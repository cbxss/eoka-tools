use std::collections::HashMap;

use eoka::{Browser, Page, StealthConfig};
use eoka_mcp::{InteractiveElement, ObserveConfig};

use super::profile::clone_profile_dir;

pub struct TabState {
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

pub struct BrowserState {
    pub browser: Browser,
    pub tabs: HashMap<String, TabState>,
    pub current_tab_id: Option<String>,
    pub config: ObserveConfig,
    pub is_live: bool,
}

impl BrowserState {
    pub async fn launched(
        headless: bool,
        copy_profile_from: Option<&std::path::Path>,
        no_stealth: bool,
    ) -> eoka::Result<Self> {
        let patch_binary = std::env::var("EOKA_PATCH_BINARY")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);

        let proxy = parse_proxy_env()?
            .map(|value| {
                eoka_proxy::parse(&value).map_err(|error| eoka::Error::Launch(error.to_string()))
            })
            .transpose()?;

        let (proxy, proxy_username, proxy_password) = match proxy {
            Some(proxy) => (Some(proxy.server), proxy.username, proxy.password),
            None => (None, None, None),
        };

        let cdp_timeout = if proxy.is_some() { 90 } else { 30 };

        let mut extra_args: Vec<String> = std::env::var("EOKA_CHROME_ARGS")
            .unwrap_or_default()
            .split(':')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        if let Some(src) = copy_profile_from {
            let dst = clone_profile_dir(src).map_err(eoka::Error::Io)?;
            extra_args.push(format!("--user-data-dir={}", dst.display()));
            eprintln!(
                "[eoka] cloned profile {} → {}",
                src.display(),
                dst.display()
            );
        }

        eprintln!(
            "[eoka] launching browser (headless={}, stealth={}, cdp_timeout={}s, proxy={}, profile_clone={})",
            headless,
            !no_stealth,
            cdp_timeout,
            proxy.is_some(),
            copy_profile_from.is_some()
        );

        let config = StealthConfig {
            headless,
            patch_binary: patch_binary && !no_stealth,
            proxy,
            proxy_username,
            proxy_password,
            cdp_timeout,
            extra_args,
            live_session: no_stealth,
            filter_cdp: !no_stealth,
            ..Default::default()
        };
        let browser = Browser::launch_with_config(config).await?;
        Ok(Self {
            browser,
            tabs: HashMap::new(),
            current_tab_id: None,
            config: ObserveConfig::default(),
            is_live: false,
        })
    }

    pub async fn connected(ws_url: &str) -> eoka::Result<Self> {
        eprintln!("[eoka] connecting to {}", ws_url);
        let browser = Browser::connect(ws_url).await?;
        Ok(Self {
            browser,
            tabs: HashMap::new(),
            current_tab_id: None,
            config: ObserveConfig::default(),
            is_live: true,
        })
    }

    pub async fn ensure_tab(&mut self, url: &str) -> eoka::Result<&mut TabState> {
        if let Some(existing_id) = self.current_tab_id.clone() {
            if let Some(tab) = self.tabs.get_mut(&existing_id) {
                tab.invalidate();
                tab.page.goto(url).await?;
            }
            return self
                .tabs
                .get_mut(&existing_id)
                .ok_or_else(|| eoka::Error::cdp_msg("Current tab disappeared"));
        }

        let page = self.browser.new_page(url).await?;
        let id = page.target_id().to_string();
        self.tabs.insert(id.clone(), TabState::new(page));
        self.current_tab_id = Some(id.clone());
        Ok(self.tabs.get_mut(&id).expect("just inserted"))
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

    pub async fn ensure_blank_tab(&mut self) -> eoka::Result<&mut TabState> {
        let page = self.browser.new_blank_page().await?;
        let id = page.target_id().to_string();
        self.tabs.insert(id.clone(), TabState::new(page));
        self.current_tab_id = Some(id.clone());
        Ok(self.tabs.get_mut(&id).expect("just inserted"))
    }

    pub async fn new_tab(&mut self, url: Option<&str>) -> eoka::Result<(String, &mut TabState)> {
        let page = match url {
            Some(u) => self.browser.new_page(u).await?,
            None => self.browser.new_blank_page().await?,
        };
        let tab_id = page.target_id().to_string();
        self.tabs.insert(tab_id.clone(), TabState::new(page));
        self.browser.activate_tab(&tab_id).await?;
        self.current_tab_id = Some(tab_id.clone());
        let tab = self.tabs.get_mut(&tab_id).expect("just inserted");
        Ok((tab_id, tab))
    }

    pub async fn switch_tab(&mut self, tab_id: &str) -> eoka::Result<()> {
        if !self.tabs.contains_key(tab_id) {
            let page = self.browser.attach_page(tab_id).await?;
            self.tabs.insert(tab_id.to_string(), TabState::new(page));
        }
        self.browser.activate_tab(tab_id).await?;
        self.current_tab_id = Some(tab_id.to_string());
        Ok(())
    }

    pub async fn attach_existing_tab(&mut self, tab_id: &str) -> eoka::Result<()> {
        if !self.tabs.contains_key(tab_id) {
            let page = self.browser.attach_page(tab_id).await?;
            self.tabs.insert(tab_id.to_string(), TabState::new(page));
        }
        self.current_tab_id = Some(tab_id.to_string());
        Ok(())
    }

    pub async fn close_tab(&mut self, tab_id: &str) -> eoka::Result<()> {
        if self.tabs.len() <= 1 {
            return Err(eoka::Error::cdp_msg("Cannot close the last tab"));
        }
        self.browser.close_tab(tab_id).await?;
        self.tabs.remove(tab_id);
        if self.current_tab_id.as_deref() == Some(tab_id) {
            if let Some(new_id) = self.tabs.keys().next().cloned() {
                self.browser.activate_tab(&new_id).await?;
                self.current_tab_id = Some(new_id);
            } else {
                self.current_tab_id = None;
            }
        }
        Ok(())
    }

    pub async fn close(self) -> eoka::Result<()> {
        self.browser.close().await
    }
}

fn parse_proxy_env() -> eoka::Result<Option<String>> {
    if let Ok(path) = std::env::var("EOKA_PROXY_FILE") {
        let contents = std::fs::read_to_string(path)?;
        let lines: Vec<&str> = contents
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .collect();
        if lines.is_empty() {
            return Err(eoka::Error::Launch(
                "proxy file does not contain a proxy".to_owned(),
            ));
        }
        use std::time::SystemTime;
        let idx = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos() as usize % lines.len())
            .unwrap_or(0);
        Ok(Some(lines[idx].to_string()))
    } else {
        Ok(std::env::var("EOKA_PROXY").ok())
    }
}
