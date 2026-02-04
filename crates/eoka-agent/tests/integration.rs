//! Integration tests for eoka-agent
//!
//! These tests require Chrome to be installed and available.
//! Run with: cargo test --test integration -- --ignored

use eoka_agent::{AgentPage, Browser, ObserveConfig};

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

    let browser = Browser::launch().await.expect("Failed to launch browser");
    let page = browser
        .new_page("about:blank")
        .await
        .expect("Failed to create page");

    let mut agent = AgentPage::new(&page);
    let elements = agent.observe().await.expect("Failed to observe");

    // Empty page should have no interactive elements
    assert!(elements.is_empty());

    browser.close().await.expect("Failed to close browser");
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_observe_populated_page() {
    if !chrome_available() {
        eprintln!("Chrome not found, skipping test");
        return;
    }

    let browser = Browser::launch().await.expect("Failed to launch browser");
    let page = browser
        .new_page("about:blank")
        .await
        .expect("Failed to create page");

    page.goto(
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

    let mut agent = AgentPage::new(&page);
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

    browser.close().await.expect("Failed to close browser");
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_click() {
    if !chrome_available() {
        eprintln!("Chrome not found, skipping test");
        return;
    }

    let browser = Browser::launch().await.expect("Failed to launch browser");
    let page = browser
        .new_page("about:blank")
        .await
        .expect("Failed to create page");

    page.goto(
        r#"data:text/html,
        <button id="btn" onclick="this.textContent = 'Clicked!'">Click Me</button>
    "#,
    )
    .await
    .expect("Failed to navigate");

    let mut agent = AgentPage::new(&page);
    agent.observe().await.expect("Failed to observe");

    // Click the button by index
    agent.click(0).await.expect("Failed to click");
    agent.wait(100).await;

    // Re-observe and check the text changed
    agent.observe().await.expect("Failed to observe");
    let list = agent.element_list();
    assert!(list.contains("Clicked!"));

    browser.close().await.expect("Failed to close browser");
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_fill() {
    if !chrome_available() {
        eprintln!("Chrome not found, skipping test");
        return;
    }

    let browser = Browser::launch().await.expect("Failed to launch browser");
    let page = browser
        .new_page("about:blank")
        .await
        .expect("Failed to create page");

    page.goto(
        r#"data:text/html,
        <input type="text" id="input" value="">
    "#,
    )
    .await
    .expect("Failed to navigate");

    let mut agent = AgentPage::new(&page);
    agent.observe().await.expect("Failed to observe");

    // Fill the input
    agent.fill(0, "Hello World").await.expect("Failed to fill");

    // Re-observe and check the value
    agent.observe().await.expect("Failed to observe");
    let el = agent.get(0).expect("Element not found");
    assert_eq!(el.value.as_deref(), Some("Hello World"));

    browser.close().await.expect("Failed to close browser");
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_screenshot() {
    if !chrome_available() {
        eprintln!("Chrome not found, skipping test");
        return;
    }

    let browser = Browser::launch().await.expect("Failed to launch browser");
    let page = browser
        .new_page("about:blank")
        .await
        .expect("Failed to create page");

    page.goto(
        r#"data:text/html,
        <button>Button 1</button>
        <button>Button 2</button>
    "#,
    )
    .await
    .expect("Failed to navigate");

    let mut agent = AgentPage::new(&page);
    let png = agent.screenshot().await.expect("Failed to take screenshot");

    // Check PNG magic bytes
    assert!(png.len() > 100);
    assert_eq!(&png[0..4], &[0x89, 0x50, 0x4E, 0x47]); // PNG signature

    browser.close().await.expect("Failed to close browser");
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_observe_diff() {
    if !chrome_available() {
        eprintln!("Chrome not found, skipping test");
        return;
    }

    let browser = Browser::launch().await.expect("Failed to launch browser");
    let page = browser
        .new_page("about:blank")
        .await
        .expect("Failed to create page");

    // Initial page with one button
    page.goto(r#"data:text/html,<button id="btn1">Button 1</button>"#)
        .await
        .expect("Failed to navigate");

    let mut agent = AgentPage::new(&page);
    agent.observe().await.expect("Failed to observe");
    assert_eq!(agent.len(), 1);

    // Add another button via JS
    page.execute(
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

    browser.close().await.expect("Failed to close browser");
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_find_by_text() {
    if !chrome_available() {
        eprintln!("Chrome not found, skipping test");
        return;
    }

    let browser = Browser::launch().await.expect("Failed to launch browser");
    let page = browser
        .new_page("about:blank")
        .await
        .expect("Failed to create page");

    page.goto(
        r#"data:text/html,
        <button>Submit</button>
        <button>Cancel</button>
        <button>Submit Form</button>
    "#,
    )
    .await
    .expect("Failed to navigate");

    let mut agent = AgentPage::new(&page);
    agent.observe().await.expect("Failed to observe");

    // Find first element containing "Submit"
    let idx = agent.find_by_text("submit").expect("Should find element");
    assert_eq!(idx, 0);

    // Find all elements containing "Submit"
    let indices = agent.find_all_by_text("submit");
    assert_eq!(indices.len(), 2);
    assert_eq!(indices, vec![0, 2]);

    browser.close().await.expect("Failed to close browser");
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_viewport_only_config() {
    if !chrome_available() {
        eprintln!("Chrome not found, skipping test");
        return;
    }

    let browser = Browser::launch().await.expect("Failed to launch browser");
    let page = browser
        .new_page("about:blank")
        .await
        .expect("Failed to create page");

    // Create a page with elements below the fold
    page.goto(
        r#"data:text/html,
        <style>body { margin: 0; }</style>
        <button style="position:absolute;top:100px">Visible</button>
        <button style="position:absolute;top:5000px">Below Fold</button>
    "#,
    )
    .await
    .expect("Failed to navigate");

    // With viewport_only = true (default)
    let mut agent = AgentPage::new(&page);
    agent.observe().await.expect("Failed to observe");
    assert_eq!(agent.len(), 1);
    assert!(agent.element_list().contains("Visible"));

    // With viewport_only = false
    let config = ObserveConfig {
        viewport_only: false,
    };
    let mut agent_all = AgentPage::with_config(&page, config);
    agent_all.observe().await.expect("Failed to observe");
    assert_eq!(agent_all.len(), 2);

    browser.close().await.expect("Failed to close browser");
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_select_dropdown() {
    if !chrome_available() {
        eprintln!("Chrome not found, skipping test");
        return;
    }

    let browser = Browser::launch().await.expect("Failed to launch browser");
    let page = browser
        .new_page("about:blank")
        .await
        .expect("Failed to create page");

    page.goto(
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

    let mut agent = AgentPage::new(&page);
    agent.observe().await.expect("Failed to observe");

    // Get options
    let options = agent.options(0).await.expect("Failed to get options");
    assert_eq!(options.len(), 3);
    assert_eq!(options[0], ("r".to_string(), "Red".to_string()));

    // Select by value
    agent.select(0, "g").await.expect("Failed to select");

    // Verify selection via JS
    let selected: String = page
        .evaluate("document.getElementById('color').value")
        .await
        .expect("Failed to evaluate");
    assert_eq!(selected, "g");

    // Select by text
    agent.select(0, "Blue").await.expect("Failed to select");
    let selected: String = page
        .evaluate("document.getElementById('color').value")
        .await
        .expect("Failed to evaluate");
    assert_eq!(selected, "b");

    browser.close().await.expect("Failed to close browser");
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_stale_element_detection() {
    use eoka_agent::Session;

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
    use eoka_agent::Session;

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

    let browser = Browser::launch().await.unwrap();
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

    let browser = Browser::launch().await.unwrap();
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

    let browser = Browser::launch().await.unwrap();
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

    let browser = Browser::launch().await.unwrap();
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

    let browser = Browser::launch().await.unwrap();
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

    let browser = Browser::launch().await.unwrap();
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

    let browser = Browser::launch().await.unwrap();
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
    use eoka_agent::Session;

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
    use eoka_agent::Session;

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
    use eoka_agent::Session;

    if !chrome_available() {
        return;
    }

    let mut agent = Session::launch().await.unwrap();
    agent
        .goto(r#"data:text/html,
            <style>.hover:hover { background: red; }</style>
            <button class="hover" onmouseenter="this.dataset.hovered='yes'">Hover Me</button>
        "#)
        .await
        .unwrap();

    agent.observe().await.unwrap();
    agent.hover(0).await.unwrap();

    // Check hover triggered
    let hovered: String = agent.eval("document.querySelector('button').dataset.hovered || 'no'").await.unwrap();
    assert_eq!(hovered, "yes");

    agent.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_session_scroll_to() {
    if !chrome_available() {
        return;
    }

    let browser = Browser::launch().await.unwrap();
    let page = browser
        .new_page(r#"data:text/html,
            <style>body{height:3000px;margin:0}</style>
            <button style="position:absolute;top:2000px">Far Button</button>
        "#)
        .await
        .unwrap();

    // Use AgentPage with viewport_only=false
    let config = ObserveConfig { viewport_only: false };
    let mut agent = AgentPage::with_config(&page, config);
    agent.observe().await.unwrap();

    // Scroll to button
    agent.scroll_to(0).await.unwrap();
    agent.wait(200).await;

    // Check we scrolled (allow some tolerance)
    let scroll_y: f64 = agent.eval("window.scrollY").await.unwrap();
    assert!(scroll_y > 500.0, "Expected scroll > 500, got {}", scroll_y);

    browser.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_session_back_forward() {
    use eoka_agent::Session;

    if !chrome_available() {
        return;
    }

    let mut agent = Session::launch().await.unwrap();

    agent.goto(r#"data:text/html,<h1>Page A</h1>"#).await.unwrap();
    agent.goto(r#"data:text/html,<h1>Page B</h1>"#).await.unwrap();

    agent.back().await.unwrap();
    assert!(agent.text().await.unwrap().contains("Page A"));

    agent.forward().await.unwrap();
    assert!(agent.text().await.unwrap().contains("Page B"));

    agent.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_session_scroll_directions() {
    use eoka_agent::Session;

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
    use eoka_agent::Session;

    if !chrome_available() {
        return;
    }

    let mut agent = Session::launch().await.unwrap();

    // Test page with keyboard event listener
    agent
        .goto(r#"data:text/html,
            <input id="inp"><div id="log"></div>
            <script>
                document.addEventListener('keydown', e => {
                    document.getElementById('log').textContent += e.key + ',';
                });
            </script>
        "#)
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
    let log: String = agent.eval("document.getElementById('log').textContent").await.unwrap();
    assert!(log.contains("Tab"), "Expected Tab key, got: {}", log);
    assert!(log.contains("Escape"), "Expected Escape key, got: {}", log);
    assert!(log.contains("Enter"), "Expected Enter key, got: {}", log);

    agent.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_session_eval_exec() {
    use eoka_agent::Session;

    if !chrome_available() {
        return;
    }

    let mut agent = Session::launch().await.unwrap();
    agent.goto(r#"data:text/html,<div id="target">Hello</div>"#).await.unwrap();

    // eval returns value
    let text: String = agent.eval("document.getElementById('target').textContent").await.unwrap();
    assert_eq!(text, "Hello");

    // exec modifies DOM
    agent.exec("document.getElementById('target').textContent = 'World'").await.unwrap();
    let text: String = agent.eval("document.getElementById('target').textContent").await.unwrap();
    assert_eq!(text, "World");

    agent.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_agent_extract() {
    if !chrome_available() {
        return;
    }

    let browser = Browser::launch().await.unwrap();
    let page = browser
        .new_page(r#"data:text/html,
            <ul>
                <li data-price="10">Apple</li>
                <li data-price="20">Banana</li>
            </ul>
        "#)
        .await
        .unwrap();

    let agent = AgentPage::new(&page);
    let prices: Vec<String> = agent
        .extract("[...document.querySelectorAll('li')].map(e => e.dataset.price)")
        .await
        .unwrap();
    assert_eq!(prices, vec!["10", "20"]);

    browser.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_session_url_title() {
    use eoka_agent::Session;

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

    let browser = Browser::launch().await.unwrap();
    let page = browser
        .new_page(r#"data:text/html,
            <select id="sel">
                <option value="a">Alpha</option>
                <option value="b">Beta</option>
                <option value="c">Gamma</option>
            </select>
        "#)
        .await
        .unwrap();

    let mut agent = AgentPage::new(&page);
    agent.observe().await.unwrap();
    let opts = agent.options(0).await.unwrap();

    assert_eq!(opts.len(), 3);
    assert_eq!(opts[0], ("a".to_string(), "Alpha".to_string()));
    assert_eq!(opts[1], ("b".to_string(), "Beta".to_string()));
    assert_eq!(opts[2], ("c".to_string(), "Gamma".to_string()));

    browser.close().await.unwrap();
}

// =============================================================================
// Edge cases and error handling
// =============================================================================

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_click_out_of_bounds() {
    use eoka_agent::Session;

    if !chrome_available() {
        return;
    }

    let mut agent = Session::launch().await.unwrap();
    agent.goto(r#"data:text/html,<button>OK</button>"#).await.unwrap();
    agent.observe().await.unwrap();

    let result = agent.click(999).await;
    assert!(result.is_err());

    agent.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_fill_clears_existing() {
    use eoka_agent::Session;

    if !chrome_available() {
        return;
    }

    let mut agent = Session::launch().await.unwrap();
    agent.goto(r#"data:text/html,<input value="old">"#).await.unwrap();

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

    let browser = Browser::launch().await.unwrap();
    let page = browser
        .new_page(r#"data:text/html,
            <button id="b1">Submit</button>
            <button id="b2">Submit</button>
            <button id="b3">Cancel</button>
        "#)
        .await
        .unwrap();

    let mut agent = AgentPage::new(&page);
    agent.observe().await.unwrap();

    let all = agent.find_all_by_text("Submit");
    assert_eq!(all.len(), 2);

    let first = agent.find_by_text("Submit");
    assert_eq!(first, Some(0));

    browser.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn test_hidden_elements_filtered() {
    use eoka_agent::Session;

    if !chrome_available() {
        return;
    }

    let mut agent = Session::launch().await.unwrap();
    agent
        .goto(r#"data:text/html,
            <button>Visible</button>
            <button style="display:none">Hidden</button>
            <button style="visibility:hidden">Invisible</button>
        "#)
        .await
        .unwrap();

    agent.observe().await.unwrap();

    // Should only find the visible button
    assert_eq!(agent.len(), 1);
    assert!(agent.element_list().contains("Visible"));

    agent.close().await.unwrap();
}
