//! SPA navigation logic.

use eoka::{Page, Result};

use super::RouterType;

/// JavaScript template for SPA navigation.
/// Takes router_type and path as parameters.
const NAVIGATE_JS: &str = r#"
((routerType, path) => {
  const result = { success: false, error: null, newPath: null };

  try {
    switch (routerType) {
      case 'nextjs':
        // Next.js - use next/router
        if (window.next?.router?.push) {
          window.next.router.push(path);
          result.success = true;
          result.newPath = path;
        } else {
          // Fallback for App Router or when router not available
          history.pushState({}, '', path);
          window.dispatchEvent(new PopStateEvent('popstate', { state: {} }));
          result.success = true;
          result.newPath = path;
        }
        break;

      case 'vue-router':
        // Vue Router
        const vueApp = document.querySelector('[data-v-app]')?.__vue_app__;
        const router = vueApp?.config?.globalProperties?.$router;
        if (router) {
          router.push(path);
          result.success = true;
          result.newPath = path;
        } else {
          // Vue 2 fallback
          const vue2Router = document.querySelector('#app')?.__vue__?.$router;
          if (vue2Router) {
            vue2Router.push(path);
            result.success = true;
            result.newPath = path;
          } else {
            result.error = 'Vue router not found';
          }
        }
        break;

      case 'react-router':
      case 'angular-router':
      case 'history-api':
      default:
        // Use History API + popstate event (works for most SPAs)
        history.pushState({}, '', path);
        window.dispatchEvent(new PopStateEvent('popstate', { state: {} }));
        result.success = true;
        result.newPath = location.pathname;
        break;
    }
  } catch (e) {
    result.error = e.message || String(e);
  }

  return JSON.stringify(result);
})
"#;

/// JavaScript for history navigation.
const HISTORY_GO_JS: &str = r#"
((delta) => {
  const result = { success: false, error: null };
  try {
    history.go(delta);
    result.success = true;
  } catch (e) {
    result.error = e.message || String(e);
  }
  return JSON.stringify(result);
})
"#;

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
        .map_err(|e| eoka::Error::CdpSimple(format!("Failed to parse navigation result: {}", e)))?;

    if result.success {
        // Brief wait for SPA to update
        page.wait(100).await;
        Ok(result.new_path.unwrap_or_else(|| path.to_string()))
    } else {
        Err(eoka::Error::CdpSimple(format!(
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
        .map_err(|e| eoka::Error::CdpSimple(format!("Failed to parse history result: {}", e)))?;

    if result.success {
        // Wait for navigation to complete
        page.wait(200).await;
        Ok(())
    } else {
        Err(eoka::Error::CdpSimple(format!(
            "History navigation failed: {}",
            result.error.unwrap_or_else(|| "unknown error".into())
        )))
    }
}
