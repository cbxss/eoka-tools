use std::collections::HashMap;

use eoka::{Browser, Page, StealthConfig, TabInfo};
use eoka_agent::{InteractiveElement, ObserveConfig};

/// State for a single tab.
pub(crate) struct TabState {
    pub page: Page,
    pub elements: Vec<InteractiveElement>,
    /// Snapshot ref label → backend_node_id (from accessibility tree snapshot)
    pub snapshot_refs: HashMap<String, i64>,
    /// Whether console capture JS has been injected via addScriptToEvaluateOnNewDocument.
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

    /// Clear all cached state (elements + snapshot refs). Call on navigation.
    pub fn invalidate(&mut self) {
        self.elements.clear();
        self.snapshot_refs.clear();
    }
}

/// Multi-tab browser state.
pub(crate) struct BrowserState {
    pub browser: Browser,
    pub tabs: HashMap<String, TabState>,
    pub current_tab_id: Option<String>,
    pub config: ObserveConfig,
    /// Set to true when a transport error is detected; triggers relaunch on next call.
    pub unhealthy: bool,
}

impl BrowserState {
    pub async fn new(headless: bool) -> eoka::Result<Self> {
        let patch_binary = std::env::var("EOKA_PATCH_BINARY")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);

        // Parse proxy: EOKA_PROXY_FILE (one per line, random) > EOKA_PROXY (single)
        // Format: host:port:username:password
        let proxy_line = if let Ok(path) = std::env::var("EOKA_PROXY_FILE") {
            match std::fs::read_to_string(&path) {
                Ok(contents) => {
                    let lines: Vec<&str> = contents
                        .lines()
                        .map(|l| l.trim())
                        .filter(|l| !l.is_empty() && !l.starts_with('#'))
                        .collect();
                    if lines.is_empty() {
                        eprintln!("[eoka-agent] proxy file is empty: {}", path);
                        None
                    } else {
                        use std::time::SystemTime;
                        let idx = SystemTime::now()
                            .duration_since(SystemTime::UNIX_EPOCH)
                            .map(|d| d.as_nanos() as usize % lines.len())
                            .unwrap_or(0);
                        Some(lines[idx].to_string())
                    }
                }
                Err(e) => {
                    eprintln!("[eoka-agent] failed to read proxy file {}: {}", path, e);
                    None
                }
            }
        } else {
            std::env::var("EOKA_PROXY").ok()
        };

        let (proxy, proxy_username, proxy_password) = if let Some(proxy_str) = proxy_line {
            let parts: Vec<&str> = proxy_str.splitn(4, ':').collect();
            if parts.len() == 4 {
                (
                    Some(format!("http://{}:{}", parts[0], parts[1])),
                    Some(parts[2].to_string()),
                    Some(parts[3].to_string()),
                )
            } else if parts.len() == 2 {
                (
                    Some(format!("http://{}:{}", parts[0], parts[1])),
                    None,
                    None,
                )
            } else {
                eprintln!("[eoka-agent] invalid proxy format, expected host:port:user:pass");
                (None, None, None)
            }
        } else {
            (None, None, None)
        };

        if let Some(ref p) = proxy {
            eprintln!("[eoka-agent] using proxy: {}", p);
        }

        // When using a proxy, increase CDP timeout (proxies are slower)
        let cdp_timeout = if proxy.is_some() { 90 } else { 30 };

        eprintln!(
            "[eoka-agent] launching browser (headless={}, cdp_timeout={}s, proxy={})",
            headless,
            cdp_timeout,
            proxy.is_some()
        );
        let config = StealthConfig {
            headless,
            patch_binary,
            proxy,
            proxy_username,
            proxy_password,
            cdp_timeout,
            ..Default::default()
        };
        let browser = Browser::launch_with_config(config).await?;
        Ok(Self {
            browser,
            tabs: HashMap::new(),
            current_tab_id: None,
            config: ObserveConfig::default(),
            unhealthy: false,
        })
    }

    /// Get or create the current tab, navigating to URL.
    pub async fn ensure_tab(&mut self, url: &str) -> eoka::Result<&mut TabState> {
        let tab_id = if let Some(existing_id) = &self.current_tab_id {
            // Navigate current tab
            if let Some(tab) = self.tabs.get_mut(existing_id) {
                tab.invalidate();
                tab.page.goto(url).await?;
            }
            existing_id.clone()
        } else {
            // Create first tab
            let page = self.browser.new_page(url).await?;
            let new_id = page.target_id().to_string();
            self.tabs.insert(new_id.clone(), TabState::new(page));
            self.current_tab_id = Some(new_id.clone());
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

    pub async fn new_tab(&mut self, url: Option<&str>) -> eoka::Result<(String, &mut TabState)> {
        let page = match url {
            Some(u) => self.browser.new_page(u).await?,
            None => self.browser.new_blank_page().await?,
        };
        let tab_id = page.target_id().to_string();
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

    /// Switch to a tab by ID. If the tab is not yet tracked (e.g., a popup opened by
    /// window.open()), automatically attaches to it and adds it to tracked tabs.
    pub async fn switch_tab(&mut self, tab_id: &str) -> eoka::Result<()> {
        if !self.tabs.contains_key(tab_id) {
            // Try to attach to an unmanaged target (popup, new window, etc.)
            let page = self.browser.attach_page(tab_id).await?;
            self.tabs.insert(tab_id.to_string(), TabState::new(page));
        }
        self.browser.activate_tab(tab_id).await?;
        self.current_tab_id = Some(tab_id.to_string());
        Ok(())
    }

    pub async fn close_tab(&mut self, tab_id: &str) -> eoka::Result<()> {
        if self.tabs.len() <= 1 {
            return Err(eoka::Error::CdpSimple("Cannot close the last tab".into()));
        }
        if !self.tabs.contains_key(tab_id) {
            return Err(eoka::Error::ElementNotFound(format!(
                "Tab {} not found",
                tab_id
            )));
        }

        self.browser.close_tab(tab_id).await?;
        self.tabs.remove(tab_id);

        // If we closed the current tab, switch to another
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

    pub async fn list_tabs(&self) -> eoka::Result<Vec<TabInfo>> {
        self.browser.tabs().await
    }

    pub async fn close(self) -> eoka::Result<()> {
        self.browser.close().await
    }
}
