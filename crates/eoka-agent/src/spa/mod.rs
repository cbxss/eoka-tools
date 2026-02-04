//! SPA router detection and manipulation.
//!
//! This module provides tools for detecting and navigating Single Page Applications
//! without requiring page reloads. It supports:
//!
//! - React Router (v5 and v6)
//! - Next.js (App Router and Pages Router)
//! - Vue Router
//! - Remix
//! - History API fallback (works with any SPA)

mod detect;
mod navigate;

pub use detect::detect_router;
pub use navigate::{history_go, spa_navigate};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Detected SPA router type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RouterType {
    /// React Router v6 or Remix
    ReactRouter,
    /// Next.js (App or Pages router)
    NextJs,
    /// Vue Router
    VueRouter,
    /// Angular Router
    AngularRouter,
    /// History API (fallback, works with most SPAs)
    HistoryApi,
    /// Could not detect any SPA router
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

/// Information about the detected SPA router.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaRouterInfo {
    /// Detected router type.
    pub router_type: RouterType,
    /// Current path (from location.pathname).
    pub current_path: String,
    /// Query parameters as key-value pairs.
    pub query_params: HashMap<String, String>,
    /// Hash fragment (without #).
    pub hash: String,
    /// Whether programmatic navigation is available.
    pub can_navigate: bool,
    /// Additional router-specific details.
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
