use eoka::Page;
use rmcp::model::{CallToolResult, Content, ErrorData};
use std::fmt::Write;

use eoka_agent::{observe, target, InteractiveElement, Target};

use super::error::{internal, invalid};
use super::state::TabState;
use super::types::JsRequest;

/// Resolved target ready for action.
pub(crate) struct ResolvedTarget {
    pub selector: String,
    pub desc: String,
    pub bbox: target::BBox,
}

/// Resolve target to selector + bbox. Index uses cache, everything else is live.
pub(crate) async fn resolve_target(
    page: &Page,
    elements: &[InteractiveElement],
    target_str: &str,
) -> Result<ResolvedTarget, ErrorData> {
    match Target::parse(target_str) {
        Target::Index(idx) => {
            let el = elements.get(idx).ok_or_else(|| {
                invalid(format!(
                    "Index {} out of range (have {})",
                    idx,
                    elements.len()
                ))
            })?;
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
        Target::Live(pattern) => {
            let r = target::resolve(page, &pattern).await.map_err(internal)?;
            if !r.found {
                return Err(invalid(
                    r.error
                        .unwrap_or_else(|| format!("{} not found", target_str)),
                ));
            }
            Ok(ResolvedTarget {
                selector: r.selector,
                desc: format!("<{}> \"{}\"", r.tag, r.text),
                bbox: r.bbox,
            })
        }
    }
}

/// Get page title without blocking on busy JS thread.
pub(crate) async fn title_nonblocking(page: &Page) -> String {
    page.evaluate_sync("document.title || ''")
        .await
        .unwrap_or_default()
}

/// Wait for document.readyState to reach at least "interactive".
pub(crate) async fn wait_for_stable(page: &Page) -> eoka::Result<()> {
    let start = std::time::Instant::now();
    let max_wait = std::time::Duration::from_secs(10);
    loop {
        let ready: String = page
            .evaluate_sync("document.readyState || 'loading'")
            .await
            .unwrap_or_else(|_| "loading".to_string());
        if ready == "interactive" || ready == "complete" {
            break;
        }
        if start.elapsed() > max_wait {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Ok(())
}

/// Auto-observe if target is an index and element cache is empty.
pub(crate) async fn auto_observe_if_needed(
    tab: &mut TabState,
    target_str: &str,
    viewport_only: bool,
) -> Result<(), ErrorData> {
    if matches!(Target::parse(target_str), Target::Index(_)) && tab.elements.is_empty() {
        tab.elements = observe::observe(&tab.page, viewport_only)
            .await
            .map_err(internal)?;
    }
    Ok(())
}

/// Click with auto-retry: if first click fails with "not found"/"not visible",
/// re-observe and retry once.
pub(crate) async fn click_with_retry(
    tab: &mut TabState,
    target_str: &str,
    viewport_only: bool,
) -> Result<String, ErrorData> {
    let resolved = resolve_target(&tab.page, &tab.elements, target_str).await?;
    match tab.page.click(&resolved.selector).await {
        Ok(_) => Ok(resolved.desc),
        Err(e) if e.to_string().contains("not found") || e.to_string().contains("not visible") => {
            tab.elements = observe::observe(&tab.page, viewport_only)
                .await
                .map_err(internal)?;
            let resolved2 = resolve_target(&tab.page, &tab.elements, target_str).await?;
            tab.page
                .click(&resolved2.selector)
                .await
                .map_err(internal)?;
            Ok(resolved2.desc)
        }
        Err(e) => Err(internal(e)),
    }
}

/// Resolve JS code from a JsRequest: prefer `file` (read from disk), fall back to `js` inline.
pub(crate) fn resolve_js(req: &JsRequest) -> Result<String, ErrorData> {
    if let Some(path) = &req.file {
        std::fs::read_to_string(path)
            .map_err(|e| invalid(format!("Failed to read JS file '{}': {}", path, e)))
    } else if let Some(js) = &req.js {
        Ok(js.clone())
    } else {
        Err(invalid("Either 'js' or 'file' must be provided"))
    }
}

pub(crate) fn text_ok(s: impl Into<String>) -> Result<CallToolResult, ErrorData> {
    Ok(CallToolResult::success(vec![Content::text(s.into())]))
}

/// Generate element list string.
pub(crate) fn element_list(elements: &[InteractiveElement]) -> String {
    let mut out = String::with_capacity(elements.len() * 40);
    for el in elements {
        let _ = writeln!(out, "{}", el);
    }
    out
}
