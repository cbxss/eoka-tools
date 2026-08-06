mod detect;
mod navigate;

pub use detect::detect_router;
pub use navigate::{history_go, spa_navigate};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RouterType {
    ReactRouter,
    NextJs,
    VueRouter,
    AngularRouter,
    HistoryApi,
    Unknown,
}

impl std::fmt::Display for RouterType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RouterType::ReactRouter => write!(f, "React Router"),
            RouterType::NextJs => write!(f, "Next.js"),
            RouterType::VueRouter => write!(f, "Vue Router"),
            RouterType::AngularRouter => write!(f, "Angular Router"),
            RouterType::HistoryApi => write!(f, "History API"),
            RouterType::Unknown => write!(f, "Unknown"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaRouterInfo {
    pub router_type: RouterType,
    pub current_path: String,
    pub query_params: HashMap<String, String>,
    pub hash: String,
    pub can_navigate: bool,
    pub details: Option<String>,
}

impl std::fmt::Display for SpaRouterInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Router: {}", self.router_type)?;
        writeln!(f, "Path: {}", self.current_path)?;
        if !self.query_params.is_empty() {
            writeln!(f, "Query: {:?}", self.query_params)?;
        }
        if !self.hash.is_empty() {
            writeln!(f, "Hash: #{}", self.hash)?;
        }
        writeln!(
            f,
            "Can navigate: {}",
            if self.can_navigate { "yes" } else { "no" }
        )?;
        if let Some(ref details) = self.details {
            writeln!(f, "Details: {}", details)?;
        }
        Ok(())
    }
}
