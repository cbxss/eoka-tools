//! Browser and tab state management.

use std::collections::HashMap;

use eoka::{Browser, Page, StealthConfig};
use eoka_agent::{InteractiveElement, ObserveConfig};

/// State for a single tab.
pub struct TabState {
    pub page: Page,
    pub elements: Vec<InteractiveElement>,
    /// Snapshot ref label → backend_node_id (from accessibility tree snapshot).
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

    /// Clear all cached state. Call on navigation.
    pub fn invalidate(&mut self) {
        self.elements.clear();
        self.snapshot_refs.clear();
    }
}

/// Multi-tab browser state.
pub struct BrowserState {
    pub browser: Browser,
    pub tabs: HashMap<String, TabState>,
    pub current_tab_id: Option<String>,
    pub config: ObserveConfig,
}

impl BrowserState {
    pub async fn new(headless: bool) -> eoka::Result<Self> {
        let patch_binary = std::env::var("EOKA_PATCH_BINARY")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);

        let proxy_line = parse_proxy_env();

        let (proxy, proxy_username, proxy_password) = match proxy_line {
            Some(proxy_str) => parse_proxy_string(&proxy_str),
            None => (None, None, None),
        };

        let cdp_timeout = if proxy.is_some() { 90 } else { 30 };

        eprintln!(
            "[eoka] launching browser (headless={}, cdp_timeout={}s, proxy={})",
            headless, cdp_timeout, proxy.is_some()
        );
        // EOKA_CHROME_ARGS: colon-separated extra Chrome flags
        // e.g. EOKA_CHROME_ARGS="--use-fake-ui-for-media-stream:--allow-insecure-localhost"
        let extra_args: Vec<String> = std::env::var("EOKA_CHROME_ARGS")
            .unwrap_or_default()
            .split(':')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        let config = StealthConfig {
            headless,
            patch_binary,
            proxy,
            proxy_username,
            proxy_password,
            cdp_timeout,
            extra_args,
            ..Default::default()
        };
        let browser = Browser::launch_with_config(config).await?;
        Ok(Self {
            browser,
            tabs: HashMap::new(),
            current_tab_id: None,
            config: ObserveConfig::default(),
        })
    }

    /// Get or create the current tab, navigating to URL.
    pub async fn ensure_tab(&mut self, url: &str) -> eoka::Result<&mut TabState> {
        if let Some(existing_id) = self.current_tab_id.clone() {
            if let Some(tab) = self.tabs.get_mut(&existing_id) {
                tab.invalidate();
                tab.page.goto(url).await?;
            }
            return self.tabs.get_mut(&existing_id).ok_or_else(|| {
                eoka::Error::CdpSimple("Current tab disappeared".into())
            });
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

    /// Create a blank tab, set it current, return its page for further setup.
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
        // Safe: we just inserted with this key
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

    pub async fn close_tab(&mut self, tab_id: &str) -> eoka::Result<()> {
        if self.tabs.len() <= 1 {
            return Err(eoka::Error::CdpSimple("Cannot close the last tab".into()));
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

// ---------------------------------------------------------------------------
// Proxy env parsing
// ---------------------------------------------------------------------------

fn parse_proxy_env() -> Option<String> {
    if let Ok(path) = std::env::var("EOKA_PROXY_FILE") {
        let contents = std::fs::read_to_string(&path).ok()?;
        let lines: Vec<&str> = contents
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .collect();
        if lines.is_empty() {
            return None;
        }
        use std::time::SystemTime;
        let idx = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos() as usize % lines.len())
            .unwrap_or(0);
        Some(lines[idx].to_string())
    } else {
        std::env::var("EOKA_PROXY").ok()
    }
}

fn parse_proxy_string(s: &str) -> (Option<String>, Option<String>, Option<String>) {
    let parts: Vec<&str> = s.splitn(4, ':').collect();
    match parts.len() {
        4 => (
            Some(format!("http://{}:{}", parts[0], parts[1])),
            Some(parts[2].to_string()),
            Some(parts[3].to_string()),
        ),
        2 => (
            Some(format!("http://{}:{}", parts[0], parts[1])),
            None,
            None,
        ),
        _ => (None, None, None),
    }
}
