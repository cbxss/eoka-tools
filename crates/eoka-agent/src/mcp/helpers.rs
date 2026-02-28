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
/// Returns Err if the page does not stabilize within 10 seconds.
pub(crate) async fn wait_for_stable(page: &Page) -> eoka::Result<()> {
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
            return Err(eoka::Error::CdpSimple(
                "Page did not reach interactive state within 10s".into(),
            ));
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
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

/// Check if an error message indicates a stale/detached element that warrants retry.
pub(crate) fn is_stale_element_error(msg: &str) -> bool {
    const NEEDLES: &[&str] = &[
        "not found",
        "not visible",
        "node not connected",
        "stale element",
        "detached",
    ];
    NEEDLES.iter().any(|n| msg.contains(n))
}

/// Click with auto-retry: if first click fails with a stale element error,
/// re-observe and retry once.
pub(crate) async fn click_with_retry(
    tab: &mut TabState,
    target_str: &str,
    viewport_only: bool,
) -> Result<String, ErrorData> {
    let resolved = resolve_target(&tab.page, &tab.elements, target_str).await?;
    match tab.page.click(&resolved.selector).await {
        Ok(_) => Ok(resolved.desc),
        Err(e) if is_stale_element_error(&e.to_string()) => {
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

/// Fill with auto-retry: if first fill fails with a stale element error,
/// re-observe and retry once.
pub(crate) async fn fill_with_retry(
    tab: &mut TabState,
    target_str: &str,
    text: &str,
    viewport_only: bool,
) -> Result<String, ErrorData> {
    let resolved = resolve_target(&tab.page, &tab.elements, target_str).await?;
    match tab.page.fill(&resolved.selector, text).await {
        Ok(_) => Ok(resolved.desc),
        Err(e) if is_stale_element_error(&e.to_string()) => {
            tab.elements = observe::observe(&tab.page, viewport_only)
                .await
                .map_err(internal)?;
            let resolved2 = resolve_target(&tab.page, &tab.elements, target_str).await?;
            tab.page
                .fill(&resolved2.selector, text)
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

/// Valid filter values for the observe tool.
pub(crate) const VALID_OBSERVE_FILTERS: &[&str] = &["inputs", "buttons", "all"];

#[cfg(test)]
mod tests {
    use super::*;
    use eoka_agent::InteractiveElement;

    fn make_test_element(index: usize, tag: &str, text: &str) -> InteractiveElement {
        InteractiveElement {
            index,
            tag: tag.to_string(),
            role: None,
            text: text.to_string(),
            placeholder: None,
            input_type: None,
            selector: format!("[data-idx=\"{}\"]", index),
            checked: false,
            value: None,
            bbox: eoka::BoundingBox {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 30.0,
            },
            fingerprint: 0,
        }
    }

    #[test]
    fn resolve_js_inline_ok() {
        let req = JsRequest {
            js: Some("console.log('hi')".into()),
            file: None,
        };
        assert_eq!(resolve_js(&req).unwrap(), "console.log('hi')");
    }

    #[test]
    fn resolve_js_neither_field_errors() {
        let req = JsRequest {
            js: None,
            file: None,
        };
        assert!(resolve_js(&req).is_err());
    }

    #[test]
    fn resolve_js_nonexistent_file_errors() {
        let req = JsRequest {
            js: None,
            file: Some("/nonexistent/path/to/script.js".into()),
        };
        assert!(resolve_js(&req).is_err());
    }

    #[test]
    fn element_list_empty() {
        assert_eq!(element_list(&[]), "");
    }

    #[test]
    fn element_list_with_elements() {
        let els = vec![
            make_test_element(0, "button", "Submit"),
            make_test_element(1, "a", "Link"),
        ];
        let result = element_list(&els);
        assert!(result.contains("[0]"));
        assert!(result.contains("[1]"));
        assert!(result.contains("Submit"));
        assert!(result.contains("Link"));
        assert!(result.ends_with('\n'));
    }

    #[test]
    fn text_ok_returns_success() {
        let result = text_ok("hello").unwrap();
        assert!(!result.is_error.unwrap_or(false));
        assert_eq!(result.content.len(), 1);
    }

    #[test]
    fn is_stale_element_error_matches() {
        assert!(is_stale_element_error("element not found in DOM"));
        assert!(is_stale_element_error("element not visible"));
        assert!(is_stale_element_error("node not connected"));
        assert!(is_stale_element_error("stale element reference"));
        assert!(is_stale_element_error("element detached from DOM"));
    }

    #[test]
    fn is_stale_element_error_rejects_unrelated() {
        assert!(!is_stale_element_error("invalid selector"));
        assert!(!is_stale_element_error("timeout"));
        assert!(!is_stale_element_error(""));
    }
}
