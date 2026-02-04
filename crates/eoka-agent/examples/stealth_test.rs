//! Stealth test: verify bot detection bypass.
//!
//! Run with: cargo run --release --example stealth_test

use eoka::{Browser, StealthConfig};
use eoka_agent::AgentPage;

#[tokio::main]
async fn main() -> eoka::Result<()> {
    println!("eoka-tools stealth test\n");

    // Full stealth config (default)
    let browser = Browser::launch_with_config(StealthConfig::default()).await?;
    let page = browser.new_page("https://bot.sannysoft.com").await?;
    let mut agent = AgentPage::new(&page);

    // Wait for tests to run
    agent.wait(3000).await;

    // Screenshot results
    let png = agent.screenshot().await?;
    std::fs::write("stealth_result.png", &png)?;

    // Check key detection vectors
    let results: serde_json::Value = agent
        .extract(
            r#"({
            webdriver: navigator.webdriver,
            plugins: navigator.plugins.length,
            languages: navigator.languages.length,
            chromeRuntime: !!(window.chrome && window.chrome.runtime),
            automationMarkers: Object.keys(window).filter(k =>
                k.includes('$cdc_') || k.includes('webdriver')
            ).length
        })"#,
        )
        .await?;

    println!("Detection vectors:");
    println!("  webdriver:         {}", results["webdriver"]);
    println!("  plugins:           {}", results["plugins"]);
    println!("  languages:         {}", results["languages"]);
    println!("  chrome.runtime:    {}", results["chromeRuntime"]);
    println!("  automation markers:{}", results["automationMarkers"]);

    let pass = results["webdriver"] == false
        && results["plugins"].as_i64().unwrap_or(0) > 0
        && results["automationMarkers"].as_i64().unwrap_or(1) == 0;

    println!("\nResult: {}", if pass { "PASS ✓" } else { "FAIL ✗" });
    println!("Screenshot: stealth_result.png");

    browser.close().await?;
    Ok(())
}
