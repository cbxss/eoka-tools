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
