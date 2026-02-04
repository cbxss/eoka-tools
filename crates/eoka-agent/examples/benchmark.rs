//! Benchmark: measure eoka-tools operation timings.
//!
//! Run with: cargo run --release --example benchmark
//!
//! This measures real-world AI agent operations:
//! - Browser launch
//! - Navigation
//! - Element observation (DOM scanning)
//! - Annotated screenshot
//! - Form interactions

use eoka::{Browser, StealthConfig};
use eoka_agent::AgentPage;
use std::time::Instant;

fn ms(start: Instant) -> u128 {
    start.elapsed().as_millis()
}

#[tokio::main]
async fn main() -> eoka::Result<()> {
    println!("eoka-tools benchmark\n");
    println!("All times in milliseconds (ms)\n");
    println!("{:-<50}", "");

    // --- Launch ---
    let t = Instant::now();
    let config = StealthConfig {
        headless: true,
        ..Default::default()
    };
    let browser = Browser::launch_with_config(config).await?;
    println!("Browser launch:         {:>6} ms", ms(t));

    // --- Navigation ---
    let t = Instant::now();
    let page = browser.new_page("https://httpbin.org/forms/post").await?;
    println!("Navigate (httpbin):     {:>6} ms", ms(t));

    let mut agent = AgentPage::new(&page);

    // --- Observe ---
    let t = Instant::now();
    agent.observe().await?;
    let elem_count = agent.len();
    println!("Observe ({} elems):      {:>6} ms", elem_count, ms(t));

    // --- Screenshot (annotated) ---
    let t = Instant::now();
    let _png = agent.screenshot().await?;
    println!("Screenshot (annotated): {:>6} ms", ms(t));

    // --- Fill operations ---
    let t = Instant::now();
    agent.fill(0, "Test User").await?;
    agent.fill(1, "555-1234").await?;
    agent.fill(2, "test@example.com").await?;
    println!("Fill 3 fields:          {:>6} ms", ms(t));

    // --- Click ---
    let t = Instant::now();
    agent.click(4).await?;
    println!("Click (radio):          {:>6} ms", ms(t));

    // --- Navigate to content-heavy page ---
    let t = Instant::now();
    agent.goto("https://news.ycombinator.com").await?;
    println!("Navigate (HN):          {:>6} ms", ms(t));

    // Wait for content
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // --- Observe many elements ---
    let t = Instant::now();
    agent.observe().await?;
    let hn_elems = agent.len();
    println!("Observe ({} elems):    {:>6} ms", hn_elems, ms(t));

    // --- Extract data ---
    let t = Instant::now();
    let _titles: Vec<String> = agent
        .extract(
            "Array.from(document.querySelectorAll('.titleline > a')).slice(0,30).map(a=>a.textContent)",
        )
        .await?;
    println!("Extract (30 titles):    {:>6} ms", ms(t));

    // --- Scroll ---
    let t = Instant::now();
    agent.scroll_down().await?;
    println!("Scroll down:            {:>6} ms", ms(t));

    // --- Full page text ---
    let t = Instant::now();
    let text = agent.text().await?;
    println!("Page text ({} chars): {:>6} ms", text.len(), ms(t));

    println!("{:-<50}", "");

    // --- Cleanup ---
    let t = Instant::now();
    browser.close().await?;
    println!("Browser close:          {:>6} ms", ms(t));

    println!("\nDone.");
    Ok(())
}
