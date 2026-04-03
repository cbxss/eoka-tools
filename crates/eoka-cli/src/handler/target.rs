//! Target resolution and action retry logic.

use std::collections::HashMap;

use eoka::Page;
use eoka_agent::{observe, target, Target};
use serde_json::json;

use super::state::TabState;

pub struct ResolvedTarget {
    pub selector: String,
    pub desc: String,
    pub bbox: target::BBox,
}

/// Shared selector-generation JS (used by ref resolution).
const SELECTOR_JS: &str = include_str!("../../../eoka-agent/src/js/selector.js");

/// Resolve a snapshot ref (@eN) to a CSS selector via CDP.
async fn resolve_ref(
    page: &Page,
    snapshot_refs: &HashMap<String, i64>,
    label: &str,
) -> Result<String, String> {
    let stale = |detail: &str| format!("Ref {} {}. Take a new snapshot.", label, detail);

    let backend_id = snapshot_refs
        .get(label)
        .ok_or_else(|| stale("not found"))?;

    let resolve_result: serde_json::Value = page
        .session()
        .send("DOM.resolveNode", &json!({ "backendNodeId": backend_id }))
        .await
        .map_err(|_| stale("no longer exists"))?;

    let object_id = resolve_result
        .get("object")
        .and_then(|o| o.get("objectId"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| stale("could not be resolved"))?;

    let call_result: serde_json::Value = page
        .session()
        .send(
            "Runtime.callFunctionOn",
            &json!({
                "objectId": object_id,
                "functionDeclaration": SELECTOR_JS,
                "arguments": [{ "objectId": object_id }],
                "returnByValue": true
            }),
        )
        .await
        .map_err(|_| stale("selector generation failed"))?;

    call_result
        .get("result")
        .and_then(|r| r.get("value"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| stale("selector returned null"))
}

/// Resolve target string to selector + bbox.
pub async fn resolve_target(tab: &TabState, target_str: &str) -> Result<ResolvedTarget, String> {
    match Target::parse(target_str) {
        Target::Index(idx) => {
            let el = tab
                .elements
                .get(idx)
                .ok_or_else(|| format!("Index {} out of range (have {})", idx, tab.elements.len()))?;
            Ok(ResolvedTarget {
                selector: el.selector.clone(),
                desc: el.to_string(),
                bbox: target::BBox {
                    x: el.bbox.x,
                    y: el.bbox.y,
                    width: el.bbox.width,
                    height: el.bbox.height,
                },
            })
        }
        Target::Ref(label) => {
            let selector = resolve_ref(&tab.page, &tab.snapshot_refs, &label).await?;
            Ok(ResolvedTarget {
                desc: format!("ref {}", label),
                selector,
                bbox: target::BBox::default(),
            })
        }
        Target::Live(pattern) => {
            let r = target::resolve(&tab.page, &pattern)
                .await
                .map_err(|e| e.to_string())?;
            if !r.found {
                return Err(r
                    .error
                    .unwrap_or_else(|| format!("{} not found", target_str)));
            }
            Ok(ResolvedTarget {
                selector: r.selector,
                desc: format!("<{}> \"{}\"", r.tag, r.text),
                bbox: r.bbox,
            })
        }
    }
}

fn is_stale_element_error(msg: &str) -> bool {
    const NEEDLES: &[&str] = &[
        "not found",
        "not visible",
        "node not connected",
        "stale element",
        "detached",
    ];
    NEEDLES.iter().any(|n| msg.contains(n))
}

/// Auto-observe if target is an index and element cache is empty.
pub async fn auto_observe_if_needed(
    tab: &mut TabState,
    target_str: &str,
    viewport_only: bool,
) -> Result<(), String> {
    if matches!(Target::parse(target_str), Target::Index(_)) && tab.elements.is_empty() {
        tab.elements = observe::observe(&tab.page, viewport_only)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Click with auto-retry on stale element.
pub async fn click_with_retry(
    tab: &mut TabState,
    target_str: &str,
    viewport_only: bool,
) -> Result<String, String> {
    let resolved = resolve_target(tab, target_str).await?;
    match tab.page.click(&resolved.selector).await {
        Ok(_) => return Ok(resolved.desc),
        Err(e) if is_stale_element_error(&e.to_string()) => {}
        Err(e) => return Err(e.to_string()),
    }
    // Retry once after re-observing
    tab.elements = observe::observe(&tab.page, viewport_only)
        .await
        .map_err(|e| e.to_string())?;
    let resolved = resolve_target(tab, target_str).await?;
    tab.page
        .click(&resolved.selector)
        .await
        .map_err(|e| e.to_string())?;
    Ok(resolved.desc)
}

/// Fill with auto-retry on stale element.
pub async fn fill_with_retry(
    tab: &mut TabState,
    target_str: &str,
    text: &str,
    viewport_only: bool,
) -> Result<String, String> {
    let resolved = resolve_target(tab, target_str).await?;
    match tab.page.fill(&resolved.selector, text).await {
        Ok(_) => return Ok(resolved.desc),
        Err(e) if is_stale_element_error(&e.to_string()) => {}
        Err(e) => return Err(e.to_string()),
    }
    tab.elements = observe::observe(&tab.page, viewport_only)
        .await
        .map_err(|e| e.to_string())?;
    let resolved = resolve_target(tab, target_str).await?;
    tab.page
        .fill(&resolved.selector, text)
        .await
        .map_err(|e| e.to_string())?;
    Ok(resolved.desc)
}

/// Wait for document.readyState to reach "interactive" or "complete".
pub async fn wait_for_stable(page: &Page) -> Result<(), String> {
    let start = std::time::Instant::now();
    let max_wait = std::time::Duration::from_secs(10);
    loop {
        let ready: String = page
            .evaluate_sync("document.readyState || 'loading'")
            .await
            .unwrap_or_else(|_| "loading".to_string());
        if ready == "interactive" || ready == "complete" {
            return Ok(());
        }
        if start.elapsed() > max_wait {
            return Err("Page did not reach interactive state within 10s".into());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

/// Get title without blocking on busy JS thread.
pub async fn title_nonblocking(page: &Page) -> String {
    page.evaluate_sync("document.title || ''")
        .await
        .unwrap_or_default()
}

/// JSON-serialize a string value (for embedding in JS). Never fails on &str.
pub fn json_str(s: &str) -> String {
    serde_json::to_string(s).expect("string serialization is infallible")
}
