use super::params::{self, ParamDef, Params};
use super::Action;
use crate::{Error, Result};
use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::collections::HashMap;
use std::fmt;
use std::path::Path;

/// Top-level config structure.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Name of this automation config.
    pub name: String,

    /// Parameter definitions (optional).
    #[serde(default)]
    pub params: HashMap<String, ParamDef>,

    /// Browser configuration.
    #[serde(default)]
    pub browser: BrowserConfig,

    /// Target URL to navigate to.
    pub target: TargetUrl,

    /// List of actions to execute.
    #[serde(default)]
    pub actions: Vec<Action>,

    /// Success conditions (optional).
    pub success: Option<SuccessCondition>,

    /// Failure handling (optional).
    pub on_failure: Option<OnFailure>,
}

impl Config {
    /// Load config from a YAML file.
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())?;
        Self::parse_with_params(&content, &Params::new())
    }

    /// Load config from a YAML file with parameters.
    pub fn load_with_params<P: AsRef<Path>>(path: P, params: &Params) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())?;
        Self::parse_with_params(&content, params)
    }

    /// Parse config from YAML string (no params).
    pub fn parse(yaml: &str) -> Result<Self> {
        Self::parse_with_params(yaml, &Params::new())
    }

    /// Parse config from YAML string with parameter substitution.
    pub fn parse_with_params(yaml: &str, params: &Params) -> Result<Self> {
        // First pass: parse as Value to extract param definitions
        let mut value: serde_yaml::Value = serde_yaml::from_str(yaml)?;

        // Extract param definitions if present
        let defs: HashMap<String, ParamDef> = value
            .get("params")
            .and_then(|v| serde_yaml::from_value(v.clone()).ok())
            .unwrap_or_default();

        // Substitute variables in the entire config
        params::substitute_value(&mut value, params, &defs)?;

        // Now deserialize the substituted config
        let config: Config = serde_yaml::from_value(value)?;
        config.validate()?;
        Ok(config)
    }

    /// Validate the config.
    fn validate(&self) -> Result<()> {
        if self.name.is_empty() {
            return Err(Error::Config("name is required".into()));
        }
        if self.target.url.is_empty() {
            return Err(Error::Config("target.url is required".into()));
        }
        if let Some(ref success) = self.success {
            if success.any.is_some() && success.all.is_some() {
                return Err(Error::Config(
                    "success: specify either 'any' or 'all', not both".into(),
                ));
            }
        }
        if let Some(ref on_failure) = self.on_failure {
            if let Some(ref retry) = on_failure.retry {
                if retry.attempts == 0 {
                    return Err(Error::Config(
                        "on_failure.retry.attempts must be at least 1".into(),
                    ));
                }
            }
        }
        Ok(())
    }
}

/// Browser launch configuration.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct BrowserConfig {
    /// Run in headless mode.
    #[serde(default)]
    pub headless: bool,

    /// Proxy URL (e.g., "http://user:pass@host:port").
    pub proxy: Option<String>,

    /// Custom user agent.
    pub user_agent: Option<String>,

    /// Viewport size.
    pub viewport: Option<Viewport>,
}

/// Viewport dimensions.
#[derive(Debug, Clone, Deserialize)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
}

/// Target URL configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct TargetUrl {
    /// URL to navigate to.
    pub url: String,
}

/// Success condition checking.
#[derive(Debug, Clone, Deserialize)]
pub struct SuccessCondition {
    /// Any of these conditions must be true.
    pub any: Option<Vec<Condition>>,

    /// All of these conditions must be true.
    pub all: Option<Vec<Condition>>,
}

/// Individual condition.
#[derive(Debug, Clone)]
pub enum Condition {
    UrlContains(String),
    TextContains(String),
}

impl<'de> Deserialize<'de> for Condition {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(ConditionVisitor)
    }
}

struct ConditionVisitor;

impl<'de> Visitor<'de> for ConditionVisitor {
    type Value = Condition;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a condition map with single key (url_contains or text_contains)")
    }

    fn visit_map<M>(self, mut map: M) -> std::result::Result<Self::Value, M::Error>
    where
        M: MapAccess<'de>,
    {
        let key: String = map
            .next_key()?
            .ok_or_else(|| de::Error::custom("expected condition type key"))?;

        match key.as_str() {
            "url_contains" => Ok(Condition::UrlContains(map.next_value()?)),
            "text_contains" => Ok(Condition::TextContains(map.next_value()?)),
            other => Err(de::Error::unknown_variant(
                other,
                &["url_contains", "text_contains"],
            )),
        }
    }
}

/// Failure handling configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct OnFailure {
    /// Screenshot path on failure (supports {timestamp}).
    pub screenshot: Option<String>,

    /// Retry configuration.
    pub retry: Option<RetryConfig>,
}

/// Retry configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct RetryConfig {
    /// Number of retry attempts.
    pub attempts: u32,

    /// Delay between retries in milliseconds.
    pub delay_ms: u64,
}
