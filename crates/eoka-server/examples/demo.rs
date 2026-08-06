use eoka_server::Session;

#[tokio::main(flavor = "current_thread")]
async fn main() -> eoka_server::eoka::Result<()> {
    let mut agent = Session::launch().await?;

    agent.goto("https://httpbin.org/forms/post").await?;

    agent.observe().await?;
    println!("=== Form elements ===\n{}", agent.element_list());

    let png = agent.screenshot().await?;
    std::fs::write("demo_form.png", &png)?;
    println!("Saved demo_form.png");

    agent.observe().await?;
    agent.fill(0, "Agent Smith").await?;
    agent.observe().await?;
    agent.fill(1, "555-0123").await?;
    agent.observe().await?;
    agent.fill(2, "agent@example.com").await?;

    agent.observe().await?;
    agent.click(4).await?;

    agent.observe().await?;
    agent.click(6).await?;

    agent.observe().await?;
    agent.submit(12).await?;
    agent.wait(1000).await;
    println!("\nAfter submit — URL: {}", agent.url().await?);

    agent.goto("https://news.ycombinator.com").await?;
    agent.wait(1000).await;

    let titles: Vec<String> = agent.extract(
        "Array.from(document.querySelectorAll('.titleline > a')).slice(0, 5).map(a => a.textContent)"
    ).await?;
    println!("\n=== Top 5 HN titles ===");
    for (i, t) in titles.iter().enumerate() {
        println!("  {}. {}", i + 1, t);
    }

    agent.observe().await?;
    println!("\n=== HN elements: {} ===", agent.len());

    agent.close().await?;
    Ok(())
}
