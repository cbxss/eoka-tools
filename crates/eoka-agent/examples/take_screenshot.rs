use eoka::{Browser, StealthConfig};
use std::fs;

#[tokio::main]
async fn main() -> eoka::Result<()> {
    let config = StealthConfig::default();
    let browser = Browser::launch_with_config(config).await?;
    let page = browser
        .new_page("https://serene-frangipane-7fd25b.netlify.app/step2")
        .await?;

    // Wait for page to load
    let _ = page.wait_for_network_idle(500, 5000).await;
    page.wait(1000).await;

    let png = page.screenshot().await?;
    fs::write("/tmp/step2_screenshot.png", &png).unwrap();
    println!("Saved screenshot to /tmp/step2_screenshot.png");

    browser.close().await
}
