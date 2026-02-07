use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::fmt;

/// A target element - either by CSS selector or visible text.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct Target {
    /// CSS selector.
    pub selector: Option<String>,
    /// Visible text to find.
    pub text: Option<String>,
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.selector, &self.text) {
            (Some(s), _) => write!(f, "selector '{}'", s),
            (_, Some(t)) => write!(f, "text '{}'", t),
            _ => write!(f, "unknown"),
        }
    }
}

/// An action to execute in the browser.
#[derive(Debug, Clone)]
pub enum Action {
    // Navigation
    Goto(GotoAction),
    Back,
    Forward,
    Reload,

    // Waiting
    Wait(WaitAction),
    WaitForNetworkIdle(WaitForNetworkIdleAction),
    WaitFor(WaitForAction),
    WaitForVisible(WaitForAction),
    WaitForHidden(WaitForAction),
    WaitForText(WaitForTextAction),
    WaitForUrl(WaitForUrlAction),
    WaitForEmail(WaitForEmailAction),

    // Clicking
    Click(ClickAction),
    TryClick(TargetAction),
    TryClickAny(TryClickAnyAction),

    // Input
    Fill(FillAction),
    Type(TypeAction),
    Clear(ClearAction),
    Select(SelectAction),
    PressKey(PressKeyAction),

    // Mouse
    Hover(TargetAction),

    // Cookies
    SetCookie(SetCookieAction),
    DeleteCookie(DeleteCookieAction),

    // JavaScript
    Execute(ExecuteAction),

    // Scrolling
    Scroll(ScrollAction),
    ScrollTo(TargetAction),

    // Debug
    Screenshot(ScreenshotAction),
    Log(LogAction),
    AssertText(AssertTextAction),
    AssertUrl(AssertUrlAction),

    // Control flow
    IfTextExists(IfTextExistsAction),
    IfSelectorExists(IfSelectorExistsAction),
    Repeat(RepeatAction),

    // Composition
    Include(IncludeAction),
}

impl Action {
    /// Short name for logging.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Goto(_) => "goto",
            Self::Back => "back",
            Self::Forward => "forward",
            Self::Reload => "reload",
            Self::Wait(_) => "wait",
            Self::WaitForNetworkIdle(_) => "wait_for_network_idle",
            Self::WaitFor(_) => "wait_for",
            Self::WaitForVisible(_) => "wait_for_visible",
            Self::WaitForHidden(_) => "wait_for_hidden",
            Self::WaitForText(_) => "wait_for_text",
            Self::WaitForUrl(_) => "wait_for_url",
            Self::WaitForEmail(_) => "wait_for_email",
            Self::Click(_) => "click",
            Self::TryClick(_) => "try_click",
            Self::TryClickAny(_) => "try_click_any",
            Self::Fill(_) => "fill",
            Self::Type(_) => "type",
            Self::Clear(_) => "clear",
            Self::Select(_) => "select",
            Self::PressKey(_) => "press_key",
            Self::Hover(_) => "hover",
            Self::SetCookie(_) => "set_cookie",
            Self::DeleteCookie(_) => "delete_cookie",
            Self::Execute(_) => "execute",
            Self::Scroll(_) => "scroll",
            Self::ScrollTo(_) => "scroll_to",
            Self::Screenshot(_) => "screenshot",
            Self::Log(_) => "log",
            Self::AssertText(_) => "assert_text",
            Self::AssertUrl(_) => "assert_url",
            Self::IfTextExists(_) => "if_text_exists",
            Self::IfSelectorExists(_) => "if_selector_exists",
            Self::Repeat(_) => "repeat",
            Self::Include(_) => "include",
        }
    }
}

const ACTION_NAMES: &[&str] = &[
    "goto",
    "back",
    "forward",
    "reload",
    "wait",
    "wait_for_network_idle",
    "wait_for",
    "wait_for_visible",
    "wait_for_hidden",
    "wait_for_text",
    "wait_for_url",
    "wait_for_email",
    "click",
    "try_click",
    "try_click_any",
    "fill",
    "type",
    "clear",
    "select",
    "press_key",
    "hover",
    "set_cookie",
    "delete_cookie",
    "execute",
    "scroll",
    "scroll_to",
    "screenshot",
    "log",
    "assert_text",
    "assert_url",
    "if_text_exists",
    "if_selector_exists",
    "repeat",
    "include",
];

impl<'de> Deserialize<'de> for Action {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(ActionVisitor)
    }
}

struct ActionVisitor;

impl<'de> Visitor<'de> for ActionVisitor {
    type Value = Action;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("an action (string for unit variants, or map with single key)")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        match value {
            "back" => Ok(Action::Back),
            "forward" => Ok(Action::Forward),
            "reload" => Ok(Action::Reload),
            other => Err(de::Error::unknown_variant(
                other,
                &["back", "forward", "reload"],
            )),
        }
    }

    fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
    where
        M: MapAccess<'de>,
    {
        let key: String = map
            .next_key()?
            .ok_or_else(|| de::Error::custom("expected action type key"))?;

        let action = match key.as_str() {
            "goto" => Action::Goto(map.next_value()?),
            "back" => {
                let _: serde_yaml::Value = map.next_value()?;
                Action::Back
            }
            "forward" => {
                let _: serde_yaml::Value = map.next_value()?;
                Action::Forward
            }
            "reload" => {
                let _: serde_yaml::Value = map.next_value()?;
                Action::Reload
            }
            "wait" => Action::Wait(map.next_value()?),
            "wait_for_network_idle" => Action::WaitForNetworkIdle(map.next_value()?),
            "wait_for" => Action::WaitFor(map.next_value()?),
            "wait_for_visible" => Action::WaitForVisible(map.next_value()?),
            "wait_for_hidden" => Action::WaitForHidden(map.next_value()?),
            "wait_for_text" => Action::WaitForText(map.next_value()?),
            "wait_for_url" => Action::WaitForUrl(map.next_value()?),
            "wait_for_email" => Action::WaitForEmail(map.next_value()?),
            "click" => Action::Click(map.next_value()?),
            "try_click" => Action::TryClick(map.next_value()?),
            "try_click_any" => Action::TryClickAny(map.next_value()?),
            "fill" => Action::Fill(map.next_value()?),
            "type" => Action::Type(map.next_value()?),
            "clear" => Action::Clear(map.next_value()?),
            "select" => Action::Select(map.next_value()?),
            "press_key" => Action::PressKey(map.next_value()?),
            "hover" => Action::Hover(map.next_value()?),
            "set_cookie" => Action::SetCookie(map.next_value()?),
            "delete_cookie" => Action::DeleteCookie(map.next_value()?),
            "execute" => Action::Execute(map.next_value()?),
            "scroll" => Action::Scroll(map.next_value()?),
            "scroll_to" => Action::ScrollTo(map.next_value()?),
            "screenshot" => Action::Screenshot(map.next_value()?),
            "log" => Action::Log(map.next_value()?),
            "assert_text" => Action::AssertText(map.next_value()?),
            "assert_url" => Action::AssertUrl(map.next_value()?),
            "if_text_exists" => Action::IfTextExists(map.next_value()?),
            "if_selector_exists" => Action::IfSelectorExists(map.next_value()?),
            "repeat" => Action::Repeat(map.next_value()?),
            "include" => Action::Include(map.next_value()?),
            other => return Err(de::Error::unknown_variant(other, ACTION_NAMES)),
        };

        Ok(action)
    }
}

// --- Action payloads ---

#[derive(Debug, Clone, Deserialize)]
pub struct GotoAction {
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WaitAction {
    pub ms: u64,
}

fn default_idle_ms() -> u64 {
    500
}
fn default_timeout_ms() -> u64 {
    10000
}

#[derive(Debug, Clone, Deserialize)]
pub struct WaitForNetworkIdleAction {
    #[serde(default = "default_idle_ms")]
    pub idle_ms: u64,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WaitForAction {
    pub selector: String,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WaitForTextAction {
    pub text: String,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WaitForUrlAction {
    pub contains: String,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImapConfigAction {
    pub host: String,
    #[serde(default = "ImapConfigAction::default_port")]
    pub port: u16,
    #[serde(default = "ImapConfigAction::default_tls")]
    pub tls: bool,
    pub username: String,
    pub password: String,
    #[serde(default = "ImapConfigAction::default_mailbox")]
    pub mailbox: String,
}

impl ImapConfigAction {
    fn default_port() -> u16 { 993 }
    fn default_tls() -> bool { true }
    fn default_mailbox() -> String { "INBOX".into() }
}

#[derive(Debug, Clone, Deserialize)]
pub struct EmailFilterAction {
    pub from: Option<String>,
    pub subject_contains: Option<String>,
    #[serde(default = "EmailFilterAction::default_unseen_only")]
    pub unseen_only: bool,
    pub since_minutes: Option<i64>,
    #[serde(default)]
    pub mark_seen: bool,
}

impl EmailFilterAction {
    fn default_unseen_only() -> bool { true }
}

impl Default for EmailFilterAction {
    fn default() -> Self {
        Self {
            from: None,
            subject_contains: None,
            unseen_only: true,
            since_minutes: None,
            mark_seen: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct WaitForEmailAction {
    pub imap: ImapConfigAction,
    #[serde(default)]
    pub filter: EmailFilterAction,
    #[serde(default = "WaitForEmailAction::default_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "WaitForEmailAction::default_poll_interval_ms")]
    pub poll_interval_ms: u64,
    #[serde(default)]
    pub extract: EmailExtractAction,
    #[serde(default)]
    pub action: Option<EmailAction>,
}

impl WaitForEmailAction {
    fn default_timeout_ms() -> u64 { 120_000 }
    fn default_poll_interval_ms() -> u64 { 2_000 }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct EmailExtractAction {
    pub link: Option<EmailLinkExtract>,
    pub code: Option<EmailCodeExtract>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EmailLinkExtract {
    pub allow_domains: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EmailCodeExtract {
    pub regex: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmailAction {
    OpenLink(EmailOpenLinkAction),
    Fill(EmailFillAction),
}

/// Empty config — accepts both `open_link: {}` and bare `open_link:` in YAML.
#[derive(Debug, Clone, Default)]
pub struct EmailOpenLinkAction;

impl<'de> Deserialize<'de> for EmailOpenLinkAction {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Accept null/unit (bare `open_link:`) or empty map (`open_link: {}`)
        let v = serde_yaml::Value::deserialize(deserializer)?;
        match v {
            serde_yaml::Value::Null | serde_yaml::Value::Mapping(_) => Ok(Self),
            _ => Err(serde::de::Error::custom("expected null or empty map for open_link")),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct EmailFillAction {
    pub selector: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClickAction {
    #[serde(flatten)]
    pub target: Target,
    #[serde(default)]
    pub human: bool,
    #[serde(default)]
    pub scroll_into_view: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TryClickAnyAction {
    pub selectors: Option<Vec<String>>,
    pub texts: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FillAction {
    #[serde(flatten)]
    pub target: Target,
    pub value: String,
    #[serde(default)]
    pub human: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TypeAction {
    #[serde(flatten)]
    pub target: Target,
    pub value: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClearAction {
    #[serde(flatten)]
    pub target: Target,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SelectAction {
    #[serde(flatten)]
    pub target: Target,
    pub value: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PressKeyAction {
    pub key: String,
}

/// Generic action that just needs a target element.
#[derive(Debug, Clone, Deserialize)]
pub struct TargetAction {
    #[serde(flatten)]
    pub target: Target,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SetCookieAction {
    pub name: String,
    pub value: String,
    pub domain: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeleteCookieAction {
    pub name: String,
    pub domain: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExecuteAction {
    pub js: String,
}

fn default_scroll_amount() -> u32 {
    1
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScrollAction {
    pub direction: ScrollDirection,
    #[serde(default = "default_scroll_amount")]
    pub amount: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScrollDirection {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScreenshotAction {
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LogAction {
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssertTextAction {
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssertUrlAction {
    pub contains: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IfTextExistsAction {
    pub text: String,
    #[serde(rename = "then")]
    pub then_actions: Vec<Action>,
    #[serde(rename = "else", default)]
    pub else_actions: Vec<Action>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IfSelectorExistsAction {
    pub selector: String,
    #[serde(rename = "then")]
    pub then_actions: Vec<Action>,
    #[serde(rename = "else", default)]
    pub else_actions: Vec<Action>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RepeatAction {
    pub times: u32,
    pub actions: Vec<Action>,
}

/// Include another config's actions.
#[derive(Debug, Clone, Deserialize)]
pub struct IncludeAction {
    /// Path to the config file to include.
    pub path: String,

    /// Parameters to pass to the included config.
    #[serde(default)]
    pub params: std::collections::HashMap<String, String>,
}
