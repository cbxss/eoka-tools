//! # eoka-agent
//!
//! AI agent interaction layer for browser automation. Use directly or via MCP.
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use eoka_agent::Session;
//!
//! # #[tokio::main]
//! # async fn main() -> eoka::Result<()> {
//! let mut session = Session::launch().await?;
//! session.goto("https://example.com").await?;
//!
//! // Observe → get compact element list → act by index
//! session.observe().await?;
//! println!("{}", session.element_list());
//! session.click(0).await?;
//!
//! session.close().await?;
//! # Ok(())
//! # }
//! ```

pub mod annotate;
pub mod observe;
pub mod spa;

pub use spa::{RouterType, SpaRouterInfo};

use std::collections::HashSet;
use std::fmt;

use eoka::{BoundingBox, Page, Result};

// Re-export eoka types that users need
pub use eoka::{Browser, Error, StealthConfig};

/// An interactive element on the page, identified by index.
#[derive(Debug, Clone)]
pub struct InteractiveElement {
    /// Zero-based index (stable until next `observe()`)
    pub index: usize,
    /// HTML tag name (e.g. "button", "input", "a")
    pub tag: String,
    /// ARIA role if set
    pub role: Option<String>,
    /// Visible text content, truncated to 60 chars
    pub text: String,
    /// Placeholder attribute for inputs
    pub placeholder: Option<String>,
    /// Input type (only for `<input>` and `<select>` elements)
    pub input_type: Option<String>,
    /// Unique CSS selector for this element
    pub selector: String,
    /// Whether the element is checked (radio/checkbox)
    pub checked: bool,
    /// Current value of form element (None if empty or non-form)
    pub value: Option<String>,
    /// Bounding box in viewport coordinates
    pub bbox: BoundingBox,
    /// Fingerprint for stale element detection (hash of tag+text+attributes)
    pub fingerprint: u64,
}

impl InteractiveElement {
    /// Create a fingerprint from element properties for stale detection.
    /// Includes enough fields to distinguish similar elements.
    pub fn compute_fingerprint(
        tag: &str,
        text: &str,
        role: Option<&str>,
        input_type: Option<&str>,
        placeholder: Option<&str>,
        selector: &str,
    ) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        tag.hash(&mut hasher);
        text.hash(&mut hasher);
        role.hash(&mut hasher);
        input_type.hash(&mut hasher);
        placeholder.hash(&mut hasher);
        // Include selector prefix (first 50 chars) for positional uniqueness
        selector[..selector.len().min(50)].hash(&mut hasher);
        hasher.finish()
    }
}

impl fmt::Display for InteractiveElement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] <{}", self.index, self.tag)?;
        if let Some(ref t) = self.input_type {
            if t != "text" {
                write!(f, " type=\"{}\"", t)?;
            }
        }
        f.write_str(">")?;
        if self.checked {
            f.write_str(" [checked]")?;
        }
        if !self.text.is_empty() {
            write!(f, " \"{}\"", self.text)?;
        }
        if let Some(ref v) = self.value {
            write!(f, " value=\"{}\"", v)?;
        }
        if let Some(ref p) = self.placeholder {
            write!(f, " placeholder=\"{}\"", p)?;
        }
        if let Some(ref r) = self.role {
            let redundant = (r == "button" && self.tag == "button")
                || (r == "link" && self.tag == "a")
                || (r == "menuitem" && self.tag == "a");
            if !redundant {
                write!(f, " role=\"{}\"", r)?;
            }
        }
        Ok(())
    }
}

/// Configuration for observation behavior.
#[derive(Debug, Clone)]
pub struct ObserveConfig {
    /// Only include elements visible in the current viewport.
    /// Dramatically reduces token count on long pages. Default: true.
    pub viewport_only: bool,
}

impl Default for ObserveConfig {
    fn default() -> Self {
        Self {
            viewport_only: true,
        }
    }
}

/// Result of a diff-based observation.
#[derive(Debug)]
pub struct ObserveDiff {
    /// Indices of elements that appeared since last observe.
    pub added: Vec<usize>,
    /// Count of elements that disappeared since last observe.
    pub removed: usize,
    /// Total element count after this observe.
    pub total: usize,
}

impl fmt::Display for ObserveDiff {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.added.is_empty() && self.removed == 0 {
            write!(f, "no changes ({} elements)", self.total)
        } else {
            let mut need_sep = false;
            if !self.added.is_empty() {
                write!(f, "+{} added", self.added.len())?;
                need_sep = true;
            }
            if self.removed > 0 {
                if need_sep {
                    write!(f, ", ")?;
                }
                write!(f, "-{} removed", self.removed)?;
            }
            write!(f, " ({} total)", self.total)
        }
    }
}

/// Wraps a `Page` with agent-friendly observation and interaction methods.
///
/// The core loop is: `observe()` → read `element_list()` → `click(i)` / `fill(i, text)`.
pub struct AgentPage<'a> {
    page: &'a Page,
    elements: Vec<InteractiveElement>,
    config: ObserveConfig,
}

impl<'a> AgentPage<'a> {
    /// Create an AgentPage wrapping an existing eoka Page.
    pub fn new(page: &'a Page) -> Self {
        Self {
            page,
            elements: Vec::new(),
            config: ObserveConfig::default(),
        }
    }

    /// Create with custom observation config.
    pub fn with_config(page: &'a Page, config: ObserveConfig) -> Self {
        Self {
            page,
            elements: Vec::new(),
            config,
        }
    }

    /// Get a reference to the underlying Page.
    pub fn page(&self) -> &Page {
        self.page
    }

    // =========================================================================
    // Observation
    // =========================================================================

    /// Snapshot the page: enumerate all interactive elements.
    pub async fn observe(&mut self) -> Result<&[InteractiveElement]> {
        self.elements = observe::observe(self.page, self.config.viewport_only).await?;
        Ok(&self.elements)
    }

    /// Observe and return a diff against the previous observation.
    /// Use this in multi-step sessions to minimize tokens — only send
    /// `added_element_list()` to the LLM instead of the full list.
    pub async fn observe_diff(&mut self) -> Result<ObserveDiff> {
        let old_selectors: HashSet<String> =
            self.elements.iter().map(|e| e.selector.clone()).collect();

        self.elements = observe::observe(self.page, self.config.viewport_only).await?;

        let new_selectors: HashSet<&str> =
            self.elements.iter().map(|e| e.selector.as_str()).collect();

        let added: Vec<usize> = self
            .elements
            .iter()
            .filter(|e| !old_selectors.contains(&e.selector))
            .map(|e| e.index)
            .collect();

        let removed = old_selectors
            .iter()
            .filter(|s| !new_selectors.contains(s.as_str()))
            .count();

        Ok(ObserveDiff {
            added,
            removed,
            total: self.elements.len(),
        })
    }

    /// Compact text list of only the added elements from the last `observe_diff()`.
    pub fn added_element_list(&self, diff: &ObserveDiff) -> String {
        let mut out = String::new();
        for &idx in &diff.added {
            if let Some(el) = self.elements.get(idx) {
                out.push_str(&el.to_string());
                out.push('\n');
            }
        }
        out
    }

    /// Take an annotated screenshot with numbered boxes on each element.
    /// Calls `observe()` first if no elements have been enumerated yet.
    pub async fn screenshot(&mut self) -> Result<Vec<u8>> {
        if self.elements.is_empty() {
            self.observe().await?;
        }
        annotate::annotated_screenshot(self.page, &self.elements).await
    }

    /// Take a plain screenshot without annotations.
    pub async fn screenshot_plain(&self) -> Result<Vec<u8>> {
        self.page.screenshot().await
    }

    /// Compact text list for LLM consumption.
    /// Each line: `[index] <tag type="x"> "text" placeholder="y"`
    pub fn element_list(&self) -> String {
        let mut out = String::with_capacity(self.elements.len() * 40);
        for el in &self.elements {
            out.push_str(&el.to_string());
            out.push('\n');
        }
        out
    }

    /// Get element info by index.
    pub fn get(&self, index: usize) -> Option<&InteractiveElement> {
        self.elements.get(index)
    }

    /// Get all observed elements.
    pub fn elements(&self) -> &[InteractiveElement] {
        &self.elements
    }

    /// Number of observed elements.
    pub fn len(&self) -> usize {
        self.elements.len()
    }

    /// Whether the element list is empty.
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    /// Find first element whose text contains the given substring (case-insensitive).
    /// Returns the element index, or None.
    pub fn find_by_text(&self, needle: &str) -> Option<usize> {
        let needle_lower = needle.to_lowercase();
        self.elements
            .iter()
            .find(|e| e.text.to_lowercase().contains(&needle_lower))
            .map(|e| e.index)
    }

    /// Find all elements whose text contains the given substring (case-insensitive).
    pub fn find_all_by_text(&self, needle: &str) -> Vec<usize> {
        let needle_lower = needle.to_lowercase();
        self.elements
            .iter()
            .filter(|e| e.text.to_lowercase().contains(&needle_lower))
            .map(|e| e.index)
            .collect()
    }

    // =========================================================================
    // Actions (index-based)
    // =========================================================================

    /// Click an element by its index.
    pub async fn click(&self, index: usize) -> Result<()> {
        let el = self.require(index)?;
        self.page.click(&el.selector).await
    }

    /// Try to click — returns `Ok(false)` if element is missing or not visible.
    pub async fn try_click(&self, index: usize) -> Result<bool> {
        let el = self.require(index)?;
        self.page.try_click(&el.selector).await
    }

    /// Human-like click by index.
    pub async fn human_click(&self, index: usize) -> Result<()> {
        let el = self.require(index)?;
        self.page.human_click(&el.selector).await
    }

    /// Clear and type into an element by index.
    pub async fn fill(&self, index: usize, text: &str) -> Result<()> {
        let el = self.require(index)?;
        self.page.fill(&el.selector, text).await
    }

    /// Human-like fill by index.
    pub async fn human_fill(&self, index: usize, text: &str) -> Result<()> {
        let el = self.require(index)?;
        self.page.human_fill(&el.selector, text).await
    }

    /// Focus an element by index.
    pub async fn focus(&self, index: usize) -> Result<()> {
        let el = self.require(index)?;
        self.page
            .execute(&format!(
                "document.querySelector({})?.focus()",
                serde_json::to_string(&el.selector).unwrap()
            ))
            .await
    }

    /// Select a dropdown option by index. `value` matches the option's value or visible text.
    pub async fn select(&self, index: usize, value: &str) -> Result<()> {
        let el = self.require(index)?;
        let arg = serde_json::json!({ "sel": el.selector, "val": value });
        let js = format!(
            r#"(() => {{
                const arg = {arg};
                const sel = document.querySelector(arg.sel);
                if (!sel) return false;
                const opt = Array.from(sel.options).find(o => o.value === arg.val || o.text === arg.val);
                if (!opt) return false;
                sel.value = opt.value;
                sel.dispatchEvent(new Event('change', {{ bubbles: true }}));
                return true;
            }})()"#,
            arg = serde_json::to_string(&arg).unwrap()
        );
        let selected: bool = self.page.evaluate(&js).await?;
        if !selected {
            return Err(eoka::Error::ElementNotFound(format!(
                "option \"{}\" in element [{}]",
                value, index
            )));
        }
        Ok(())
    }

    /// Get dropdown options for a select element. Returns vec of (value, text) pairs.
    pub async fn options(&self, index: usize) -> Result<Vec<(String, String)>> {
        let el = self.require(index)?;
        let js = format!(
            r#"(() => {{
                const sel = document.querySelector({});
                if (!sel || !sel.options) return '[]';
                return JSON.stringify(Array.from(sel.options).map(o => [o.value, o.text]));
            }})()"#,
            serde_json::to_string(&el.selector).unwrap()
        );
        let json_str: String = self.page.evaluate(&js).await?;
        let pairs: Vec<(String, String)> = serde_json::from_str(&json_str)
            .map_err(|e| eoka::Error::CdpSimple(format!("options parse error: {}", e)))?;
        Ok(pairs)
    }

    /// Scroll element at index into view.
    pub async fn scroll_to(&self, index: usize) -> Result<()> {
        let el = self.require(index)?;
        let js = format!(
            "document.querySelector({})?.scrollIntoView({{behavior:'smooth',block:'center'}})",
            serde_json::to_string(&el.selector).unwrap()
        );
        self.page.execute(&js).await
    }

    // =========================================================================
    // Navigation
    // =========================================================================

    /// Navigate to a URL. Clears element list (call `observe()` after navigation).
    pub async fn goto(&mut self, url: &str) -> Result<()> {
        self.elements.clear();
        self.page.goto(url).await
    }

    /// Go back in history. Clears element list (call `observe()` after navigation).
    pub async fn back(&mut self) -> Result<()> {
        self.elements.clear();
        self.page.back().await
    }

    /// Go forward in history. Clears element list (call `observe()` after navigation).
    pub async fn forward(&mut self) -> Result<()> {
        self.elements.clear();
        self.page.forward().await
    }

    /// Reload the page. Clears element list (call `observe()` after navigation).
    pub async fn reload(&mut self) -> Result<()> {
        self.elements.clear();
        self.page.reload().await
    }

    // =========================================================================
    // Page state
    // =========================================================================

    /// Get the current URL.
    pub async fn url(&self) -> Result<String> {
        self.page.url().await
    }

    /// Get the page title.
    pub async fn title(&self) -> Result<String> {
        self.page.title().await
    }

    /// Get visible text content of the page.
    pub async fn text(&self) -> Result<String> {
        self.page.text().await
    }

    // =========================================================================
    // Scrolling
    // =========================================================================

    /// Scroll down by approximately one viewport height.
    pub async fn scroll_down(&self) -> Result<()> {
        self.page
            .execute("window.scrollBy(0, window.innerHeight * 0.8)")
            .await
    }

    /// Scroll up by approximately one viewport height.
    pub async fn scroll_up(&self) -> Result<()> {
        self.page
            .execute("window.scrollBy(0, -window.innerHeight * 0.8)")
            .await
    }

    /// Scroll to the top of the page.
    pub async fn scroll_to_top(&self) -> Result<()> {
        self.page.execute("window.scrollTo(0, 0)").await
    }

    /// Scroll to the bottom of the page.
    pub async fn scroll_to_bottom(&self) -> Result<()> {
        self.page
            .execute("window.scrollTo(0, document.body.scrollHeight)")
            .await
    }

    // =========================================================================
    // Waiting
    // =========================================================================

    /// Wait for text to appear on the page.
    pub async fn wait_for_text(&self, text: &str, timeout_ms: u64) -> Result<()> {
        self.page.wait_for_text(text, timeout_ms).await?;
        Ok(())
    }

    /// Wait for a URL pattern (substring match).
    pub async fn wait_for_url(&self, pattern: &str, timeout_ms: u64) -> Result<()> {
        self.page.wait_for_url_contains(pattern, timeout_ms).await
    }

    /// Wait for network activity to settle.
    pub async fn wait_for_idle(&self, timeout_ms: u64) -> Result<()> {
        self.page.wait_for_network_idle(500, timeout_ms).await
    }

    /// Fixed delay in milliseconds.
    pub async fn wait(&self, ms: u64) {
        self.page.wait(ms).await;
    }

    // =========================================================================
    // JavaScript
    // =========================================================================

    /// Evaluate JavaScript and return the result.
    pub async fn eval<T: serde::de::DeserializeOwned>(&self, js: &str) -> Result<T> {
        self.page.evaluate(js).await
    }

    /// Execute JavaScript (no return value).
    pub async fn exec(&self, js: &str) -> Result<()> {
        self.page.execute(js).await
    }

    // =========================================================================
    // Keyboard
    // =========================================================================

    /// Press a key (e.g. "Enter", "Tab", "Escape", "ArrowDown", "Backspace").
    pub async fn press_key(&self, key: &str) -> Result<()> {
        self.page.human().press_key(key).await
    }

    /// Focus element by index and press Enter (common for form submission).
    pub async fn submit(&self, index: usize) -> Result<()> {
        self.focus(index).await?;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        self.page.human().press_key("Enter").await
    }

    // =========================================================================
    // Hover
    // =========================================================================

    /// Hover over element by index (triggers hover states, tooltips, menus).
    pub async fn hover(&self, index: usize) -> Result<()> {
        let el = self.require(index)?;
        let cx = el.bbox.x + el.bbox.width / 2.0;
        let cy = el.bbox.y + el.bbox.height / 2.0;
        self.page
            .session()
            .dispatch_mouse_event(eoka::cdp::MouseEventType::MouseMoved, cx, cy, None, None)
            .await
    }

    // =========================================================================
    // Extraction
    // =========================================================================

    /// Extract structured data from the page using a JS expression that returns JSON.
    ///
    /// Example:
    /// ```rust,no_run
    /// # use eoka_agent::AgentPage;
    /// # async fn example(agent: &AgentPage<'_>) -> eoka::Result<()> {
    /// let titles: Vec<String> = agent.extract(
    ///     "Array.from(document.querySelectorAll('h2')).map(h => h.textContent.trim())"
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn extract<T: serde::de::DeserializeOwned>(&self, js_expression: &str) -> Result<T> {
        // Use eval() to handle multi-statement code - returns value of last expression
        // Safely escape the JS code to prevent injection
        let escaped_js = serde_json::to_string(js_expression)
            .map_err(|e| eoka::Error::CdpSimple(format!("Failed to escape JS: {}", e)))?;
        let js = format!("JSON.stringify(eval({}))", escaped_js);
        let json_str: String = self.page.evaluate(&js).await?;
        if json_str == "null" || json_str == "undefined" || json_str.is_empty() {
            return Err(eoka::Error::CdpSimple(format!(
                "extract returned null/undefined for: {}",
                if js_expression.len() > 60 {
                    &js_expression[..60]
                } else {
                    js_expression
                }
            )));
        }
        serde_json::from_str(&json_str).map_err(|e| {
            eoka::Error::CdpSimple(format!(
                "extract parse error: {} (got: {})",
                e,
                if json_str.len() > 80 {
                    &json_str[..80]
                } else {
                    &json_str
                }
            ))
        })
    }

    // =========================================================================
    // Smart Waiting
    // =========================================================================

    /// Wait for the page to stabilize after an action.
    /// Waits up to 2s for network idle, then 50ms for DOM settle.
    /// Intentionally succeeds even if network doesn't fully idle (some sites never stop polling).
    pub async fn wait_for_stable(&self) -> Result<()> {
        // Best-effort network wait - ignore timeout (some sites have constant polling)
        let _ = self.page.wait_for_network_idle(200, 2000).await;
        // Brief DOM settle time
        self.page.wait(50).await;
        Ok(())
    }

    /// Click an element and wait for page to stabilize.
    pub async fn click_and_wait(&mut self, index: usize) -> Result<()> {
        self.click(index).await?;
        self.wait_for_stable().await?;
        // Invalidate elements since page likely changed
        self.elements.clear();
        Ok(())
    }

    /// Fill an element and wait for page to stabilize.
    pub async fn fill_and_wait(&mut self, index: usize, text: &str) -> Result<()> {
        self.fill(index, text).await?;
        self.wait_for_stable().await?;
        Ok(())
    }

    /// Select an option and wait for page to stabilize.
    pub async fn select_and_wait(&mut self, index: usize, value: &str) -> Result<()> {
        self.select(index, value).await?;
        self.wait_for_stable().await?;
        Ok(())
    }

    // =========================================================================
    // SPA Navigation
    // =========================================================================

    /// Detect the SPA router type and current route state.
    pub async fn spa_info(&self) -> Result<SpaRouterInfo> {
        spa::detect_router(self.page).await
    }

    /// Navigate the SPA to a new path without page reload.
    /// Automatically detects the router type and uses the appropriate navigation method.
    /// Clears element list since the DOM will change.
    pub async fn spa_navigate(&mut self, path: &str) -> Result<String> {
        let info = spa::detect_router(self.page).await?;
        let result = spa::spa_navigate(self.page, &info.router_type, path).await?;
        self.elements.clear();
        Ok(result)
    }

    /// Navigate browser history by delta steps.
    /// delta = -1 goes back, delta = 1 goes forward.
    /// Clears element list since the DOM will change.
    pub async fn history_go(&mut self, delta: i32) -> Result<()> {
        spa::history_go(self.page, delta).await?;
        self.elements.clear();
        Ok(())
    }

    // =========================================================================
    // Internal
    // =========================================================================

    fn require(&self, index: usize) -> Result<&InteractiveElement> {
        self.elements.get(index).ok_or_else(|| {
            eoka::Error::ElementNotFound(format!(
                "element [{}] (observed {} elements — call observe() to refresh)",
                index,
                self.elements.len()
            ))
        })
    }
}

// =============================================================================
// Session - owns Browser and Page, no lifetime gymnastics
// =============================================================================

/// A browser session that owns its browser and page.
/// This is the primary API for most use cases.
pub struct Session {
    browser: Browser,
    page: Page,
    elements: Vec<InteractiveElement>,
    config: ObserveConfig,
}

impl Session {
    /// Launch a new browser and create an owned agent page.
    pub async fn launch() -> Result<Self> {
        let browser = Browser::launch().await?;
        let page = browser.new_page("about:blank").await?;
        Ok(Self {
            browser,
            page,
            elements: Vec::new(),
            config: ObserveConfig::default(),
        })
    }

    /// Launch with custom stealth config.
    pub async fn launch_with_config(stealth: StealthConfig) -> Result<Self> {
        let browser = Browser::launch_with_config(stealth).await?;
        let page = browser.new_page("about:blank").await?;
        Ok(Self {
            browser,
            page,
            elements: Vec::new(),
            config: ObserveConfig::default(),
        })
    }

    /// Set observation config.
    pub fn set_observe_config(&mut self, config: ObserveConfig) {
        self.config = config;
    }

    /// Get reference to underlying page.
    pub fn page(&self) -> &Page {
        &self.page
    }

    /// Get reference to browser.
    pub fn browser(&self) -> &Browser {
        &self.browser
    }

    // =========================================================================
    // Observation
    // =========================================================================

    /// Snapshot the page: enumerate all interactive elements.
    pub async fn observe(&mut self) -> Result<&[InteractiveElement]> {
        self.elements = observe::observe(&self.page, self.config.viewport_only).await?;
        Ok(&self.elements)
    }

    /// Take an annotated screenshot with numbered boxes on each element.
    pub async fn screenshot(&mut self) -> Result<Vec<u8>> {
        if self.elements.is_empty() {
            self.observe().await?;
        }
        annotate::annotated_screenshot(&self.page, &self.elements).await
    }

    /// Compact text list for LLM consumption.
    pub fn element_list(&self) -> String {
        let mut out = String::with_capacity(self.elements.len() * 40);
        for el in &self.elements {
            out.push_str(&el.to_string());
            out.push('\n');
        }
        out
    }

    /// Get element info by index.
    pub fn get(&self, index: usize) -> Option<&InteractiveElement> {
        self.elements.get(index)
    }

    /// Get all observed elements.
    pub fn elements(&self) -> &[InteractiveElement] {
        &self.elements
    }

    /// Number of observed elements.
    pub fn len(&self) -> usize {
        self.elements.len()
    }

    /// Whether the element list is empty.
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    /// Find first element whose text contains the given substring (case-insensitive).
    pub fn find_by_text(&self, needle: &str) -> Option<usize> {
        let needle_lower = needle.to_lowercase();
        self.elements
            .iter()
            .find(|e| e.text.to_lowercase().contains(&needle_lower))
            .map(|e| e.index)
    }

    // =========================================================================
    // Actions with auto-recovery
    // =========================================================================

    /// Get an element, verifying it still exists in DOM.
    /// If element moved, returns error with hint about new location.
    async fn require_fresh(&mut self, index: usize) -> Result<&InteractiveElement> {
        // First check if element exists at index
        let stored = self.elements.get(index).cloned();

        if let Some(ref el) = stored {
            // Verify the element still exists in DOM
            let js = format!(
                "!!document.querySelector({})",
                serde_json::to_string(&el.selector).unwrap()
            );
            let exists: bool = self.page.evaluate(&js).await.unwrap_or(false);

            if exists {
                return self.elements.get(index).ok_or_else(|| {
                    eoka::Error::ElementNotFound(format!("element [{}] disappeared", index))
                });
            }

            // Element gone from DOM - re-observe and look for it
            self.observe().await?;

            // Try to find element with matching fingerprint
            if let Some(new_idx) = self
                .elements
                .iter()
                .position(|e| e.fingerprint == el.fingerprint)
            {
                // Found at different index - error with helpful message
                return Err(eoka::Error::ElementNotFound(format!(
                    "element [{}] \"{}\" moved to [{}] - call observe() to refresh",
                    index, el.text, new_idx
                )));
            }

            return Err(eoka::Error::ElementNotFound(format!(
                "element [{}] \"{}\" no longer exists on page",
                index, el.text
            )));
        }

        Err(eoka::Error::ElementNotFound(format!(
            "element [{}] not found (observed {} elements)",
            index,
            self.elements.len()
        )))
    }

    /// Click an element, auto-recovering if stale.
    /// Clears element cache since clicks often trigger navigation/DOM changes.
    pub async fn click(&mut self, index: usize) -> Result<()> {
        let el = self.require_fresh(index).await?;
        let selector = el.selector.clone();
        self.page.click(&selector).await?;
        self.wait_for_stable().await?;
        self.elements.clear(); // Clicks often change the page
        Ok(())
    }

    /// Fill an element, auto-recovering if stale.
    /// Does NOT clear element cache (typing rarely changes DOM structure).
    pub async fn fill(&mut self, index: usize, text: &str) -> Result<()> {
        let el = self.require_fresh(index).await?;
        let selector = el.selector.clone();
        self.page.fill(&selector, text).await?;
        self.wait_for_stable().await?;
        Ok(())
    }

    /// Select a dropdown option, auto-recovering if stale.
    /// Clears element cache since onChange handlers may modify DOM.
    pub async fn select(&mut self, index: usize, value: &str) -> Result<()> {
        let el = self.require_fresh(index).await?;
        let selector = el.selector.clone();
        let arg = serde_json::json!({ "sel": selector, "val": value });
        let js = format!(
            r#"(() => {{
                const arg = {arg};
                const sel = document.querySelector(arg.sel);
                if (!sel) return false;
                const opt = Array.from(sel.options).find(o => o.value === arg.val || o.text === arg.val);
                if (!opt) return false;
                sel.value = opt.value;
                sel.dispatchEvent(new Event('change', {{ bubbles: true }}));
                return true;
            }})()"#,
            arg = serde_json::to_string(&arg).unwrap()
        );
        let selected: bool = self.page.evaluate(&js).await?;
        if !selected {
            return Err(eoka::Error::ElementNotFound(format!(
                "option \"{}\" in element [{}]",
                value, index
            )));
        }
        self.wait_for_stable().await?;
        self.elements.clear(); // onChange handlers may modify DOM
        Ok(())
    }

    /// Hover over element.
    pub async fn hover(&mut self, index: usize) -> Result<()> {
        let el = self.require_fresh(index).await?;
        let cx = el.bbox.x + el.bbox.width / 2.0;
        let cy = el.bbox.y + el.bbox.height / 2.0;
        self.page
            .session()
            .dispatch_mouse_event(eoka::cdp::MouseEventType::MouseMoved, cx, cy, None, None)
            .await
    }

    /// Scroll element into view.
    pub async fn scroll_to(&mut self, index: usize) -> Result<()> {
        let el = self.require_fresh(index).await?;
        let selector = el.selector.clone();
        let js = format!(
            "document.querySelector({})?.scrollIntoView({{behavior:'smooth',block:'center'}})",
            serde_json::to_string(&selector).unwrap()
        );
        self.page.execute(&js).await
    }

    // =========================================================================
    // Navigation
    // =========================================================================

    /// Navigate to a URL.
    pub async fn goto(&mut self, url: &str) -> Result<()> {
        self.elements.clear();
        self.page.goto(url).await?;
        self.wait_for_stable().await
    }

    /// Go back in history.
    pub async fn back(&mut self) -> Result<()> {
        self.elements.clear();
        self.page.back().await?;
        self.wait_for_stable().await
    }

    /// Go forward in history.
    pub async fn forward(&mut self) -> Result<()> {
        self.elements.clear();
        self.page.forward().await?;
        self.wait_for_stable().await
    }

    // =========================================================================
    // Page state
    // =========================================================================

    /// Get the current URL.
    pub async fn url(&self) -> Result<String> {
        self.page.url().await
    }

    /// Get the page title.
    pub async fn title(&self) -> Result<String> {
        self.page.title().await
    }

    /// Get visible text content of the page.
    pub async fn text(&self) -> Result<String> {
        self.page.text().await
    }

    // =========================================================================
    // Scrolling
    // =========================================================================

    /// Scroll down by approximately one viewport height.
    pub async fn scroll_down(&self) -> Result<()> {
        self.page
            .execute("window.scrollBy(0, window.innerHeight * 0.8)")
            .await
    }

    /// Scroll up by approximately one viewport height.
    pub async fn scroll_up(&self) -> Result<()> {
        self.page
            .execute("window.scrollBy(0, -window.innerHeight * 0.8)")
            .await
    }

    /// Scroll to top.
    pub async fn scroll_to_top(&self) -> Result<()> {
        self.page.execute("window.scrollTo(0, 0)").await
    }

    /// Scroll to bottom.
    pub async fn scroll_to_bottom(&self) -> Result<()> {
        self.page
            .execute("window.scrollTo(0, document.body.scrollHeight)")
            .await
    }

    // =========================================================================
    // Smart Waiting
    // =========================================================================

    /// Wait for the page to stabilize after an action.
    /// Waits up to 2s for network idle, then 50ms for DOM settle.
    /// Intentionally succeeds even if network doesn't fully idle (some sites never stop polling).
    pub async fn wait_for_stable(&self) -> Result<()> {
        // Best-effort network wait - ignore timeout (some sites have constant polling)
        let _ = self.page.wait_for_network_idle(200, 2000).await;
        // Brief DOM settle time
        self.page.wait(50).await;
        Ok(())
    }

    /// Fixed delay in milliseconds.
    pub async fn wait(&self, ms: u64) {
        self.page.wait(ms).await;
    }

    // =========================================================================
    // Keyboard
    // =========================================================================

    /// Press a key.
    pub async fn press_key(&self, key: &str) -> Result<()> {
        self.page.human().press_key(key).await
    }

    // =========================================================================
    // JavaScript
    // =========================================================================

    /// Evaluate JavaScript and return the result.
    pub async fn eval<T: serde::de::DeserializeOwned>(&self, js: &str) -> Result<T> {
        self.page.evaluate(js).await
    }

    /// Execute JavaScript (no return value).
    pub async fn exec(&self, js: &str) -> Result<()> {
        self.page.execute(js).await
    }

    // =========================================================================
    // SPA Navigation
    // =========================================================================

    /// Detect the SPA router type and current route state.
    pub async fn spa_info(&self) -> Result<SpaRouterInfo> {
        spa::detect_router(&self.page).await
    }

    /// Navigate the SPA to a new path without page reload.
    /// Automatically detects the router type and uses the appropriate navigation method.
    /// Clears element cache since the DOM will change.
    pub async fn spa_navigate(&mut self, path: &str) -> Result<String> {
        let info = spa::detect_router(&self.page).await?;
        let result = spa::spa_navigate(&self.page, &info.router_type, path).await?;
        self.elements.clear();
        Ok(result)
    }

    /// Navigate browser history by delta steps.
    /// delta = -1 goes back, delta = 1 goes forward.
    /// Clears element cache since the DOM will change.
    pub async fn history_go(&mut self, delta: i32) -> Result<()> {
        spa::history_go(&self.page, delta).await?;
        self.elements.clear();
        Ok(())
    }

    // =========================================================================
    // Cleanup
    // =========================================================================

    /// Close the browser.
    pub async fn close(self) -> Result<()> {
        self.browser.close().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_element(
        index: usize,
        tag: &str,
        text: &str,
        role: Option<&str>,
        input_type: Option<&str>,
        placeholder: Option<&str>,
        value: Option<&str>,
        checked: bool,
    ) -> InteractiveElement {
        let selector = format!("[data-idx=\"{}\"]", index);
        let fingerprint = InteractiveElement::compute_fingerprint(
            tag,
            text,
            role,
            input_type,
            placeholder,
            &selector,
        );
        InteractiveElement {
            index,
            tag: tag.to_string(),
            text: text.to_string(),
            role: role.map(|s| s.to_string()),
            input_type: input_type.map(|s| s.to_string()),
            placeholder: placeholder.map(|s| s.to_string()),
            value: value.map(|s| s.to_string()),
            checked,
            selector,
            bbox: BoundingBox {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 30.0,
            },
            fingerprint,
        }
    }

    #[test]
    fn test_element_display_basic() {
        let el = make_element(0, "button", "Submit", None, None, None, None, false);
        assert_eq!(el.to_string(), "[0] <button> \"Submit\"");
    }

    #[test]
    fn test_element_display_with_input_type() {
        // text type is suppressed
        let el = make_element(0, "input", "", None, Some("text"), None, None, false);
        assert_eq!(el.to_string(), "[0] <input>");

        // other types are shown
        let el = make_element(0, "input", "", None, Some("password"), None, None, false);
        assert_eq!(el.to_string(), "[0] <input type=\"password\">");
    }

    #[test]
    fn test_element_display_with_placeholder() {
        let el = make_element(
            0,
            "input",
            "",
            None,
            Some("text"),
            Some("Enter email"),
            None,
            false,
        );
        assert_eq!(el.to_string(), "[0] <input> placeholder=\"Enter email\"");
    }

    #[test]
    fn test_element_display_with_value() {
        let el = make_element(
            0,
            "input",
            "",
            None,
            Some("text"),
            None,
            Some("hello"),
            false,
        );
        assert_eq!(el.to_string(), "[0] <input> value=\"hello\"");
    }

    #[test]
    fn test_element_display_checked() {
        let el = make_element(0, "input", "", None, Some("checkbox"), None, None, true);
        assert_eq!(el.to_string(), "[0] <input type=\"checkbox\"> [checked]");
    }

    #[test]
    fn test_element_display_redundant_role_suppressed() {
        // button role on button tag is redundant
        let el = make_element(
            0,
            "button",
            "Click",
            Some("button"),
            None,
            None,
            None,
            false,
        );
        assert_eq!(el.to_string(), "[0] <button> \"Click\"");

        // link role on a tag is redundant
        let el = make_element(0, "a", "Link", Some("link"), None, None, None, false);
        assert_eq!(el.to_string(), "[0] <a> \"Link\"");

        // menuitem role on a tag is redundant
        let el = make_element(0, "a", "Menu", Some("menuitem"), None, None, None, false);
        assert_eq!(el.to_string(), "[0] <a> \"Menu\"");
    }

    #[test]
    fn test_element_display_non_redundant_role_shown() {
        // tab role on button is meaningful
        let el = make_element(0, "button", "Tab 1", Some("tab"), None, None, None, false);
        assert_eq!(el.to_string(), "[0] <button> \"Tab 1\" role=\"tab\"");

        // button role on div is meaningful
        let el = make_element(0, "div", "Click", Some("button"), None, None, None, false);
        assert_eq!(el.to_string(), "[0] <div> \"Click\" role=\"button\"");
    }

    #[test]
    fn test_observe_diff_display_no_changes() {
        let diff = ObserveDiff {
            added: vec![],
            removed: 0,
            total: 5,
        };
        assert_eq!(diff.to_string(), "no changes (5 elements)");
    }

    #[test]
    fn test_observe_diff_display_added_only() {
        let diff = ObserveDiff {
            added: vec![5, 6],
            removed: 0,
            total: 7,
        };
        assert_eq!(diff.to_string(), "+2 added (7 total)");
    }

    #[test]
    fn test_observe_diff_display_removed_only() {
        let diff = ObserveDiff {
            added: vec![],
            removed: 3,
            total: 2,
        };
        assert_eq!(diff.to_string(), "-3 removed (2 total)");
    }

    #[test]
    fn test_observe_diff_display_both() {
        let diff = ObserveDiff {
            added: vec![3, 4],
            removed: 1,
            total: 5,
        };
        assert_eq!(diff.to_string(), "+2 added, -1 removed (5 total)");
    }

    #[test]
    fn test_observe_config_default() {
        let config = ObserveConfig::default();
        assert!(config.viewport_only);
    }
}
