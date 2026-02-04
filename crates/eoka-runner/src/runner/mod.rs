mod executor;

use crate::config::{BrowserConfig, Config};
use crate::Result;
use eoka::{Browser, Page};
use executor::ExecutionContext;
use std::path::Path;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};

/// Result of running a config.
#[derive(Debug)]
pub struct RunResult {
    /// Whether the run succeeded.
    pub success: bool,
    /// Error message if failed.
    pub error: Option<String>,
    /// Number of actions executed.
    pub actions_executed: usize,
    /// Total duration in milliseconds.
    pub duration_ms: u64,
    /// Number of retry attempts made.
    pub retries: u32,
}

/// Executes automation configs.
pub struct Runner {
    browser: Browser,
    page: Page,
}

impl Runner {
    /// Create a new runner with browser config.
    pub async fn new(config: &BrowserConfig) -> Result<Self> {
        let stealth = eoka::StealthConfig {
            headless: config.headless,
            proxy: config.proxy.clone(),
            user_agent: config.user_agent.clone(),
            viewport_width: config.viewport.as_ref().map(|v| v.width).unwrap_or(1280),
            viewport_height: config.viewport.as_ref().map(|v| v.height).unwrap_or(720),
            ..Default::default()
        };

        debug!(
            "Launching browser (headless: {}, proxy: {:?})",
            config.headless, config.proxy
        );
        let browser = Browser::launch_with_config(stealth).await?;
        let page = browser.new_page("about:blank").await?;

        Ok(Self { browser, page })
    }

    /// Get a reference to the page (for swarm integration).
    pub fn page(&self) -> &Page {
        &self.page
    }

    /// Run the config with retry support.
    pub async fn run(&mut self, config: &Config) -> Result<RunResult> {
        self.run_with_base_path(config, ".").await
    }

    /// Run the config with a base path for resolving includes.
    pub async fn run_with_base_path(
        &mut self,
        config: &Config,
        base_path: impl AsRef<Path>,
    ) -> Result<RunResult> {
        let ctx = ExecutionContext::new(base_path.as_ref());
        let start = Instant::now();
        let retry_config = config.on_failure.as_ref().and_then(|f| f.retry.as_ref());
        let max_attempts = retry_config.map(|r| r.attempts).unwrap_or(1);
        let retry_delay = retry_config.map(|r| r.delay_ms).unwrap_or(0);

        let mut last_error = None;
        let mut last_actions_executed = 0;
        let mut retries = 0;

        for attempt in 1..=max_attempts {
            if attempt > 1 {
                retries += 1;
                info!("Retry attempt {}/{}", attempt, max_attempts);
                if retry_delay > 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(retry_delay)).await;
                }
            }

            match self.run_once(config, &ctx).await {
                Ok(result) if result.success => {
                    return Ok(RunResult {
                        success: true,
                        error: None,
                        actions_executed: result.actions_executed,
                        duration_ms: start.elapsed().as_millis() as u64,
                        retries,
                    });
                }
                Ok(result) => {
                    last_actions_executed = result.actions_executed;
                    last_error = Some("success conditions not met".to_string());
                    if attempt == max_attempts {
                        self.handle_failure(config).await;
                    }
                }
                Err(e) => {
                    warn!("Attempt {} failed: {}", attempt, e);
                    last_error = Some(e.to_string());
                    if attempt == max_attempts {
                        self.handle_failure(config).await;
                    }
                }
            }
        }

        Ok(RunResult {
            success: false,
            error: last_error,
            actions_executed: last_actions_executed,
            duration_ms: start.elapsed().as_millis() as u64,
            retries,
        })
    }

    async fn handle_failure(&self, config: &Config) {
        if let Some(ref on_failure) = config.on_failure {
            if let Some(ref screenshot_path) = on_failure.screenshot {
                let timestamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let path = screenshot_path.replace("{timestamp}", &timestamp.to_string());
                info!("Saving failure screenshot to: {}", path);
                if let Ok(data) = self.page.screenshot().await {
                    if let Err(e) = std::fs::write(&path, data) {
                        warn!("Failed to save screenshot: {}", e);
                    }
                }
            }
        }
    }

    async fn run_once(&mut self, config: &Config, ctx: &ExecutionContext) -> Result<RunResult> {
        info!("Navigating to: {}", config.target.url);
        self.page.goto(&config.target.url).await?;

        let mut actions_executed = 0;
        for (i, action) in config.actions.iter().enumerate() {
            debug!("Executing action {}: {}", i + 1, action.name());
            executor::execute_with_context(&self.page, action, ctx).await?;
            actions_executed += 1;
        }

        let success = self.check_success(config).await?;
        debug!("Success check: {}", success);

        Ok(RunResult {
            success,
            error: None,
            actions_executed,
            duration_ms: 0,
            retries: 0,
        })
    }

    async fn check_success(&self, config: &Config) -> Result<bool> {
        let Some(ref success) = config.success else {
            return Ok(true);
        };

        if let Some(ref any) = success.any {
            for cond in any {
                if self.check_condition(cond).await? {
                    return Ok(true);
                }
            }
            return Ok(false);
        }

        if let Some(ref all) = success.all {
            for cond in all {
                if !self.check_condition(cond).await? {
                    return Ok(false);
                }
            }
        }

        Ok(true)
    }

    async fn check_condition(&self, condition: &crate::config::schema::Condition) -> Result<bool> {
        use crate::config::schema::Condition;
        match condition {
            Condition::UrlContains(pattern) => {
                let url = self.page.url().await?;
                Ok(url.contains(pattern))
            }
            Condition::TextContains(pattern) => {
                let text = self.page.text().await?;
                Ok(text.contains(pattern))
            }
        }
    }

    /// Close the browser.
    pub async fn close(self) -> Result<()> {
        self.browser.close().await?;
        Ok(())
    }
}
