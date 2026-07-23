//! Integration tests for eoka-agent
//!
//! These tests require Chrome to be installed and available.
//! Run with: cargo test --test integration -- --ignored

use std::collections::HashMap;

use eoka_agent::{ObserveConfig, Session};

/// Check if Chrome is available
fn chrome_available() -> bool {
    eoka::stealth::patcher::find_chrome().is_ok()
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_observe_empty_page() {
    if !chrome_available() {
        eprintln!("Chrome not found, skipping test");
        return;
    }

    let mut agent = Session::launch().await.expect("Failed to launch");
    agent.goto("about:blank").await.expect("Failed to navigate");

    let elements = agent.observe().await.expect("Failed to observe");

    // Empty page should have no interactive elements
    assert!(elements.is_empty());

    agent.close().await.expect("Failed to close");
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_observe_populated_page() {
    if !chrome_available() {
        eprintln!("Chrome not found, skipping test");
        return;
    }

    let mut agent = Session::launch().await.expect("Failed to launch");
    agent
        .goto(
            r##"data:text/html,
        <style>body { margin: 0; padding: 20px; }</style>
        <button id="btn1">Click Me</button>
        <input type="text" placeholder="Enter name">
        <a href="https://example.com">Link</a>
        <select><option>Option 1</option></select>
    "##,
        )
        .await
        .expect("Failed to navigate");

    let elements = agent.observe().await.expect("Failed to observe");

    // Should find button, input, link, select (at least 4 elements)
    assert!(
        elements.len() >= 4,
        "Expected at least 4 elements, got {}",
        elements.len()
    );

    // Check element list formatting
    let list = agent.element_list();
    assert!(list.contains("<button>"), "list: {}", list);
    assert!(list.contains("<input>"), "list: {}", list);
    assert!(list.contains("<a>"), "list: {}", list);
    // select has type="select" so it's displayed as <select type="select">
    assert!(list.contains("<select "), "list: {}", list);
    assert!(list.contains("Click Me"), "list: {}", list);
    assert!(
        list.contains("placeholder=\"Enter name\""),
        "list: {}",
        list
    );

    agent.close().await.expect("Failed to close");
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_click() {
    if !chrome_available() {
        eprintln!("Chrome not found, skipping test");
        return;
    }

    let mut agent = Session::launch().await.expect("Failed to launch");
    agent
        .goto(
            r#"data:text/html,
        <button id="btn" onclick="this.textContent = 'Clicked!'">Click Me</button>
    "#,
        )
        .await
        .expect("Failed to navigate");

    agent.observe().await.expect("Failed to observe");

    // Click the button by index
    agent.click(0).await.expect("Failed to click");
    agent.wait(100).await;

    // Re-observe and check the text changed
    agent.observe().await.expect("Failed to observe");
    let list = agent.element_list();
    assert!(list.contains("Clicked!"));

    agent.close().await.expect("Failed to close");
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_fill() {
    if !chrome_available() {
        eprintln!("Chrome not found, skipping test");
        return;
    }

    let mut agent = Session::launch().await.expect("Failed to launch");
    agent
        .goto(
            r#"data:text/html,
        <input type="text" id="input" value="">
    "#,
        )
        .await
        .expect("Failed to navigate");

    agent.observe().await.expect("Failed to observe");

    // Fill the input
    agent.fill(0, "Hello World").await.expect("Failed to fill");

    // Re-observe and check the value
    agent.observe().await.expect("Failed to observe");
    let el = agent.get(0).expect("Element not found");
    assert_eq!(el.value.as_deref(), Some("Hello World"));

    agent.close().await.expect("Failed to close");
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_screenshot() {
    if !chrome_available() {
        eprintln!("Chrome not found, skipping test");
        return;
    }

    let mut agent = Session::launch().await.expect("Failed to launch");
    agent
        .goto(
            r#"data:text/html,
        <button>Button 1</button>
        <button>Button 2</button>
    "#,
        )
        .await
        .expect("Failed to navigate");

    let png = agent.screenshot().await.expect("Failed to take screenshot");

    // Check PNG magic bytes
    assert!(png.len() > 100);
    assert_eq!(&png[0..4], &[0x89, 0x50, 0x4E, 0x47]); // PNG signature

    agent.close().await.expect("Failed to close");
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_observe_diff() {
    if !chrome_available() {
        eprintln!("Chrome not found, skipping test");
        return;
    }

    let mut agent = Session::launch().await.expect("Failed to launch");

    // Initial page with one button
    agent
        .goto(r#"data:text/html,<button id="btn1">Button 1</button>"#)
        .await
        .expect("Failed to navigate");

    agent.observe().await.expect("Failed to observe");
    assert_eq!(agent.len(), 1);

    // Add another button via JS
    agent
        .exec(
            r#"
        const btn = document.createElement('button');
        btn.id = 'btn2';
        btn.textContent = 'Button 2';
        document.body.appendChild(btn);
    "#,
        )
        .await
        .expect("Failed to execute JS");

    // Observe diff
    let diff = agent.observe_diff().await.expect("Failed to observe_diff");

    assert_eq!(diff.added.len(), 1);
    assert_eq!(diff.removed, 0);
    assert_eq!(diff.total, 2);

    // Check Display impl
    let display = diff.to_string();
    assert!(display.contains("+1 added"));
    assert!(display.contains("(2 total)"));

    agent.close().await.expect("Failed to close");
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_find_by_text() {
    if !chrome_available() {
        eprintln!("Chrome not found, skipping test");
        return;
    }

    let mut agent = Session::launch().await.expect("Failed to launch");
    agent
        .goto(
            r#"data:text/html,
        <button>Submit</button>
        <button>Cancel</button>
        <button>Submit Form</button>
    "#,
        )
        .await
        .expect("Failed to navigate");

    agent.observe().await.expect("Failed to observe");

    // Find first element containing "Submit"
    let idx = agent.find_by_text("submit").expect("Should find element");
    assert_eq!(idx, 0);

    // Find all elements containing "Submit"
    let indices = agent.find_all_by_text("submit");
    assert_eq!(indices.len(), 2);
    assert_eq!(indices, vec![0, 2]);

    agent.close().await.expect("Failed to close");
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_viewport_only_config() {
    if !chrome_available() {
        eprintln!("Chrome not found, skipping test");
        return;
    }

    // With viewport_only = true (default)
    let mut agent = Session::launch().await.expect("Failed to launch");
    agent
        .goto(
            r#"data:text/html,
        <style>body { margin: 0; }</style>
        <button style="position:absolute;top:100px">Visible</button>
        <button style="position:absolute;top:5000px">Below Fold</button>
    "#,
        )
        .await
        .expect("Failed to navigate");

    agent.observe().await.expect("Failed to observe");
    assert_eq!(agent.len(), 1);
    assert!(agent.element_list().contains("Visible"));

    // With viewport_only = false
    agent.set_observe_config(ObserveConfig {
        viewport_only: false,
    });
    agent.observe().await.expect("Failed to observe");
    assert_eq!(agent.len(), 2);

    agent.close().await.expect("Failed to close");
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_select_dropdown() {
    if !chrome_available() {
        eprintln!("Chrome not found, skipping test");
        return;
    }

    let mut agent = Session::launch().await.expect("Failed to launch");
    agent
        .goto(
            r#"data:text/html,
        <select id="color">
            <option value="r">Red</option>
            <option value="g">Green</option>
            <option value="b">Blue</option>
        </select>
    "#,
        )
        .await
        .expect("Failed to navigate");

    agent.observe().await.expect("Failed to observe");

    // Get options
    let options = agent.options(0).await.expect("Failed to get options");
    assert_eq!(options.len(), 3);
    assert_eq!(options[0], ("r".to_string(), "Red".to_string()));

    // Select by value
    agent.select(0, "g").await.expect("Failed to select");

    // Verify selection via JS
    let selected: String = agent
        .eval("document.getElementById('color').value")
        .await
        .expect("Failed to evaluate");
    assert_eq!(selected, "g");

    // Select by text
    agent.observe().await.expect("re-observe after select");
    agent.select(0, "Blue").await.expect("Failed to select");
    let selected: String = agent
        .eval("document.getElementById('color').value")
        .await
        .expect("Failed to evaluate");
    assert_eq!(selected, "b");

    agent.close().await.expect("Failed to close");
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_stale_element_detection() {
    if !chrome_available() {
        eprintln!("Chrome not found, skipping test");
        return;
    }

    let mut agent = Session::launch().await.expect("Failed to launch");

    // Page with a button that removes itself when clicked
    agent
        .goto(
            r#"data:text/html,
            <button id="btn" onclick="this.remove()">Click to Remove</button>
            <button id="other">Other Button</button>
        "#,
        )
        .await
        .expect("Failed to navigate");

    agent.observe().await.expect("Failed to observe");
    assert_eq!(agent.len(), 2);

    // Remove the button via JS (simulating DOM mutation)
    agent
        .exec("document.getElementById('btn').remove()")
        .await
        .expect("Failed to exec");

    // Try to click the removed element - should error
    let result = agent.click(0).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("no longer exists") || err.contains("moved"),
        "Expected stale element error, got: {}",
        err
    );

    agent.close().await.expect("Failed to close");
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_session_basic() {
    if !chrome_available() {
        eprintln!("Chrome not found, skipping test");
        return;
    }

    let mut agent = Session::launch().await.expect("Failed to launch");

    agent
        .goto(
            r#"data:text/html,
            <button onclick="document.body.innerHTML += '<p>Clicked!</p>'">Click Me</button>
        "#,
        )
        .await
        .expect("Failed to navigate");

    agent.observe().await.expect("Failed to observe");
    assert_eq!(agent.len(), 1);

    // Click should work and auto-wait
    agent.click(0).await.expect("Failed to click");

    // Verify the click worked
    let text = agent.text().await.expect("Failed to get text");
    assert!(text.contains("Clicked!"), "Page text: {}", text);

    agent.close().await.expect("Failed to close");
}

// =============================================================================
// Live targeting tests
// =============================================================================

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_live_resolve_text() {
    use eoka_agent::{target, LivePattern};

    if !chrome_available() {
        return;
    }

    let browser = eoka_agent::Browser::launch().await.unwrap();
    let page = browser
        .new_page(r#"data:text/html,<button>Submit Form</button><button>Cancel</button>"#)
        .await
        .unwrap();

    let r = target::resolve(&page, &LivePattern::Text("Submit".into()))
        .await
        .unwrap();
    assert!(r.found);
    assert_eq!(r.tag, "button");
    assert!(r.text.contains("Submit"));

    // Case insensitive
    let r2 = target::resolve(&page, &LivePattern::Text("cancel".into()))
        .await
        .unwrap();
    assert!(r2.found);
    assert!(r2.text.contains("Cancel"));

    browser.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_live_resolve_placeholder() {
    use eoka_agent::{target, LivePattern};

    if !chrome_available() {
        return;
    }

    let browser = eoka_agent::Browser::launch().await.unwrap();
    let page = browser
        .new_page(r#"data:text/html,<input placeholder="Enter your email"><input placeholder="Password">"#)
        .await
        .unwrap();

    let r = target::resolve(&page, &LivePattern::Placeholder("email".into()))
        .await
        .unwrap();
    assert!(r.found);
    assert_eq!(r.tag, "input");

    let r2 = target::resolve(&page, &LivePattern::Placeholder("Password".into()))
        .await
        .unwrap();
    assert!(r2.found);

    browser.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_live_resolve_css() {
    use eoka_agent::{target, LivePattern};

    if !chrome_available() {
        return;
    }

    let browser = eoka_agent::Browser::launch().await.unwrap();
    let page = browser
        .new_page(r#"data:text/html,<button class="primary">OK</button><button class="secondary">Cancel</button>"#)
        .await
        .unwrap();

    let r = target::resolve(&page, &LivePattern::Css("button.primary".into()))
        .await
        .unwrap();
    assert!(r.found);
    assert!(r.text.contains("OK"));

    let r2 = target::resolve(&page, &LivePattern::Css(".secondary".into()))
        .await
        .unwrap();
    assert!(r2.found);
    assert!(r2.text.contains("Cancel"));

    browser.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_live_resolve_id() {
    use eoka_agent::{target, LivePattern};

    if !chrome_available() {
        return;
    }

    let browser = eoka_agent::Browser::launch().await.unwrap();
    let page = browser
        .new_page(r#"data:text/html,<button id="submit-btn">Submit</button>"#)
        .await
        .unwrap();

    let r = target::resolve(&page, &LivePattern::Id("submit-btn".into()))
        .await
        .unwrap();
    assert!(r.found);
    assert_eq!(r.selector, "#submit-btn");
    assert!(r.text.contains("Submit"));

    browser.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_live_resolve_role() {
    use eoka_agent::{target, LivePattern};

    if !chrome_available() {
        return;
    }

    let browser = eoka_agent::Browser::launch().await.unwrap();
    let page = browser
        .new_page(r#"data:text/html,<div role="button" tabindex="0">Custom Button</div>"#)
        .await
        .unwrap();

    let r = target::resolve(&page, &LivePattern::Role("button".into()))
        .await
        .unwrap();
    assert!(r.found);
    assert!(r.text.contains("Custom Button"));

    browser.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_live_resolve_not_found() {
    use eoka_agent::{target, LivePattern};

    if !chrome_available() {
        return;
    }

    let browser = eoka_agent::Browser::launch().await.unwrap();
    let page = browser
        .new_page(r#"data:text/html,<button>OK</button>"#)
        .await
        .unwrap();

    let r = target::resolve(&page, &LivePattern::Text("NonExistent".into()))
        .await
        .unwrap();
    assert!(!r.found);
    assert!(r.error.is_some());

    browser.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_live_resolve_bbox() {
    use eoka_agent::{target, LivePattern};

    if !chrome_available() {
        return;
    }

    let browser = eoka_agent::Browser::launch().await.unwrap();
    let page = browser
        .new_page(r#"data:text/html,<style>body{margin:0}</style><button style="width:100px;height:50px">Click</button>"#)
        .await
        .unwrap();

    let r = target::resolve(&page, &LivePattern::Text("Click".into()))
        .await
        .unwrap();
    assert!(r.found);
    assert!(r.bbox.width >= 100.0);
    assert!(r.bbox.height >= 50.0);

    browser.close().await.unwrap();
}

// =============================================================================
// SPA tests
// =============================================================================

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_spa_info_history_api() {
    if !chrome_available() {
        return;
    }

    let mut agent = Session::launch().await.unwrap();
    agent
        .goto(r#"data:text/html,<button>Hello</button>"#)
        .await
        .unwrap();

    let info = agent.spa_info().await.unwrap();
    // data: URLs should fall back to history API
    assert!(info.can_navigate);

    agent.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_history_go() {
    if !chrome_available() {
        return;
    }

    let mut agent = Session::launch().await.unwrap();

    // Navigate to two pages
    agent
        .goto(r#"data:text/html,<h1>Page 1</h1>"#)
        .await
        .unwrap();
    agent
        .goto(r#"data:text/html,<h1>Page 2</h1>"#)
        .await
        .unwrap();

    // Go back
    agent.history_go(-1).await.unwrap();

    let text = agent.text().await.unwrap();
    assert!(text.contains("Page 1"), "Expected Page 1, got: {}", text);

    // Go forward
    agent.history_go(1).await.unwrap();

    let text = agent.text().await.unwrap();
    assert!(text.contains("Page 2"), "Expected Page 2, got: {}", text);

    agent.close().await.unwrap();
}

// =============================================================================
// Session method tests
// =============================================================================

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_session_hover() {
    if !chrome_available() {
        return;
    }

    let mut agent = Session::launch().await.unwrap();
    agent
        .goto(
            r#"data:text/html,
            <style>.hover:hover { background: red; }</style>
            <button class="hover" onmouseenter="this.dataset.hovered='yes'">Hover Me</button>
        "#,
        )
        .await
        .unwrap();

    agent.observe().await.unwrap();
    agent.hover(0).await.unwrap();

    // Check hover triggered
    let hovered: String = agent
        .eval("document.querySelector('button').dataset.hovered || 'no'")
        .await
        .unwrap();
    assert_eq!(hovered, "yes");

    agent.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_session_scroll_to() {
    if !chrome_available() {
        return;
    }

    let mut agent = Session::launch().await.unwrap();
    agent.set_observe_config(ObserveConfig {
        viewport_only: false,
    });
    agent
        .goto(
            r#"data:text/html,
            <style>body{height:3000px;margin:0}</style>
            <button style="position:absolute;top:2000px">Far Button</button>
        "#,
        )
        .await
        .unwrap();

    agent.observe().await.unwrap();

    // Scroll to button
    agent.scroll_to(0).await.unwrap();
    agent.wait(200).await;

    // Check we scrolled (allow some tolerance)
    let scroll_y: f64 = agent.eval("window.scrollY").await.unwrap();
    assert!(scroll_y > 500.0, "Expected scroll > 500, got {}", scroll_y);

    agent.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_session_back_forward() {
    if !chrome_available() {
        return;
    }

    let mut agent = Session::launch().await.unwrap();

    agent
        .goto(r#"data:text/html,<h1>Page A</h1>"#)
        .await
        .unwrap();
    agent
        .goto(r#"data:text/html,<h1>Page B</h1>"#)
        .await
        .unwrap();

    agent.back().await.unwrap();
    assert!(agent.text().await.unwrap().contains("Page A"));

    agent.forward().await.unwrap();
    assert!(agent.text().await.unwrap().contains("Page B"));

    agent.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_session_scroll_directions() {
    if !chrome_available() {
        return;
    }

    let mut agent = Session::launch().await.unwrap();
    agent
        .goto(r#"data:text/html,<style>body{height:5000px}</style><p>Top</p>"#)
        .await
        .unwrap();

    // Scroll down
    agent.scroll_down().await.unwrap();
    agent.wait(100).await;
    let y1: f64 = agent.eval("window.scrollY").await.unwrap();
    assert!(y1 > 0.0);

    // Scroll to bottom
    agent.scroll_to_bottom().await.unwrap();
    agent.wait(100).await;
    let y2: f64 = agent.eval("window.scrollY").await.unwrap();
    assert!(y2 > y1);

    // Scroll to top
    agent.scroll_to_top().await.unwrap();
    agent.wait(100).await;
    let y3: f64 = agent.eval("window.scrollY").await.unwrap();
    assert_eq!(y3, 0.0);

    agent.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_session_press_key() {
    if !chrome_available() {
        return;
    }

    let mut agent = Session::launch().await.unwrap();

    // Test page with keyboard event listener
    agent
        .goto(
            r#"data:text/html,
            <input id="inp"><div id="log"></div>
            <script>
                document.addEventListener('keydown', e => {
                    document.getElementById('log').textContent += e.key + ',';
                });
            </script>
        "#,
        )
        .await
        .unwrap();
    agent.wait(100).await;

    // Focus the document body for key events
    agent.exec("document.body.focus()").await.unwrap();
    agent.wait(50).await;

    // Press various keys - they should be captured by the listener
    agent.press_key("Tab").await.unwrap();
    agent.press_key("Escape").await.unwrap();
    agent.press_key("Enter").await.unwrap();
    agent.wait(100).await;

    // Verify keys were captured
    let log: String = agent
        .eval("document.getElementById('log').textContent")
        .await
        .unwrap();
    assert!(log.contains("Tab"), "Expected Tab key, got: {}", log);
    assert!(log.contains("Escape"), "Expected Escape key, got: {}", log);
    assert!(log.contains("Enter"), "Expected Enter key, got: {}", log);

    agent.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_session_eval_exec() {
    if !chrome_available() {
        return;
    }

    let mut agent = Session::launch().await.unwrap();
    agent
        .goto(r#"data:text/html,<div id="target">Hello</div>"#)
        .await
        .unwrap();

    // eval returns value
    let text: String = agent
        .eval("document.getElementById('target').textContent")
        .await
        .unwrap();
    assert_eq!(text, "Hello");

    // exec modifies DOM
    agent
        .exec("document.getElementById('target').textContent = 'World'")
        .await
        .unwrap();
    let text: String = agent
        .eval("document.getElementById('target').textContent")
        .await
        .unwrap();
    assert_eq!(text, "World");

    agent.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_agent_extract() {
    if !chrome_available() {
        return;
    }

    let mut agent = Session::launch().await.unwrap();
    agent
        .goto(
            r#"data:text/html,
            <ul>
                <li data-price="10">Apple</li>
                <li data-price="20">Banana</li>
            </ul>
        "#,
        )
        .await
        .unwrap();

    let prices: Vec<String> = agent
        .extract("[...document.querySelectorAll('li')].map(e => e.dataset.price)")
        .await
        .unwrap();
    assert_eq!(prices, vec!["10", "20"]);

    agent.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_session_url_title() {
    if !chrome_available() {
        return;
    }

    let mut agent = Session::launch().await.unwrap();
    agent
        .goto(r#"data:text/html,<title>Test Title</title><p>Content</p>"#)
        .await
        .unwrap();

    let title = agent.title().await.unwrap();
    assert_eq!(title, "Test Title");

    let url = agent.url().await.unwrap();
    assert!(url.starts_with("data:"));

    agent.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_agent_options() {
    if !chrome_available() {
        return;
    }

    let mut agent = Session::launch().await.unwrap();
    agent
        .goto(
            r#"data:text/html,
            <select id="sel">
                <option value="a">Alpha</option>
                <option value="b">Beta</option>
                <option value="c">Gamma</option>
            </select>
        "#,
        )
        .await
        .unwrap();

    agent.observe().await.unwrap();
    let opts = agent.options(0).await.unwrap();

    assert_eq!(opts.len(), 3);
    assert_eq!(opts[0], ("a".to_string(), "Alpha".to_string()));
    assert_eq!(opts[1], ("b".to_string(), "Beta".to_string()));
    assert_eq!(opts[2], ("c".to_string(), "Gamma".to_string()));

    agent.close().await.unwrap();
}

// =============================================================================
// Edge cases and error handling
// =============================================================================

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_click_out_of_bounds() {
    if !chrome_available() {
        return;
    }

    let mut agent = Session::launch().await.unwrap();
    agent
        .goto(r#"data:text/html,<button>OK</button>"#)
        .await
        .unwrap();
    agent.observe().await.unwrap();

    let result = agent.click(999).await;
    assert!(result.is_err());

    agent.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_fill_clears_existing() {
    if !chrome_available() {
        return;
    }

    let mut agent = Session::launch().await.unwrap();
    agent
        .goto(r#"data:text/html,<input value="old">"#)
        .await
        .unwrap();

    agent.observe().await.unwrap();
    agent.fill(0, "new").await.unwrap();

    agent.observe().await.unwrap();
    let el = agent.get(0).unwrap();
    assert_eq!(el.value.as_deref(), Some("new"));

    agent.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_multiple_elements_same_text() {
    if !chrome_available() {
        return;
    }

    let mut agent = Session::launch().await.unwrap();
    agent
        .goto(
            r#"data:text/html,
            <button id="b1">Submit</button>
            <button id="b2">Submit</button>
            <button id="b3">Cancel</button>
        "#,
        )
        .await
        .unwrap();

    agent.observe().await.unwrap();

    let all = agent.find_all_by_text("Submit");
    assert_eq!(all.len(), 2);

    let first = agent.find_by_text("Submit");
    assert_eq!(first, Some(0));

    agent.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_hidden_elements_filtered() {
    if !chrome_available() {
        return;
    }

    let mut agent = Session::launch().await.unwrap();
    agent
        .goto(
            r#"data:text/html,
            <button>Visible</button>
            <button style="display:none">Hidden</button>
            <button style="visibility:hidden">Invisible</button>
        "#,
        )
        .await
        .unwrap();

    agent.observe().await.unwrap();

    // Should only find the visible button
    assert_eq!(agent.len(), 1);
    assert!(agent.element_list().contains("Visible"));

    agent.close().await.unwrap();
}

// =============================================================================
// State save/load
// =============================================================================

/// Helper: capture state from a page (mirrors the MCP helper logic which is pub(crate)).
async fn capture_state(
    page: &eoka::Page,
) -> (
    Vec<eoka::SessionCookie>,
    HashMap<String, String>,
    HashMap<String, String>,
) {
    let cookies = page.cookies().await.unwrap();
    let local_storage: HashMap<String, String> = page
        .evaluate(
            "(() => { try { return Object.fromEntries(Object.entries(localStorage)) } catch(e) { return {} } })()",
        )
        .await
        .unwrap_or_default();
    let session_storage: HashMap<String, String> = page
        .evaluate(
            "(() => { try { return Object.fromEntries(Object.entries(sessionStorage)) } catch(e) { return {} } })()",
        )
        .await
        .unwrap_or_default();
    (cookies, local_storage, session_storage)
}

/// Helper: restore storage from HashMaps (mirrors the MCP helper logic).
async fn restore_storage(
    page: &eoka::Page,
    local_storage: &HashMap<String, String>,
    session_storage: &HashMap<String, String>,
) {
    if !local_storage.is_empty() {
        let json = serde_json::to_string(local_storage).unwrap();
        let js = format!(
            "(() => {{ localStorage.clear(); const d = {}; for (const [k,v] of Object.entries(d)) localStorage.setItem(k,v); }})()",
            json
        );
        let _: Result<String, _> = page.evaluate(&js).await;
    }
    if !session_storage.is_empty() {
        let json = serde_json::to_string(session_storage).unwrap();
        let js = format!(
            "(() => {{ sessionStorage.clear(); const d = {}; for (const [k,v] of Object.entries(d)) sessionStorage.setItem(k,v); }})()",
            json
        );
        let _: Result<String, _> = page.evaluate(&js).await;
    }
}

/// Helper: cookies round-trip directly as `SessionCookie` in eoka 0.5
/// (`set_cookies_bulk` takes the same type `cookies()` returns).
fn cookie_to_set_cookie(c: &eoka::SessionCookie) -> eoka::SessionCookie {
    c.clone()
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_state_capture_cookies() {
    if !chrome_available() {
        return;
    }

    let mut agent = Session::launch().await.unwrap();
    agent.goto("https://example.com").await.unwrap();

    let page = agent.page();
    page.set_cookie("test_session", "abc123", Some(".example.com"), Some("/"))
        .await
        .unwrap();
    page.set_cookie("test_pref", "dark", Some(".example.com"), Some("/"))
        .await
        .unwrap();

    let (cookies, _, _) = capture_state(page).await;

    let our_cookies: Vec<_> = cookies
        .iter()
        .filter(|c| c.name.starts_with("test_"))
        .collect();
    assert!(
        our_cookies.len() >= 2,
        "Expected at least 2 test cookies, got {}: {:?}",
        our_cookies.len(),
        our_cookies.iter().map(|c| &c.name).collect::<Vec<_>>()
    );

    let session_cookie = cookies.iter().find(|c| c.name == "test_session").unwrap();
    assert_eq!(session_cookie.value, "abc123");
    assert_eq!(session_cookie.domain, ".example.com");

    agent.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_state_capture_local_storage() {
    if !chrome_available() {
        return;
    }

    let mut agent = Session::launch().await.unwrap();
    agent.goto("https://example.com").await.unwrap();

    let page = agent.page();
    page.execute("localStorage.setItem('auth_token', 'jwt.abc.def')")
        .await
        .unwrap();
    page.execute("localStorage.setItem('theme', 'dark')")
        .await
        .unwrap();

    let (_, local_storage, _) = capture_state(page).await;

    assert_eq!(local_storage.get("auth_token").unwrap(), "jwt.abc.def");
    assert_eq!(local_storage.get("theme").unwrap(), "dark");

    agent.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_state_capture_session_storage() {
    if !chrome_available() {
        return;
    }

    let mut agent = Session::launch().await.unwrap();
    agent.goto("https://example.com").await.unwrap();

    let page = agent.page();
    page.execute("sessionStorage.setItem('tab_id', 'tab_42')")
        .await
        .unwrap();

    let (_, _, session_storage) = capture_state(page).await;

    assert_eq!(session_storage.get("tab_id").unwrap(), "tab_42");

    agent.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_state_restore_cookies() {
    if !chrome_available() {
        return;
    }

    let mut agent = Session::launch().await.unwrap();
    agent.goto("https://example.com").await.unwrap();

    let page = agent.page();

    // Set a cookie, then clear + restore different ones
    page.set_cookie("original", "yes", Some(".example.com"), Some("/"))
        .await
        .unwrap();

    page.clear_all_cookies().await.unwrap();
    let restored_cookies = vec![
        eoka::SessionCookie {
            name: "restored_1".into(),
            value: "val1".into(),
            domain: ".example.com".into(),
            path: "/".into(),
            secure: false,
            http_only: true,
            same_site: Some("Lax".into()),
            expires: None,
        },
        eoka::SessionCookie {
            name: "restored_2".into(),
            value: "val2".into(),
            domain: ".example.com".into(),
            path: "/".into(),
            secure: false,
            http_only: false,
            same_site: None,
            expires: None,
        },
    ];
    page.set_cookies_bulk(restored_cookies).await.unwrap();

    let cookies = page.cookies().await.unwrap();
    let names: Vec<&str> = cookies.iter().map(|c| c.name.as_str()).collect();

    assert!(
        !names.contains(&"original"),
        "original cookie should be cleared"
    );
    assert!(
        names.contains(&"restored_1"),
        "restored_1 missing from {:?}",
        names
    );
    assert!(
        names.contains(&"restored_2"),
        "restored_2 missing from {:?}",
        names
    );

    // Verify httpOnly flag was preserved
    let r1 = cookies.iter().find(|c| c.name == "restored_1").unwrap();
    assert!(r1.http_only, "httpOnly should be true for restored_1");

    agent.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_state_restore_local_storage() {
    if !chrome_available() {
        return;
    }

    let mut agent = Session::launch().await.unwrap();
    agent.goto("https://example.com").await.unwrap();

    let page = agent.page();
    page.execute("localStorage.setItem('old_key', 'old_val')")
        .await
        .unwrap();

    // Restore: clear and set new values
    let data: HashMap<String, String> = [
        ("token".into(), "new_jwt".into()),
        ("pref".into(), "light".into()),
    ]
    .into();
    restore_storage(page, &data, &HashMap::new()).await;

    let (_, local_storage, _) = capture_state(page).await;

    assert!(
        !local_storage.contains_key("old_key"),
        "old_key should be cleared"
    );
    assert_eq!(local_storage.get("token").unwrap(), "new_jwt");
    assert_eq!(local_storage.get("pref").unwrap(), "light");

    agent.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_state_full_roundtrip() {
    if !chrome_available() {
        return;
    }

    let mut agent = Session::launch().await.unwrap();
    agent.goto("https://example.com").await.unwrap();

    let page = agent.page();

    // Set up state: cookies + localStorage + sessionStorage
    page.set_cookie("sid", "session123", Some(".example.com"), Some("/"))
        .await
        .unwrap();
    page.execute("localStorage.setItem('app_token', 'tok_xyz')")
        .await
        .unwrap();
    page.execute("sessionStorage.setItem('view', 'dashboard')")
        .await
        .unwrap();

    // Capture
    let (cookies, ls, ss) = capture_state(page).await;
    assert!(cookies
        .iter()
        .any(|c| c.name == "sid" && c.value == "session123"));
    assert_eq!(ls.get("app_token").unwrap(), "tok_xyz");
    assert_eq!(ss.get("view").unwrap(), "dashboard");

    // Wipe everything
    page.clear_all_cookies().await.unwrap();
    page.execute("localStorage.clear(); sessionStorage.clear()")
        .await
        .unwrap();

    // Verify wiped
    let (c2, ls2, ss2) = capture_state(page).await;
    assert!(
        !c2.iter().any(|c| c.name == "sid"),
        "cookies should be wiped"
    );
    assert!(
        !ls2.contains_key("app_token"),
        "localStorage should be wiped"
    );
    assert!(!ss2.contains_key("view"), "sessionStorage should be wiped");

    // Restore cookies
    let set_cookies: Vec<_> = cookies.iter().map(cookie_to_set_cookie).collect();
    page.set_cookies_bulk(set_cookies).await.unwrap();

    // Restore storage
    restore_storage(page, &ls, &ss).await;

    // Verify restored
    let (c3, ls3, ss3) = capture_state(page).await;
    assert!(
        c3.iter()
            .any(|c| c.name == "sid" && c.value == "session123"),
        "sid cookie should be restored, got: {:?}",
        c3.iter().map(|c| &c.name).collect::<Vec<_>>()
    );
    assert_eq!(ls3.get("app_token").unwrap(), "tok_xyz");
    assert_eq!(ss3.get("view").unwrap(), "dashboard");

    agent.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_state_empty_storage_capture() {
    if !chrome_available() {
        return;
    }

    let mut agent = Session::launch().await.unwrap();
    agent.goto("https://example.com").await.unwrap();

    let page = agent.page();
    // Clear everything first
    page.clear_all_cookies().await.unwrap();
    page.execute("localStorage.clear(); sessionStorage.clear()")
        .await
        .unwrap();

    let (cookies, ls, ss) = capture_state(page).await;

    // Cookies might have some from example.com, but storage should be empty
    assert!(ls.is_empty(), "localStorage should be empty");
    assert!(ss.is_empty(), "sessionStorage should be empty");
    // Cookies: we cleared them, but the site might set some on load.
    // Just verify capture doesn't error on empty state.
    let _ = cookies;

    agent.close().await.unwrap();
}
