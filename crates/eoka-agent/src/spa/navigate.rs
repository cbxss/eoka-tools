//! SPA navigation logic.

use eoka::{Page, Result};

use super::RouterType;

/// JavaScript template for SPA navigation.
/// Takes router_type and path as parameters.
const NAVIGATE_JS: &str = include_str!("../js/spa_navigate.js");

/// JavaScript for history navigation.
const HISTORY_GO_JS: &str = include_str!("../js/spa_history.js");

/// Result from navigation JavaScript.
#[derive(Debug, serde::Deserialize)]
struct NavResult {
    success: bool,
    error: Option<String>,
    #[serde(default)]
    new_path: Option<String>,
}

/// Navigate an SPA to a new path without page reload.
///
/// This uses the detected router type to call the appropriate navigation method.
/// Falls back to History API + popstate event for unknown routers.
pub async fn spa_navigate(page: &Page, router_type: &RouterType, path: &str) -> Result<String> {
    let router_str = match router_type {
        RouterType::ReactRouter => "react-router",
        RouterType::NextJs => "nextjs",
        RouterType::VueRouter => "vue-router",
        RouterType::AngularRouter => "angular-router",
        RouterType::HistoryApi => "history-api",
        RouterType::Unknown => "history-api", // Fallback
    };

    let js = format!(
        "{}({}, {})",
        NAVIGATE_JS,
        serde_json::to_string(router_str).unwrap(),
        serde_json::to_string(path).unwrap()
    );

    let json: String = page.evaluate(&js).await?;
    let result: NavResult = serde_json::from_str(&json)
        .map_err(|e| eoka::Error::cdp_msg(format!("Failed to parse navigation result: {}", e)))?;

    if result.success {
        // Brief wait for SPA to update
        page.wait(100).await;
        Ok(result.new_path.unwrap_or_else(|| path.to_string()))
    } else {
        Err(eoka::Error::cdp_msg(format!(
            "SPA navigation failed: {}",
            result.error.unwrap_or_else(|| "unknown error".into())
        )))
    }
}

/// Navigate browser history by delta steps.
///
/// - delta = -1: go back one step
/// - delta = 1: go forward one step
/// - delta = -2: go back two steps, etc.
pub async fn history_go(page: &Page, delta: i32) -> Result<()> {
    let js = format!("{}({})", HISTORY_GO_JS, delta);

    let json: String = page.evaluate(&js).await?;
    let result: NavResult = serde_json::from_str(&json)
        .map_err(|e| eoka::Error::cdp_msg(format!("Failed to parse history result: {}", e)))?;

    if result.success {
        // Wait for navigation to complete
        page.wait(200).await;
        Ok(())
    } else {
        Err(eoka::Error::cdp_msg(format!(
            "History navigation failed: {}",
            result.error.unwrap_or_else(|| "unknown error".into())
        )))
    }
}
