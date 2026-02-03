//! Demo: observe, annotated screenshot, fill, click, extract, keyboard.

use eoka::Browser;
use eoka_tools::AgentPage;

#[tokio::main]
async fn main() -> eoka::Result<()> {
    let browser = Browser::launch().await?;

    // --- Form interaction ---
    let page = browser.new_page("https://httpbin.org/forms/post").await?;
    let mut agent = AgentPage::new(&page);

    agent.observe().await?;
    println!("=== Form elements ===\n{}", agent.element_list());

    // Annotated screenshot
    let png = agent.screenshot().await?;
    std::fs::write("demo_form.png", &png)?;
    println!("Saved demo_form.png");

    // Fill by index
    agent.fill(0, "Agent Smith").await?;
    agent.fill(1, "555-0123").await?;
    agent.fill(2, "agent@example.com").await?;

    // Click radio button
    agent.click(4).await?; // Medium

    // Check a checkbox
    agent.click(6).await?; // Bacon

    // Submit via keyboard
    agent.submit(12).await?;
    agent.wait(1000).await;
    println!("\nAfter submit — URL: {}", agent.url().await?);

    // --- Extract structured data ---
    agent.goto("https://news.ycombinator.com").await?;
    agent.wait(1000).await;

    let titles: Vec<String> = agent.extract(
        "Array.from(document.querySelectorAll('.titleline > a')).slice(0, 5).map(a => a.textContent)"
    ).await?;
    println!("\n=== Top 5 HN titles ===");
    for (i, t) in titles.iter().enumerate() {
        println!("  {}. {}", i + 1, t);
    }

    // Observe after navigation
    agent.observe().await?;
    println!("\n=== HN elements: {} ===", agent.len());

    browser.close().await?;
    Ok(())
}
