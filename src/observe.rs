//! DOM enumeration — finds all interactive elements on the page.

use eoka::{Page, Result};
use serde::Deserialize;

use crate::InteractiveElement;

#[derive(Deserialize)]
struct RawElement {
    tag: String,
    role: Option<String>,
    text: String,
    placeholder: Option<String>,
    input_type: Option<String>,
    selector: String,
    checked: bool,
    value: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

/// JavaScript that enumerates all interactive elements on the page.
const OBSERVE_JS: &str = r#"
(() => {
    const INTERACTIVE = 'a, button, input, select, textarea, [role="button"], [role="link"], [role="tab"], [role="menuitem"], [onclick], [contenteditable="true"]';
    const results = [];
    const seen = new Set();

    // Helper: find associated label for a form element
    function getLabel(el) {
        if (el.id) {
            const label = document.querySelector('label[for=' + JSON.stringify(el.id) + ']');
            if (label) return label.textContent.trim();
        }
        const parentLabel = el.closest('label');
        if (parentLabel) {
            const clone = parentLabel.cloneNode(true);
            clone.querySelectorAll('input, select, textarea').forEach(c => c.remove());
            const t = clone.textContent.trim();
            if (t) return t;
        }
        const labelledBy = el.getAttribute('aria-labelledby');
        if (labelledBy) {
            const lbl = document.getElementById(labelledBy);
            if (lbl) return lbl.textContent.trim();
        }
        const prev = el.previousElementSibling;
        if (prev && prev.tagName === 'LABEL') return prev.textContent.trim();
        return '';
    }

    // Collect elements from a root (document or shadowRoot)
    function collect(root) {
        const all = root.querySelectorAll('*');
        for (const node of all) {
            if (node.matches(INTERACTIVE)) processElement(node);
            if (node.shadowRoot) collect(node.shadowRoot);
        }
    }

    function processElement(el) {
        const rect = el.getBoundingClientRect();
        if (rect.width < 2 || rect.height < 2) return;

        const style = getComputedStyle(el);
        if (style.display === 'none' || style.visibility === 'hidden' || parseFloat(style.opacity) < 0.1) return;

        // Viewport filtering
        if (typeof __eoka_viewport_only !== 'undefined' && __eoka_viewport_only) {
            if (rect.bottom < 0 || rect.top > window.innerHeight) return;
            if (rect.right < 0 || rect.left > window.innerWidth) return;
        }

        const tag = el.tagName.toLowerCase();
        const isFormEl = tag === 'input' || tag === 'select' || tag === 'textarea';
        const inputType = el.getAttribute('type') || '';

        // Get meaningful text
        let text = el.getAttribute('aria-label') || '';
        if (!text) {
            if (tag === 'a' || tag === 'button') {
                text = (el.textContent || '').trim().replace(/\s+/g, ' ');
                if (text.length > 80) text = '';
            } else if (isFormEl) {
                const label = getLabel(el);
                if (label) {
                    text = label;
                } else if (tag === 'select') {
                    // Show selected option text
                    const opt = el.options && el.options[el.selectedIndex];
                    text = opt ? opt.text : '';
                }
            } else {
                text = (el.textContent || '').trim().replace(/\s+/g, ' ');
            }
        }
        if (text.length > 60) text = text.substring(0, 57) + '...';

        const placeholder = el.getAttribute('placeholder') || '';
        const ariaLabel = el.getAttribute('aria-label') || '';
        const title = el.getAttribute('title') || '';
        if (!text && !placeholder && !ariaLabel && !title && !isFormEl) {
            return;
        }

        // Skip redundant nested wrappers
        if ((tag === 'a' || tag === 'button') && el.children.length === 1) {
            const child = el.children[0];
            const childTag = child.tagName.toLowerCase();
            if (childTag === 'button' || childTag === 'input') return;
        }

        // Build unique selector
        let selector;
        if (el.id) {
            selector = '#' + CSS.escape(el.id);
        } else if (isFormEl && el.name) {
            if ((inputType === 'radio' || inputType === 'checkbox') && el.value) {
                selector = tag + '[name=' + JSON.stringify(el.name) + '][value=' + JSON.stringify(el.value) + ']';
            } else {
                selector = tag + '[name=' + JSON.stringify(el.name) + ']';
            }
        } else if (ariaLabel) {
            selector = tag + '[aria-label=' + JSON.stringify(ariaLabel) + ']';
        } else if (tag === 'input' && inputType && placeholder) {
            selector = 'input[type=' + JSON.stringify(inputType) + '][placeholder=' + JSON.stringify(placeholder) + ']';
        } else if (el.getAttribute('data-testid')) {
            selector = '[data-testid=' + JSON.stringify(el.getAttribute('data-testid')) + ']';
        } else {
            const parts = [];
            let node = el;
            while (node && node !== document.body && parts.length < 4) {
                let s = node.tagName.toLowerCase();
                if (node.id) {
                    parts.unshift('#' + CSS.escape(node.id));
                    break;
                }
                const parent = node.parentElement;
                if (parent) {
                    const siblings = Array.from(parent.children).filter(c => c.tagName === node.tagName);
                    if (siblings.length > 1) {
                        s += ':nth-of-type(' + (siblings.indexOf(node) + 1) + ')';
                    }
                }
                parts.unshift(s);
                node = parent;
            }
            selector = parts.join(' > ');
        }

        if (seen.has(selector)) return;
        seen.add(selector);

        // Get current value for form elements
        let value = '';
        if (isFormEl && inputType !== 'password') {
            if (tag === 'select') {
                const opt = el.options && el.options[el.selectedIndex];
                value = opt ? opt.value : '';
            } else {
                value = (el.value || '').trim();
            }
            if (value.length > 40) value = value.substring(0, 37) + '...';
        }

        results.push({
            tag,
            role: el.getAttribute('role') || null,
            text,
            placeholder: placeholder || null,
            input_type: tag === 'input' ? (inputType || 'text') : (tag === 'select' ? 'select' : null),
            selector,
            checked: !!el.checked,
            value,
            x: Math.round(rect.x),
            y: Math.round(rect.y),
            width: Math.round(rect.width),
            height: Math.round(rect.height),
        });
    }

    collect(document);
    return JSON.stringify(results);
})()
"#;

/// Run the observe script and return parsed interactive elements.
pub async fn observe(page: &Page, viewport_only: bool) -> Result<Vec<InteractiveElement>> {
    let js = format!(
        "var __eoka_viewport_only = {}; {}",
        viewport_only, OBSERVE_JS
    );
    let json_str: String = page.evaluate(&js).await?;

    let raw: Vec<RawElement> = serde_json::from_str(&json_str)
        .map_err(|e| eoka::Error::CdpSimple(format!("observe parse error: {}", e)))?;

    Ok(raw
        .into_iter()
        .enumerate()
        .map(|(i, r)| InteractiveElement {
            index: i,
            tag: r.tag,
            role: r.role,
            text: r.text,
            placeholder: r.placeholder,
            input_type: r.input_type,
            selector: r.selector,
            checked: r.checked,
            value: if r.value.is_empty() {
                None
            } else {
                Some(r.value)
            },
            bbox: eoka::BoundingBox {
                x: r.x,
                y: r.y,
                width: r.width,
                height: r.height,
            },
        })
        .collect())
}
