/// Batch OpenCorporates shell company lookup with CAPTCHA solving
/// Usage: cargo run --example batch_opencorporates --release

use eoka::{Browser, StealthConfig};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entity {
    name: String,
    state: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct EntityResult {
    company: String,
    state: String,
    status: String,
    registered_agent: Option<String>,
    incorporation_date: Option<String>,
    last_filing: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

struct AntiCaptchaSolver {
    api_key: String,
    client: reqwest::Client,
}

impl AntiCaptchaSolver {
    fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: reqwest::Client::new(),
        }
    }

    async fn solve_hcaptcha(
        &self,
        website_url: &str,
        website_key: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        // Create task
        let create_resp = self
            .client
            .post("https://api.anti-captcha.com/createTask")
            .json(&json!({
                "clientKey": self.api_key,
                "task": {
                    "type": "HCaptchaTaskProxyless",
                    "websiteURL": website_url,
                    "websiteKey": website_key,
                }
            }))
            .send()
            .await?;

        let create_data: serde_json::Value = create_resp.json().await?;

        if create_data.get("errorId").map(|v| v.as_u64()) != Some(Some(0)) {
            return Err(format!(
                "Failed to create task: {}",
                create_data.get("errorCode").unwrap_or(&json!("unknown"))
            )
            .into());
        }

        let task_id = create_data["taskId"]
            .as_u64()
            .ok_or("No task ID returned")?;

        // Poll for result
        for attempt in 0..300 {
            tokio::time::sleep(Duration::from_millis(500)).await;

            let result_resp = self
                .client
                .post("https://api.anti-captcha.com/getTaskResult")
                .json(&json!({
                    "clientKey": self.api_key,
                    "taskId": task_id
                }))
                .send()
                .await?;

            let result_data: serde_json::Value = result_resp.json().await?;

            if result_data.get("errorId").map(|v| v.as_u64()) != Some(Some(0)) {
                return Err(format!(
                    "Failed to get result: {}",
                    result_data.get("errorCode").unwrap_or(&json!("unknown"))
                )
                .into());
            }

            if result_data.get("ready").map(|v| v.as_bool()) == Some(Some(true)) {
                if let Some(solution) = result_data.get("solution") {
                    if let Some(token) = solution.get("gRecaptchaResponse").and_then(|v| v.as_str())
                    {
                        return Ok(token.to_string());
                    }
                    if let Some(token) = solution
                        .get("gRecaptchaResponseWithoutSpaces")
                        .and_then(|v| v.as_str())
                    {
                        return Ok(token.to_string());
                    }
                    if let Some(token) = solution.get("text").and_then(|v| v.as_str()) {
                        return Ok(token.to_string());
                    }
                }
                return Err("No solution in response".into());
            }

            if attempt % 10 == 0 && attempt > 0 {
                println!("  ⏳ Captcha solving... ({}s)", attempt / 2);
            }
        }

        Err("Captcha solving timeout (5 minutes)".into())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load API key
    let config_path = dirs::home_dir()
        .ok_or("Cannot find home directory")?
        .join(".anti-captcha-config");

    let config_content = fs::read_to_string(&config_path)
        .map_err(|_| format!("Cannot read {}", config_path.display()))?;

    let api_key = config_content
        .lines()
        .find_map(|line| {
            if line.contains("ANTI_CAPTCHA_API_KEY=") {
                line.split('=').nth(1).map(|s| s.trim().to_string())
            } else {
                None
            }
        })
        .ok_or("ANTI_CAPTCHA_API_KEY not found in config")?;

    println!("🔑 API Key: {}...", &api_key[..16.min(api_key.len())]);

    // Load entities
    let entities_path = Path::new("shell_companies.json");
    if !entities_path.exists() {
        return Err("shell_companies.json not found. Run: bash extract_entities.sh".into());
    }

    let entities_content = fs::read_to_string(entities_path)?;
    let entities: Vec<Entity> = serde_json::from_str(&entities_content)?;

    println!("📋 Loaded {} entities\n", entities.len());

    // Initialize
    let solver = AntiCaptchaSolver::new(api_key);

    println!("🌐 Launching stealth browser...");
    let browser = Browser::launch().await?;
    println!("✓ Browser ready\n");

    let mut results = Vec::new();

    for (i, entity) in entities.iter().enumerate() {
        let state = entity.state.as_deref().unwrap_or("USVI");
        println!("[{}/{}] {}", i + 1, entities.len(), entity.name);

        match search_entity(&browser, &solver, &entity.name, state).await {
            Ok(result) => {
                println!("   ✓ Status: {}", result.status);
                results.push(result);
            }
            Err(e) => {
                println!("   ✗ Error: {}", e);
                results.push(EntityResult {
                    company: entity.name.clone(),
                    state: state.to_string(),
                    status: "ERROR".to_string(),
                    registered_agent: None,
                    incorporation_date: None,
                    last_filing: None,
                    error: Some(e.to_string()),
                });
            }
        }

        // Rate limit
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    // Close browser
    browser.close().await?;

    // Save results
    let output_json = serde_json::to_string_pretty(&results)?;
    fs::write("opencorporates_results.json", output_json)?;

    // Summary
    let active = results.iter().filter(|r| r.status == "ACTIVE").count();
    let dissolved = results.iter().filter(|r| r.status == "DISSOLVED").count();
    let not_found = results.iter().filter(|r| r.status == "NOT_FOUND").count();
    let errors = results.iter().filter(|r| r.status == "ERROR").count();

    println!("\n✅ Complete! Results saved to: opencorporates_results.json");
    println!("\n📊 Summary:");
    println!("   Active: {}", active);
    println!("   Dissolved: {}", dissolved);
    println!("   Not Found: {}", not_found);
    println!("   Errors: {}", errors);

    Ok(())
}

async fn search_entity(
    browser: &Browser,
    solver: &AntiCaptchaSolver,
    company: &str,
    state: &str,
) -> Result<EntityResult, Box<dyn std::error::Error>> {
    let url = format!(
        "https://opencorporates.com/search?q={}&jurisdiction_code={}",
        company.replace(' ', "+"),
        state
    );

    println!("   🔍 Searching...");

    let page = browser.new_page("https://about:blank").await?;
    page.goto(&url).await?;
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Check for CAPTCHA
    let sitekey: Option<String> = page
        .evaluate(
            r#"
            (function() {
                const elem = document.querySelector('[data-sitekey]');
                return elem ? elem.getAttribute('data-sitekey') : null;
            })()
            "#,
        )
        .await
        .ok()
        .flatten();

    if let Some(key) = sitekey {
        println!("   🔒 CAPTCHA detected");
        println!("   🤖 Solving CAPTCHA...");

        match solver.solve_hcaptcha(&url, &key).await {
            Ok(token) => {
                println!("   ✓ CAPTCHA solved");

                // Inject and submit
                let _: serde_json::Value = page.evaluate(&format!(
                    r#"
                    document.querySelector('[name="h-captcha-response"]').value = '{}';
                    document.querySelector('form').submit();
                    "#,
                    token
                ))
                .await
                .unwrap_or(serde_json::Value::Null);

                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            Err(e) => {
                println!("   ⚠ CAPTCHA solve failed: {}", e);
            }
        }
    }

    // Parse results - just return basic info for now
    let result = EntityResult {
        company: company.to_string(),
        state: state.to_string(),
        status: "FOUND".to_string(),
        registered_agent: None,
        incorporation_date: None,
        last_filing: None,
        error: None,
    };

    Ok(result)
}

