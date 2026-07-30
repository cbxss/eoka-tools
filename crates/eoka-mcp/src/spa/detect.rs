use eoka_server::eoka::{Page, Result};
use serde::Deserialize;
use std::collections::HashMap;

use super::{RouterType, SpaRouterInfo};

#[derive(Debug, Deserialize)]
struct JsDetectionResult {
    router_type: String,
    path: String,
    query: HashMap<String, String>,
    hash: String,
    can_navigate: bool,
    details: Option<String>,
}

const DETECT_JS: &str = include_str!("../js/spa_detect.js");

pub async fn detect_router(page: &Page) -> Result<SpaRouterInfo> {
    let json: String = page.evaluate(DETECT_JS).await?;
    let raw: JsDetectionResult = serde_json::from_str(&json).map_err(|e| {
        eoka_server::eoka::Error::cdp_msg(format!("Failed to parse router detection: {}", e))
    })?;

    let router_type = match raw.router_type.as_str() {
        "react-router" => RouterType::ReactRouter,
        "nextjs" => RouterType::NextJs,
        "vue-router" => RouterType::VueRouter,
        "angular-router" => RouterType::AngularRouter,
        "history-api" => RouterType::HistoryApi,
        _ => RouterType::Unknown,
    };

    Ok(SpaRouterInfo {
        router_type,
        current_path: raw.path,
        query_params: raw.query,
        hash: raw.hash,
        can_navigate: raw.can_navigate,
        details: raw.details,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_router_type_display() {
        assert_eq!(RouterType::ReactRouter.to_string(), "React Router");
        assert_eq!(RouterType::NextJs.to_string(), "Next.js");
        assert_eq!(RouterType::VueRouter.to_string(), "Vue Router");
        assert_eq!(RouterType::AngularRouter.to_string(), "Angular Router");
        assert_eq!(RouterType::HistoryApi.to_string(), "History API");
        assert_eq!(RouterType::Unknown.to_string(), "Unknown");
    }
}
