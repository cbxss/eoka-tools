use crate::{annotate, observe, snapshot, spa};

pub use crate::spa::{RouterType, SpaRouterInfo};
pub use crate::target::{BBox, LivePattern, Resolved, Target};

use std::collections::HashSet;
use std::fmt;

use crate::eoka::{BoundingBox, Page, Result};

pub use crate::eoka::{Browser, Error, StealthConfig};

#[derive(Debug, Clone)]
pub struct InteractiveElement {
    pub index: usize,
    pub tag: String,
    pub role: Option<String>,
    pub text: String,
    pub placeholder: Option<String>,
    pub input_type: Option<String>,
    pub selector: String,
    pub checked: bool,
    pub value: Option<String>,
    pub bbox: BoundingBox,
    pub fingerprint: u64,
}

impl InteractiveElement {
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
        selector.hash(&mut hasher);
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

#[derive(Debug, Clone)]
pub struct ObserveConfig {
    pub viewport_only: bool,
}

impl Default for ObserveConfig {
    fn default() -> Self {
        Self {
            viewport_only: true,
        }
    }
}

#[derive(Debug)]
pub struct ObserveDiff {
    pub added: Vec<usize>,
    pub removed: usize,
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

pub struct Session {
    browser: Browser,
    page: Page,
    elements: Vec<InteractiveElement>,
    config: ObserveConfig,
}

impl Session {
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

    pub fn set_observe_config(&mut self, config: ObserveConfig) {
        self.config = config;
    }

    pub fn page(&self) -> &Page {
        &self.page
    }

    pub fn browser(&self) -> &Browser {
        &self.browser
    }

    pub async fn ax_snapshot(&self, include_all: bool) -> anyhow::Result<snapshot::SnapshotResult> {
        snapshot::snapshot(&self.page, include_all).await
    }

    pub async fn observe(&mut self) -> Result<&[InteractiveElement]> {
        self.elements = observe::observe(&self.page, self.config.viewport_only).await?;
        Ok(&self.elements)
    }

    pub async fn screenshot(&mut self) -> Result<Vec<u8>> {
        if self.elements.is_empty() {
            self.observe().await?;
        }
        annotate::annotated_screenshot(&self.page, &self.elements).await
    }

    pub fn element_list(&self) -> String {
        let mut out = String::with_capacity(self.elements.len() * 40);
        for el in &self.elements {
            out.push_str(&el.to_string());
            out.push('\n');
        }
        out
    }

    pub fn get(&self, index: usize) -> Option<&InteractiveElement> {
        self.elements.get(index)
    }

    pub fn elements(&self) -> &[InteractiveElement] {
        &self.elements
    }

    pub fn len(&self) -> usize {
        self.elements.len()
    }

    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    pub fn find_by_text(&self, needle: &str) -> Option<usize> {
        let needle_lower = needle.to_lowercase();
        self.elements
            .iter()
            .find(|e| e.text.to_lowercase().contains(&needle_lower))
            .map(|e| e.index)
    }

    pub fn find_all_by_text(&self, needle: &str) -> Vec<usize> {
        let needle_lower = needle.to_lowercase();
        self.elements
            .iter()
            .filter(|e| e.text.to_lowercase().contains(&needle_lower))
            .map(|e| e.index)
            .collect()
    }

    pub async fn observe_diff(&mut self) -> Result<ObserveDiff> {
        let old_selectors: HashSet<String> =
            self.elements.iter().map(|e| e.selector.clone()).collect();

        self.elements = observe::observe(&self.page, self.config.viewport_only).await?;

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

    pub async fn screenshot_plain(&self) -> Result<Vec<u8>> {
        self.page.screenshot().await
    }

    async fn require_fresh(&mut self, index: usize) -> Result<&InteractiveElement> {
        let stored = self.elements.get(index).cloned();

        if let Some(ref el) = stored {
            let js = format!(
                "!!document.querySelector({})",
                serde_json::to_string(&el.selector).unwrap()
            );
            let exists: bool = self.page.evaluate(&js).await.unwrap_or(false);

            if exists {
                return self.elements.get(index).ok_or_else(|| {
                    crate::eoka::Error::ElementNotFound(format!("element [{}] disappeared", index))
                });
            }

            self.observe().await?;

            if let Some(new_idx) = self
                .elements
                .iter()
                .position(|e| e.fingerprint == el.fingerprint)
            {
                return Err(crate::eoka::Error::ElementNotFound(format!(
                    "element [{}] \"{}\" moved to [{}] - call observe() to refresh",
                    index, el.text, new_idx
                )));
            }

            return Err(crate::eoka::Error::ElementNotFound(format!(
                "element [{}] \"{}\" no longer exists on page",
                index, el.text
            )));
        }

        Err(crate::eoka::Error::ElementNotFound(format!(
            "element [{}] not found (observed {} elements)",
            index,
            self.elements.len()
        )))
    }

    pub async fn click(&mut self, index: usize) -> Result<()> {
        let el = self.require_fresh(index).await?;
        let selector = el.selector.clone();
        self.page.click(&selector).await?;
        self.wait_for_stable().await?;
        self.elements.clear();
        Ok(())
    }

    pub async fn fill(&mut self, index: usize, text: &str) -> Result<()> {
        let el = self.require_fresh(index).await?;
        let selector = el.selector.clone();
        self.page.fill(&selector, text).await?;
        self.wait_for_stable().await?;
        Ok(())
    }

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
            return Err(crate::eoka::Error::ElementNotFound(format!(
                "option \"{}\" in element [{}]",
                value, index
            )));
        }
        self.wait_for_stable().await?;
        self.elements.clear();
        Ok(())
    }

    pub async fn hover(&mut self, index: usize) -> Result<()> {
        let selector = self.require_fresh(index).await?.selector.clone();
        self.page.hover(&selector).await
    }

    pub async fn scroll_to(&mut self, index: usize) -> Result<()> {
        let el = self.require_fresh(index).await?;
        let selector = el.selector.clone();
        let js = format!(
            "document.querySelector({})?.scrollIntoView({{behavior:'smooth',block:'center'}})",
            serde_json::to_string(&selector).unwrap()
        );
        self.page.execute(&js).await
    }

    pub async fn try_click(&mut self, index: usize) -> Result<bool> {
        let el = self.require_fresh(index).await?;
        let selector = el.selector.clone();
        self.page.try_click(&selector).await
    }

    pub async fn human_click(&mut self, index: usize) -> Result<()> {
        let el = self.require_fresh(index).await?;
        let selector = el.selector.clone();
        self.page.human_click(&selector).await
    }

    pub async fn human_fill(&mut self, index: usize, text: &str) -> Result<()> {
        let el = self.require_fresh(index).await?;
        let selector = el.selector.clone();
        self.page.human_fill(&selector, text).await
    }

    pub async fn focus(&mut self, index: usize) -> Result<()> {
        let el = self.require_fresh(index).await?;
        let selector = el.selector.clone();
        self.page
            .execute(&format!(
                "document.querySelector({})?.focus()",
                serde_json::to_string(&selector).unwrap()
            ))
            .await
    }

    pub async fn submit(&mut self, index: usize) -> Result<()> {
        self.focus(index).await?;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        self.page.human().press_key("Enter").await
    }

    pub async fn options(&mut self, index: usize) -> Result<Vec<(String, String)>> {
        let el = self.require_fresh(index).await?;
        let selector = el.selector.clone();
        let js = format!(
            r#"(() => {{
                const sel = document.querySelector({});
                if (!sel || !sel.options) return '[]';
                return JSON.stringify(Array.from(sel.options).map(o => [o.value, o.text]));
            }})()"#,
            serde_json::to_string(&selector).unwrap()
        );
        let json_str: String = self.page.evaluate(&js).await?;
        let pairs: Vec<(String, String)> = serde_json::from_str(&json_str)
            .map_err(|e| crate::eoka::Error::cdp_msg(format!("options parse error: {}", e)))?;
        Ok(pairs)
    }

    pub async fn goto(&mut self, url: &str) -> Result<()> {
        self.elements.clear();
        self.page.goto(url).await?;
        self.wait_for_stable().await
    }

    pub async fn back(&mut self) -> Result<()> {
        self.elements.clear();
        self.page.back().await?;
        self.wait_for_stable().await
    }

    pub async fn forward(&mut self) -> Result<()> {
        self.elements.clear();
        self.page.forward().await?;
        self.wait_for_stable().await
    }

    pub async fn reload(&mut self) -> Result<()> {
        self.elements.clear();
        self.page.reload().await?;
        self.wait_for_stable().await
    }

    pub async fn url(&self) -> Result<String> {
        self.page.url().await
    }

    pub async fn title(&self) -> Result<String> {
        self.page.title().await
    }

    pub async fn text(&self) -> Result<String> {
        self.page.text().await
    }

    pub async fn scroll_down(&self) -> Result<()> {
        self.page
            .execute("window.scrollBy(0, window.innerHeight * 0.8)")
            .await
    }

    pub async fn scroll_up(&self) -> Result<()> {
        self.page
            .execute("window.scrollBy(0, -window.innerHeight * 0.8)")
            .await
    }

    pub async fn scroll_to_top(&self) -> Result<()> {
        self.page.execute("window.scrollTo(0, 0)").await
    }

    pub async fn scroll_to_bottom(&self) -> Result<()> {
        self.page
            .execute("window.scrollTo(0, document.body.scrollHeight)")
            .await
    }

    pub async fn wait_for_stable(&self) -> Result<()> {
        let _ = self.page.wait_for_network_idle(200, 2000).await;
        self.page.wait(50).await;
        Ok(())
    }

    pub async fn wait(&self, ms: u64) {
        self.page.wait(ms).await;
    }

    pub async fn wait_for_text(&self, text: &str, timeout_ms: u64) -> Result<()> {
        self.page.wait_for_text(text, timeout_ms).await?;
        Ok(())
    }

    pub async fn wait_for_url(&self, pattern: &str, timeout_ms: u64) -> Result<()> {
        self.page.wait_for_url_contains(pattern, timeout_ms).await
    }

    pub async fn wait_for_idle(&self, timeout_ms: u64) -> Result<()> {
        self.page.wait_for_network_idle(500, timeout_ms).await
    }

    pub async fn press_key(&self, key: &str) -> Result<()> {
        self.page.human().press_key(key).await
    }

    pub async fn eval<T: serde::de::DeserializeOwned>(&self, js: &str) -> Result<T> {
        self.page.evaluate(js).await
    }

    pub async fn exec(&self, js: &str) -> Result<()> {
        self.page.execute(js).await
    }

    pub async fn extract<T: serde::de::DeserializeOwned>(&self, js_expression: &str) -> Result<T> {
        let escaped_js = serde_json::to_string(js_expression)
            .map_err(|e| crate::eoka::Error::cdp_msg(format!("Failed to escape JS: {}", e)))?;
        let js = format!("JSON.stringify(eval({}))", escaped_js);
        let json_str: String = self.page.evaluate(&js).await?;
        if json_str == "null" || json_str == "undefined" || json_str.is_empty() {
            return Err(crate::eoka::Error::cdp_msg(format!(
                "extract returned null/undefined for: {}",
                if js_expression.len() > 60 {
                    &js_expression[..60]
                } else {
                    js_expression
                }
            )));
        }
        serde_json::from_str(&json_str).map_err(|e| {
            crate::eoka::Error::cdp_msg(format!(
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

    pub async fn spa_info(&self) -> Result<SpaRouterInfo> {
        spa::detect_router(&self.page).await
    }

    pub async fn spa_navigate(&mut self, path: &str) -> Result<String> {
        let info = spa::detect_router(&self.page).await?;
        let result = spa::spa_navigate(&self.page, &info.router_type, path).await?;
        self.elements.clear();
        Ok(result)
    }

    pub async fn history_go(&mut self, delta: i32) -> Result<()> {
        spa::history_go(&self.page, delta).await?;
        self.elements.clear();
        Ok(())
    }

    pub async fn close(self) -> Result<()> {
        self.browser.close().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::too_many_arguments)]
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
        let el = make_element(0, "input", "", None, Some("text"), None, None, false);
        assert_eq!(el.to_string(), "[0] <input>");

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

        let el = make_element(0, "a", "Link", Some("link"), None, None, None, false);
        assert_eq!(el.to_string(), "[0] <a> \"Link\"");

        let el = make_element(0, "a", "Menu", Some("menuitem"), None, None, None, false);
        assert_eq!(el.to_string(), "[0] <a> \"Menu\"");
    }

    #[test]
    fn test_element_display_non_redundant_role_shown() {
        let el = make_element(0, "button", "Tab 1", Some("tab"), None, None, None, false);
        assert_eq!(el.to_string(), "[0] <button> \"Tab 1\" role=\"tab\"");

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

    #[test]
    fn test_fingerprint_uses_full_selector() {
        let base = "a".repeat(50);
        let sel_a = format!("{}AAAA", base);
        let sel_b = format!("{}BBBB", base);

        let fp_a = InteractiveElement::compute_fingerprint("button", "X", None, None, None, &sel_a);
        let fp_b = InteractiveElement::compute_fingerprint("button", "X", None, None, None, &sel_b);

        assert_ne!(
            fp_a, fp_b,
            "selectors differing after char 50 should produce different fingerprints"
        );
    }
}
