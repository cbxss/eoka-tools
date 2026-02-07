use crate::config::actions::{
    EmailAction, EmailExtractAction, EmailFilterAction, ImapConfigAction, ScrollDirection, Target,
    TryClickAnyAction, WaitForEmailAction,
};
use crate::config::{Action, Config, Params};
use crate::{Error, Result};
use chrono::Duration as ChronoDuration;
use eoka::Page;
use eoka_email::{
    extract_code, extract_first_link, AsyncImapClient, ImapConfig, LinkFilter, SearchCriteria,
    WaitOptions,
};
use regex::Regex;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

impl From<&ImapConfigAction> for ImapConfig {
    fn from(a: &ImapConfigAction) -> Self {
        Self {
            host: a.host.clone(),
            port: a.port,
            tls: a.tls,
            username: a.username.clone(),
            password: a.password.clone(),
            mailbox: a.mailbox.clone(),
        }
    }
}

/// Maximum include depth to prevent infinite loops.
const MAX_INCLUDE_DEPTH: usize = 10;

/// Context for action execution.
#[derive(Clone)]
pub struct ExecutionContext {
    /// Base path for resolving relative includes.
    pub base_path: PathBuf,
    /// Current include depth.
    pub include_depth: usize,
}

impl ExecutionContext {
    /// Create a new context with a base path.
    pub fn new(base_path: impl Into<PathBuf>) -> Self {
        Self {
            base_path: base_path.into(),
            include_depth: 0,
        }
    }

    /// Create a child context for an include.
    pub fn child(&self, new_base: impl Into<PathBuf>) -> Result<Self> {
        if self.include_depth >= MAX_INCLUDE_DEPTH {
            return Err(Error::Config(format!(
                "maximum include depth ({}) exceeded",
                MAX_INCLUDE_DEPTH
            )));
        }
        Ok(Self {
            base_path: new_base.into(),
            include_depth: self.include_depth + 1,
        })
    }

    /// Resolve a relative path against the base path.
    pub fn resolve_path(&self, path: &str) -> PathBuf {
        let path = Path::new(path);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.base_path.join(path)
        }
    }
}

impl Default for ExecutionContext {
    fn default() -> Self {
        Self::new(std::env::current_dir().unwrap_or_default())
    }
}

/// Find element by text - returns CSS selector.
const FIND_BY_TEXT_JS: &str = r#"(() => {
    const text = arguments[0];
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_ELEMENT, null);
    while (walker.nextNode()) {
        const el = walker.currentNode;
        if (el.textContent?.trim().toLowerCase().includes(text.toLowerCase())) {
            if (el.matches('a, button, input, select, [role="button"], [onclick]')) {
                if (el.id) return '#' + el.id;
                const path = [];
                let node = el;
                while (node && node !== document.body) {
                    let selector = node.tagName.toLowerCase();
                    if (node.id) {
                        path.unshift('#' + node.id);
                        break;
                    }
                    const siblings = Array.from(node.parentNode?.children || []);
                    const index = siblings.indexOf(node) + 1;
                    if (siblings.length > 1) selector += ':nth-child(' + index + ')';
                    path.unshift(selector);
                    node = node.parentNode;
                }
                return path.join(' > ');
            }
        }
    }
    return null;
})()"#;

/// Execute a single action on the page with context.
pub async fn execute_with_context(
    page: &Page,
    action: &Action,
    ctx: &ExecutionContext,
) -> Result<()> {
    match action {
        Action::Goto(a) => {
            info!("goto: {}", a.url);
            page.goto(&a.url).await?;
        }
        Action::Back => {
            debug!("back");
            page.back().await?;
        }
        Action::Forward => {
            debug!("forward");
            page.forward().await?;
        }
        Action::Reload => {
            debug!("reload");
            page.reload().await?;
        }
        Action::Wait(a) => {
            debug!("wait: {}ms", a.ms);
            page.wait(a.ms).await;
        }
        Action::WaitForNetworkIdle(a) => {
            debug!(
                "wait_for_network_idle: idle={}ms, timeout={}ms",
                a.idle_ms, a.timeout_ms
            );
            page.wait_for_network_idle(a.idle_ms, a.timeout_ms).await?;
        }
        Action::WaitForText(a) => {
            debug!("wait_for_text: '{}'", a.text);
            page.wait_for_text(&a.text, a.timeout_ms).await?;
        }
        Action::WaitForUrl(a) => {
            debug!("wait_for_url: contains '{}'", a.contains);
            page.wait_for_url_contains(&a.contains, a.timeout_ms)
                .await?;
        }
        Action::WaitForEmail(a) => {
            wait_for_email(page, a).await?;
        }
        Action::Click(a) => {
            let selector = resolve_target(page, &a.target).await?;
            info!("click: {}", a.target);
            if a.scroll_into_view {
                scroll_into_view(page, &selector).await?;
            }
            if a.human {
                page.human_click(&selector).await?;
            } else {
                page.click(&selector).await?;
            }
        }
        Action::TryClick(a) => {
            debug!("try_click: {}", a.target);
            if let Ok(selector) = resolve_target(page, &a.target).await {
                let _ = page.try_click(&selector).await;
            }
        }
        Action::TryClickAny(a) => {
            debug!(
                "try_click_any: {:?}",
                a.texts.as_ref().or(a.selectors.as_ref())
            );
            try_click_any(page, a).await?;
        }
        Action::Fill(a) => {
            info!("fill: {} = '{}'", a.target, a.value);
            let selector = resolve_target(page, &a.target).await?;
            if a.human {
                page.human_fill(&selector, &a.value).await?;
            } else {
                page.fill(&selector, &a.value).await?;
            }
        }
        Action::Type(a) => {
            debug!("type: {} = '{}'", a.target, a.value);
            let selector = resolve_target(page, &a.target).await?;
            focus_element(page, &selector).await?;
            page.type_text(&a.value).await?;
        }
        Action::Clear(a) => {
            debug!("clear: {}", a.target);
            let selector = resolve_target(page, &a.target).await?;
            page.fill(&selector, "").await?;
        }
        Action::Select(a) => {
            info!("select: {} = '{}'", a.target, a.value);
            let selector = resolve_target(page, &a.target).await?;
            select_option(page, &selector, &a.value, &a.target).await?;
        }
        Action::PressKey(a) => {
            debug!("press_key: {}", a.key);
            page.human().press_key(&a.key).await?;
        }
        Action::Hover(a) => {
            debug!("hover: {}", a.target);
            let selector = resolve_target(page, &a.target).await?;
            hover_element(page, &selector).await?;
        }
        Action::SetCookie(a) => {
            debug!("set_cookie: {}={}", a.name, a.value);
            page.set_cookie(&a.name, &a.value, a.domain.as_deref(), a.path.as_deref())
                .await?;
        }
        Action::DeleteCookie(a) => {
            debug!("delete_cookie: {}", a.name);
            page.delete_cookie(&a.name, a.domain.as_deref()).await?;
        }
        Action::Execute(a) => {
            debug!("execute: {}...", &a.js[..a.js.len().min(50)]);
            page.execute(&a.js).await?;
        }
        Action::Screenshot(a) => {
            info!("screenshot: {}", a.path);
            let data = page.screenshot().await?;
            std::fs::write(&a.path, data)?;
        }
        Action::Log(a) => {
            info!("[log] {}", a.message);
        }
        Action::AssertText(a) => {
            debug!("assert_text: '{}'", a.text);
            let text = page.text().await?;
            if !text.contains(&a.text) {
                return Err(Error::AssertionFailed(format!(
                    "text '{}' not found",
                    a.text
                )));
            }
        }
        Action::AssertUrl(a) => {
            debug!("assert_url: contains '{}'", a.contains);
            let url = page.url().await?;
            if !url.contains(&a.contains) {
                return Err(Error::AssertionFailed(format!(
                    "url does not contain '{}'",
                    a.contains
                )));
            }
        }
        Action::Scroll(a) => {
            debug!("scroll: {:?} x{}", a.direction, a.amount);
            scroll(page, &a.direction, a.amount).await?;
        }
        Action::ScrollTo(a) => {
            debug!("scroll_to: {}", a.target);
            let selector = resolve_target(page, &a.target).await?;
            scroll_into_view(page, &selector).await?;
        }
        Action::WaitFor(a) => {
            debug!("wait_for: {}", a.selector);
            page.wait_for(&a.selector, a.timeout_ms).await?;
        }
        Action::WaitForVisible(a) => {
            debug!("wait_for_visible: {}", a.selector);
            page.wait_for_visible(&a.selector, a.timeout_ms).await?;
        }
        Action::WaitForHidden(a) => {
            debug!("wait_for_hidden: {}", a.selector);
            page.wait_for_hidden(&a.selector, a.timeout_ms).await?;
        }
        Action::IfTextExists(a) => {
            let text = page.text().await?;
            let exists = text.contains(&a.text);
            debug!("if_text_exists '{}': {}", a.text, exists);
            let actions = if exists {
                &a.then_actions
            } else {
                &a.else_actions
            };
            for action in actions {
                Box::pin(execute_with_context(page, action, ctx)).await?;
            }
        }
        Action::IfSelectorExists(a) => {
            let exists = element_exists(page, &a.selector).await?;
            debug!("if_selector_exists '{}': {}", a.selector, exists);
            let actions = if exists {
                &a.then_actions
            } else {
                &a.else_actions
            };
            for action in actions {
                Box::pin(execute_with_context(page, action, ctx)).await?;
            }
        }
        Action::Repeat(a) => {
            debug!("repeat: {} times", a.times);
            for i in 0..a.times {
                debug!("repeat iteration {}/{}", i + 1, a.times);
                for action in &a.actions {
                    Box::pin(execute_with_context(page, action, ctx)).await?;
                }
            }
        }
        Action::Include(a) => {
            let path = ctx.resolve_path(&a.path);
            info!("include: {}", path.display());

            // Build params from the include action
            let mut params = Params::new();
            for (k, v) in &a.params {
                params = params.set(k.clone(), v.clone());
            }

            // Load the included config
            let included_config = Config::load_with_params(&path, &params).map_err(|e| {
                Error::Config(format!(
                    "failed to load include '{}': {}",
                    path.display(),
                    e
                ))
            })?;

            // Create child context with the included file's directory as base
            let child_base = path.parent().unwrap_or(Path::new("."));
            let child_ctx = ctx.child(child_base)?;

            // Execute included actions
            for action in &included_config.actions {
                Box::pin(execute_with_context(page, action, &child_ctx)).await?;
            }
        }
    }
    Ok(())
}

async fn wait_for_email(page: &Page, action: &WaitForEmailAction) -> Result<()> {
    let imap = ImapConfig::from(&action.imap);

    let criteria = build_email_criteria(&action.filter);

    let options = WaitOptions::new(
        ChronoDuration::milliseconds(action.timeout_ms as i64),
        ChronoDuration::milliseconds(action.poll_interval_ms as i64),
    );

    let mut client = AsyncImapClient::connect(&imap)
        .await
        .map_err(|e| Error::ActionFailed(e.to_string()))?;

    let msg = client
        .wait_for_message(&criteria, &options)
        .await
        .map_err(|e| Error::ActionFailed(e.to_string()))?;

    let (link, code) = extract_email_values(&msg, &action.extract)?;

    match &action.action {
        Some(EmailAction::OpenLink(_)) => {
            let link = link.ok_or_else(|| {
                Error::ActionFailed("no link extracted from email".into())
            })?;
            info!("email link: {}", link);
            page.goto(&link).await?;
        }
        Some(EmailAction::Fill(fill)) => {
            let code = code.ok_or_else(|| {
                Error::ActionFailed("no code extracted from email".into())
            })?;
            info!("email code: {}", code);
            page.fill(&fill.selector, &code).await?;
        }
        None => {
            if let Some(link) = link {
                info!("email link: {}", link);
            }
            if let Some(code) = code {
                info!("email code: {}", code);
            }
        }
    }

    Ok(())
}

fn build_email_criteria(filter: &EmailFilterAction) -> SearchCriteria {
    let mut criteria = SearchCriteria::new()
        .unseen_only(filter.unseen_only)
        .mark_seen(filter.mark_seen);

    if let Some(ref from) = filter.from {
        criteria = criteria.from(from.clone());
    }
    if let Some(ref subject) = filter.subject_contains {
        criteria = criteria.subject_contains(subject.clone());
    }
    if let Some(minutes) = filter.since_minutes {
        criteria = criteria.since_minutes(minutes);
    }

    criteria
}

fn extract_email_values(
    msg: &eoka_email::EmailMessage,
    extract: &EmailExtractAction,
) -> Result<(Option<String>, Option<String>)> {
    let mut link: Option<String> = None;
    let mut code: Option<String> = None;

    if let Some(ref link_cfg) = extract.link {
        let filter = LinkFilter {
            allow_domains: link_cfg.allow_domains.clone(),
        };
        link = extract_first_link(msg, &filter);
    }

    if let Some(ref code_cfg) = extract.code {
        let re = Regex::new(&code_cfg.regex)
            .map_err(|e| Error::ActionFailed(format!("invalid code regex: {}", e)))?;
        code = extract_code(msg, &re);
    }

    Ok((link, code))
}

/// Resolve a Target to a CSS selector.
pub async fn resolve_target(page: &Page, target: &Target) -> Result<String> {
    if let Some(ref sel) = target.selector {
        return Ok(sel.clone());
    }
    if let Some(ref txt) = target.text {
        let js = FIND_BY_TEXT_JS.replace("arguments[0]", &serde_json::to_string(txt).unwrap());
        let result: Option<String> = page.evaluate(&js).await?;
        if let Some(sel) = result {
            return Ok(sel);
        }
        return Err(Error::ActionFailed(format!(
            "element with text '{}' not found",
            txt
        )));
    }
    Err(Error::ActionFailed(
        "either selector or text must be provided".into(),
    ))
}

async fn focus_element(page: &Page, selector: &str) -> Result<()> {
    let js = format!(
        "document.querySelector({})?.focus()",
        serde_json::to_string(selector).unwrap()
    );
    page.execute(&js).await?;
    Ok(())
}

async fn element_exists(page: &Page, selector: &str) -> Result<bool> {
    let js = format!(
        "!!document.querySelector({})",
        serde_json::to_string(selector).unwrap()
    );
    Ok(page.evaluate(&js).await?)
}

async fn scroll_into_view(page: &Page, selector: &str) -> Result<()> {
    let js = format!(
        "document.querySelector({})?.scrollIntoView({{behavior:'smooth',block:'center'}})",
        serde_json::to_string(selector).unwrap()
    );
    page.execute(&js).await?;
    page.wait(200).await;
    Ok(())
}

async fn scroll(page: &Page, direction: &ScrollDirection, amount: u32) -> Result<()> {
    let (x, y) = match direction {
        ScrollDirection::Up => (0, -(amount as i32 * 300)),
        ScrollDirection::Down => (0, amount as i32 * 300),
        ScrollDirection::Left => (-(amount as i32 * 300), 0),
        ScrollDirection::Right => (amount as i32 * 300, 0),
    };
    page.execute(&format!("window.scrollBy({x}, {y})")).await?;
    Ok(())
}

async fn try_click_any(page: &Page, action: &TryClickAnyAction) -> Result<()> {
    if let Some(ref selectors) = action.selectors {
        for sel in selectors {
            if page.try_click(sel).await? {
                debug!("try_click_any: clicked selector '{}'", sel);
                return Ok(());
            }
        }
    }
    if let Some(ref texts) = action.texts {
        for txt in texts {
            let target = Target {
                selector: None,
                text: Some(txt.clone()),
            };
            if let Ok(sel) = resolve_target(page, &target).await {
                if page.try_click(&sel).await? {
                    debug!("try_click_any: clicked text '{}'", txt);
                    return Ok(());
                }
            }
        }
    }
    debug!("try_click_any: no element found");
    Ok(())
}

async fn select_option(page: &Page, selector: &str, value: &str, target: &Target) -> Result<()> {
    let js = format!(
        r#"(() => {{
            const sel = document.querySelector({sel});
            if (!sel) return 'element_not_found';
            const opt = Array.from(sel.options).find(o => o.value === {val} || o.text === {val});
            if (!opt) return 'option_not_found';
            sel.value = opt.value;
            sel.dispatchEvent(new Event('change', {{ bubbles: true }}));
            return 'ok';
        }})()"#,
        sel = serde_json::to_string(selector).unwrap(),
        val = serde_json::to_string(value).unwrap()
    );
    let result: String = page.evaluate(&js).await?;
    match result.as_str() {
        "ok" => Ok(()),
        "element_not_found" => Err(Error::ActionFailed(format!(
            "select element '{}' not found",
            target
        ))),
        "option_not_found" => Err(Error::ActionFailed(format!(
            "option '{}' not found in select",
            value
        ))),
        _ => Err(Error::ActionFailed(format!("select failed: {}", result))),
    }
}

async fn hover_element(page: &Page, selector: &str) -> Result<()> {
    let js = format!(
        r#"(() => {{
            const el = document.querySelector({});
            if (!el) return null;
            const rect = el.getBoundingClientRect();
            return {{ x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 }};
        }})()"#,
        serde_json::to_string(selector).unwrap()
    );
    let coords: Option<serde_json::Value> = page.evaluate(&js).await?;
    if let Some(c) = coords {
        let x = c["x"].as_f64().unwrap_or(0.0);
        let y = c["y"].as_f64().unwrap_or(0.0);
        page.session()
            .dispatch_mouse_event(eoka::cdp::MouseEventType::MouseMoved, x, y, None, None)
            .await?;
        page.wait(100).await;
        Ok(())
    } else {
        Err(Error::ActionFailed(format!(
            "hover target '{}' not found",
            selector
        )))
    }
}
