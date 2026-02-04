//! Generic agentic browser loop.
//!
//! The LLM sees the page, reasons, and calls tools turn-by-turn.
//! No site-specific logic — the system prompt IS the task.
//!
//! Usage:
//!   ANTHROPIC_API_KEY=... cargo run --example generic_agent -- "Go to bestbuy.com and find the cheapest RTX 4090"
//!   ANTHROPIC_API_KEY=... cargo run --example generic_agent -- --context knowledge.txt "Solve all 30 steps at https://..."
//!
//! Optional: --context <file> loads extra context into the system prompt.
//! Optional: --model <model> overrides the default model.
//! Optional: --max-turns <N> overrides max turns (default 200).

use eoka::Browser;
use eoka_agent::AgentPage;
use reqwest::Client;
use serde_json::{json, Value};
use std::time::Instant;

const DEFAULT_MODEL: &str = "claude-sonnet-4-20250514";
const DEFAULT_MAX_TURNS: usize = 200;

const BASE_SYSTEM_PROMPT: &str = r#"You are a browser automation agent. You control a real browser and can see/interact with web pages.

TOOLS AVAILABLE:
- navigate: Go to a URL
- observe: List all interactive elements (buttons, links, inputs) with indices. REQUIRED before click/fill/hover.
- click: Click element by index (from observe)
- fill: Type text into an input by index
- hover: Hover over element by index
- scroll: Scroll up/down/top/bottom or to element index
- type_key: Press a keyboard key (Enter, Tab, Escape, ArrowDown, etc.)
- extract: Run arbitrary JavaScript in the page and return the result
- page_text: Get visible page text (truncated)
- screenshot: Take an annotated screenshot (for visual inspection)
- wait: Wait N milliseconds
- done: Signal task completion

RULES:
- Always call observe before clicking/filling/hovering so indices are fresh.
- Be concise. Prefer tool calls over long explanations.
- If something fails, try a different approach.
- When the task is complete, call done with a summary.
- NEVER stop or ask for confirmation. You are fully autonomous.
- NEVER use end_turn. Always make a tool call.
- If you have a lookup_context tool, use it to recall specific details from the full reference doc (function bodies, exact selectors, etc.) instead of guessing.
"#;

fn tool_definitions() -> Value {
    json!([
        {
            "name": "navigate",
            "description": "Navigate to a URL.",
            "input_schema": {
                "type": "object",
                "properties": { "url": { "type": "string" } },
                "required": ["url"]
            }
        },
        {
            "name": "observe",
            "description": "List interactive elements with indices. Required before click/fill/hover.",
            "input_schema": { "type": "object", "properties": {} }
        },
        {
            "name": "click",
            "description": "Click element by index.",
            "input_schema": {
                "type": "object",
                "properties": { "index": { "type": "integer" } },
                "required": ["index"]
            }
        },
        {
            "name": "fill",
            "description": "Type text into input element by index.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "index": { "type": "integer" },
                    "text": { "type": "string" }
                },
                "required": ["index", "text"]
            }
        },
        {
            "name": "hover",
            "description": "Hover over element by index.",
            "input_schema": {
                "type": "object",
                "properties": { "index": { "type": "integer" } },
                "required": ["index"]
            }
        },
        {
            "name": "scroll",
            "description": "Scroll: 'up', 'down', 'top', 'bottom', or element index as string.",
            "input_schema": {
                "type": "object",
                "properties": { "target": { "type": "string" } },
                "required": ["target"]
            }
        },
        {
            "name": "type_key",
            "description": "Press a key (Enter, Tab, Escape, ArrowDown, etc).",
            "input_schema": {
                "type": "object",
                "properties": { "key": { "type": "string" } },
                "required": ["key"]
            }
        },
        {
            "name": "extract",
            "description": "Run JavaScript in the page, return result as string.",
            "input_schema": {
                "type": "object",
                "properties": { "js": { "type": "string" } },
                "required": ["js"]
            }
        },
        {
            "name": "page_text",
            "description": "Get visible page text (truncated to 2000 chars).",
            "input_schema": { "type": "object", "properties": {} }
        },
        {
            "name": "screenshot",
            "description": "Take annotated screenshot showing numbered elements.",
            "input_schema": { "type": "object", "properties": {} }
        },
        {
            "name": "wait",
            "description": "Wait N milliseconds.",
            "input_schema": {
                "type": "object",
                "properties": { "ms": { "type": "integer" } },
                "required": ["ms"]
            }
        },
        {
            "name": "lookup_context",
            "description": "Search the full reference document for specific details (function bodies, selectors, etc). Use when the cheatsheet isn't enough. Returns matching sections.",
            "input_schema": {
                "type": "object",
                "properties": { "query": { "type": "string", "description": "Keyword or function name to search for" } },
                "required": ["query"]
            }
        },
        {
            "name": "done",
            "description": "Signal task completion with a summary.",
            "input_schema": {
                "type": "object",
                "properties": { "summary": { "type": "string" } },
                "required": ["summary"]
            }
        }
    ])
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let api_key = std::env::var("OPENROUTER_API_KEY")
        .or_else(|_| std::env::var("ANTHROPIC_API_KEY"))
        .expect("Set OPENROUTER_API_KEY or ANTHROPIC_API_KEY env var");
    let api_base = std::env::var("API_BASE_URL").unwrap_or_else(|_| {
        if std::env::var("OPENROUTER_API_KEY").is_ok() {
            "https://openrouter.ai/api/v1".to_string()
        } else {
            "https://api.anthropic.com/v1".to_string()
        }
    });
    let use_openrouter = api_base.contains("openrouter");

    // Parse args: [--context file] [--model model] [--max-turns N] <task...>
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut context_file: Option<String> = None;
    let mut context_full_file: Option<String> = None;
    let mut model = DEFAULT_MODEL.to_string();
    let mut max_turns = DEFAULT_MAX_TURNS;
    let mut task_parts: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--context" => {
                i += 1;
                context_file = Some(args.get(i).expect("--context requires a file path").clone());
            }
            "--context-full" => {
                i += 1;
                context_full_file = Some(
                    args.get(i)
                        .expect("--context-full requires a file path")
                        .clone(),
                );
            }
            "--model" => {
                i += 1;
                model = args.get(i).expect("--model requires a model name").clone();
            }
            "--max-turns" => {
                i += 1;
                max_turns = args
                    .get(i)
                    .expect("--max-turns requires a number")
                    .parse()?;
            }
            _ => task_parts.push(args[i].clone()),
        }
        i += 1;
    }

    let task = task_parts.join(" ");
    if task.is_empty() {
        eprintln!("Usage: generic_agent [--context CHEATSHEET] [--context-full FULL_REF] [--model MODEL] [--max-turns N] <task>");
        eprintln!("Example: generic_agent \"Go to bestbuy.com and find the cheapest RTX 4090\"");
        std::process::exit(1);
    }

    // Build system prompt — cheatsheet goes in every turn, full context only turn 0
    let mut system = BASE_SYSTEM_PROMPT.to_string();
    let cheatsheet_ctx = if let Some(path) = &context_file {
        let ctx = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("Failed to read context file {}: {}", path, e));
        system.push_str("\n=== SITE CHEATSHEET ===\n");
        system.push_str(&ctx);
        system.push('\n');
        Some(ctx)
    } else {
        None
    };
    let full_context = if let Some(path) = &context_full_file {
        let ctx = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("Failed to read context-full file {}: {}", path, e));
        Some(ctx)
    } else {
        None
    };

    // If only --context is provided (no --context-full), use it as both cheatsheet and full
    // For backwards compat: single --context still works as before
    let full_context = full_context.or_else(|| cheatsheet_ctx.clone());

    println!("Task: {}", task);
    println!("Model: {}", model);
    if context_file.is_some() {
        println!("Context (cheatsheet): {}", context_file.as_ref().unwrap());
    }
    if context_full_file.is_some() {
        println!("Context (full): {}", context_full_file.as_ref().unwrap());
    }
    println!("Max turns: {}", max_turns);
    println!("---");

    let start = Instant::now();
    let http = Client::new();

    let browser = Browser::launch().await?;
    let page = browser.new_page("about:blank").await?;
    let mut agent = AgentPage::new(&page);

    // Turn 0 gets full context injected into the user message
    let turn0_content = if let Some(ref full) = full_context {
        if context_full_file.is_some() {
            // Two-tier mode: cheatsheet in system prompt, full doc in first message
            format!(
                "{}\n\n=== FULL REFERENCE (use lookup_context tool to search this later) ===\n{}",
                task, full
            )
        } else {
            task.clone()
        }
    } else {
        task.clone()
    };

    let mut messages: Vec<Value> = vec![json!({ "role": "user", "content": turn0_content })];

    let mut total_input_tokens: u64 = 0;
    let mut total_output_tokens: u64 = 0;

    for turn in 0..max_turns {
        println!("\n--- Turn {} ---", turn);

        let body = json!({
            "model": model,
            "max_tokens": 4096,
            "system": system,
            "tools": tool_definitions(),
            "messages": messages,
        });

        let resp_json =
            call_api_with_retry(&http, &api_key, &api_base, use_openrouter, &body).await?;

        if let Some(err) = resp_json.get("error") {
            eprintln!("API error: {}", err);
            break;
        }

        // Track tokens
        if let Some(usage) = resp_json.get("usage") {
            total_input_tokens += usage["input_tokens"].as_u64().unwrap_or(0);
            total_output_tokens += usage["output_tokens"].as_u64().unwrap_or(0);
        }

        let content = resp_json["content"].as_array().unwrap_or(&vec![]).clone();

        for block in &content {
            if block["type"] == "text" {
                let t = block["text"].as_str().unwrap_or("");
                if !t.is_empty() {
                    println!("Claude: {}", t);
                }
            }
        }

        messages.push(json!({ "role": "assistant", "content": content }));

        let stop = resp_json["stop_reason"].as_str().unwrap_or("");
        if stop == "end_turn" {
            println!("  (end_turn — injecting continuation)");
            messages.push(json!({
                "role": "user",
                "content": "Keep going. Do not stop until the task is complete. Call a tool."
            }));
            continue;
        }

        let tool_uses: Vec<&Value> = content.iter().filter(|b| b["type"] == "tool_use").collect();
        if tool_uses.is_empty() {
            println!("No tool calls, stopping.");
            break;
        }

        let mut tool_results = Vec::new();
        let mut is_done = false;

        for tool_use in &tool_uses {
            let name = tool_use["name"].as_str().unwrap_or("");
            let id = tool_use["id"].as_str().unwrap_or("");
            let input = &tool_use["input"];

            if name == "done" {
                is_done = true;
                let summary = input["summary"].as_str().unwrap_or("(no summary)");
                println!("  DONE: {}", summary);
                tool_results.push(json!({
                    "type": "tool_result",
                    "tool_use_id": id,
                    "content": format!("Done: {}", summary),
                }));
                continue;
            }

            println!(
                "  Tool: {}({})",
                name,
                serde_json::to_string(input).unwrap_or_default()
            );

            let result = execute_tool(&mut agent, name, input, &full_context).await;
            let (text_result, is_error) = match result {
                Ok(r) => (r, false),
                Err(e) => (format!("Error: {}", e), true),
            };

            let truncated = if text_result.len() > 4000 {
                format!("{}...[truncated]", &text_result[..4000])
            } else {
                text_result
            };

            println!("  => {}", &truncated[..truncated.len().min(300)]);

            tool_results.push(json!({
                "type": "tool_result",
                "tool_use_id": id,
                "content": truncated,
                "is_error": is_error,
            }));
        }

        messages.push(json!({ "role": "user", "content": tool_results }));

        if is_done {
            break;
        }

        // Trim conversation — keep first message + last 40 messages
        if messages.len() > 50 {
            let first = messages[0].clone();
            let keep_from = messages.len() - 40;
            let tail: Vec<Value> = messages.drain(1..).skip(keep_from - 1).collect();
            messages = vec![first];
            messages.extend(tail);
        }
    }

    let elapsed = start.elapsed();
    println!("\n=== METRICS ===");
    println!("Time: {:.1}s", elapsed.as_secs_f64());
    println!("Turns: {}", messages.len() / 2);
    println!("Input tokens: {}", total_input_tokens);
    println!("Output tokens: {}", total_output_tokens);
    println!("Total tokens: {}", total_input_tokens + total_output_tokens);
    // Rough cost estimate (sonnet pricing: $3/MTok input, $15/MTok output)
    let cost = (total_input_tokens as f64 * 3.0 + total_output_tokens as f64 * 15.0) / 1_000_000.0;
    println!("Est. cost: ${:.4}", cost);

    agent.wait(2000).await;
    browser.close().await?;
    Ok(())
}

async fn call_api_with_retry(
    http: &Client,
    api_key: &str,
    api_base: &str,
    use_openrouter: bool,
    body: &Value,
) -> anyhow::Result<Value> {
    for attempt in 0..10 {
        let (url, req_body) = if use_openrouter {
            // OpenRouter uses OpenAI-compatible chat completions format
            let model = body["model"]
                .as_str()
                .unwrap_or("anthropic/claude-sonnet-4");
            // Map Anthropic model names to OpenRouter names
            let or_model = if model.contains("haiku") {
                "anthropic/claude-3-5-haiku"
            } else if model.contains("sonnet") {
                "anthropic/claude-sonnet-4"
            } else if model.contains("opus") {
                "anthropic/claude-opus-4"
            } else {
                model
            };

            // Convert Anthropic tools format to OpenAI tools format
            let tools: Vec<Value> = body["tools"]
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .map(|t| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": t["name"],
                            "description": t["description"],
                            "parameters": t["input_schema"]
                        }
                    })
                })
                .collect();

            // Build messages with system as first message
            let mut messages = Vec::new();
            if let Some(sys) = body["system"].as_str() {
                messages.push(json!({"role": "system", "content": sys}));
            }
            if let Some(msgs) = body["messages"].as_array() {
                for msg in msgs {
                    // Convert Anthropic tool_result format to OpenAI format
                    if let Some(content) = msg["content"].as_array() {
                        let has_tool_results = content.iter().any(|c| c["type"] == "tool_result");
                        if has_tool_results {
                            for c in content {
                                if c["type"] == "tool_result" {
                                    messages.push(json!({
                                        "role": "tool",
                                        "tool_call_id": c["tool_use_id"],
                                        "content": c["content"]
                                    }));
                                }
                            }
                            continue;
                        }
                        // Convert assistant messages with tool_use blocks
                        let has_tool_use = content.iter().any(|c| c["type"] == "tool_use");
                        if has_tool_use {
                            let text_parts: Vec<&str> = content
                                .iter()
                                .filter(|c| c["type"] == "text")
                                .filter_map(|c| c["text"].as_str())
                                .collect();
                            let tool_calls: Vec<Value> = content.iter()
                                .filter(|c| c["type"] == "tool_use")
                                .map(|c| json!({
                                    "id": c["id"],
                                    "type": "function",
                                    "function": {
                                        "name": c["name"],
                                        "arguments": serde_json::to_string(&c["input"]).unwrap_or_default()
                                    }
                                }))
                                .collect();
                            let mut m = json!({
                                "role": "assistant",
                                "tool_calls": tool_calls
                            });
                            if !text_parts.is_empty() {
                                m["content"] = json!(text_parts.join("\n"));
                            }
                            messages.push(m);
                            continue;
                        }
                    }
                    messages.push(msg.clone());
                }
            }

            let or_body = json!({
                "model": or_model,
                "max_tokens": body["max_tokens"],
                "messages": messages,
                "tools": tools,
            });
            (format!("{}/chat/completions", api_base), or_body)
        } else {
            (format!("{}/messages", api_base), body.clone())
        };

        let mut req = http.post(&url).header("content-type", "application/json");

        if use_openrouter {
            req = req.header("Authorization", format!("Bearer {}", api_key));
        } else {
            req = req
                .header("x-api-key", api_key)
                .header("anthropic-version", "2023-06-01");
        }

        let resp = req.json(&req_body).send().await?;
        let status = resp.status();
        let json: Value = resp.json().await?;

        if status == 429
            || (json.get("error").is_some()
                && (json["error"]["type"] == "rate_limit_error"
                    || json["error"]["code"] == "rate_limit_exceeded"))
        {
            let wait = (attempt + 1) * 5;
            eprintln!("  Rate limited, waiting {}s...", wait);
            tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
            continue;
        }

        // If OpenRouter, convert response back to Anthropic format
        if use_openrouter {
            return Ok(convert_openrouter_response(json));
        }

        return Ok(json);
    }
    anyhow::bail!("Rate limited after 10 retries")
}

fn convert_openrouter_response(resp: Value) -> Value {
    // Convert OpenAI chat completion format to Anthropic messages format
    let choice = &resp["choices"][0];
    let message = &choice["message"];

    let mut content = Vec::new();

    // Text content
    if let Some(text) = message["content"].as_str() {
        if !text.is_empty() {
            content.push(json!({"type": "text", "text": text}));
        }
    }

    // Tool calls
    if let Some(tool_calls) = message["tool_calls"].as_array() {
        for tc in tool_calls {
            let args: Value =
                serde_json::from_str(tc["function"]["arguments"].as_str().unwrap_or("{}"))
                    .unwrap_or(json!({}));
            content.push(json!({
                "type": "tool_use",
                "id": tc["id"],
                "name": tc["function"]["name"],
                "input": args
            }));
        }
    }

    let stop_reason = match choice["finish_reason"].as_str() {
        Some("tool_calls") => "tool_use",
        Some("stop") => "end_turn",
        Some(other) => other,
        None => "end_turn",
    };

    json!({
        "content": content,
        "stop_reason": stop_reason,
        "usage": {
            "input_tokens": resp["usage"]["prompt_tokens"],
            "output_tokens": resp["usage"]["completion_tokens"]
        }
    })
}

async fn execute_tool(
    agent: &mut AgentPage<'_>,
    name: &str,
    input: &Value,
    full_context: &Option<String>,
) -> anyhow::Result<String> {
    match name {
        "navigate" => {
            let url = input["url"].as_str().unwrap_or("about:blank");
            agent.goto(url).await?;
            agent.wait(1500).await;
            let u = agent.url().await?;
            Ok(format!("Navigated to: {}", u))
        }
        "observe" => {
            agent.observe().await?;
            let list = agent.element_list();
            let lines: Vec<&str> = list.lines().take(40).collect();
            let truncated = lines.join("\n");
            let total = agent.len();
            Ok(if truncated.is_empty() {
                "No interactive elements found.".into()
            } else if total > 40 {
                format!(
                    "{}\n[...{} total elements, showing first 40]",
                    truncated, total
                )
            } else {
                truncated
            })
        }
        "click" => {
            let idx = input["index"].as_u64().unwrap_or(0) as usize;
            agent.click(idx).await?;
            agent.wait(300).await;
            Ok(format!("Clicked [{}]", idx))
        }
        "fill" => {
            let idx = input["index"].as_u64().unwrap_or(0) as usize;
            let text = input["text"].as_str().unwrap_or("");
            match agent.fill(idx, text).await {
                Ok(()) => Ok(format!("Filled [{}] with '{}'", idx, text)),
                Err(_) => {
                    let el = agent
                        .get(idx)
                        .ok_or_else(|| anyhow::anyhow!("no element [{}]", idx))?;
                    let sel_json = serde_json::to_string(&el.selector)?;
                    let val_json = serde_json::to_string(text)?;
                    let js = format!(
                        "(() => {{ const el = document.querySelector({}); if (!el) return 'not found'; \
                         const s = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype,'value').set; \
                         s.call(el,{}); el.dispatchEvent(new Event('input',{{bubbles:true}})); \
                         el.dispatchEvent(new Event('change',{{bubbles:true}})); return 'ok'; }})()",
                        sel_json, val_json
                    );
                    let r: String = agent.page().evaluate(&js).await?;
                    Ok(format!("Filled [{}] via JS: {}", idx, r))
                }
            }
        }
        "hover" => {
            let idx = input["index"].as_u64().unwrap_or(0) as usize;
            agent.hover(idx).await?;
            agent.wait(300).await;
            Ok(format!("Hovered [{}]", idx))
        }
        "scroll" => {
            let target = input["target"].as_str().unwrap_or("down");
            match target {
                "up" => agent.scroll_up().await?,
                "down" => agent.scroll_down().await?,
                "top" => agent.scroll_to_top().await?,
                "bottom" => agent.scroll_to_bottom().await?,
                other => {
                    let idx: usize = other.parse()?;
                    agent.scroll_to(idx).await?;
                }
            }
            let _ = agent
                .exec("window.dispatchEvent(new Event('scroll'))")
                .await;
            agent.wait(200).await;
            let scroll_y: String = agent
                .page()
                .evaluate("String(Math.round(window.scrollY))")
                .await
                .unwrap_or_else(|_| "?".into());
            Ok(format!("Scrolled {} (scrollY={})", target, scroll_y))
        }
        "type_key" => {
            let key = input["key"].as_str().unwrap_or("Enter");
            agent.press_key(key).await?;
            Ok(format!("Pressed {}", key))
        }
        "extract" => {
            let js = input["js"].as_str().unwrap_or("null");
            let result: String = agent.page().evaluate(&format!(
                "(() => {{ try {{ const __r = (() => {{ {} }})(); if (__r === undefined || __r === null) return 'null'; return typeof __r === 'string' ? __r : JSON.stringify(__r); }} catch(e) {{ return 'Error: ' + e.message; }} }})()",
                js
            )).await.unwrap_or_else(|e| format!("eval error: {}", e));
            Ok(result)
        }
        "page_text" => {
            let text = agent.text().await?;
            Ok(text.chars().take(2000).collect())
        }
        "screenshot" => {
            let png = agent.screenshot().await?;
            let _b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &png);
            Ok(format!(
                "[Screenshot: {} bytes, {} elements annotated]",
                png.len(),
                agent.len()
            ))
        }
        "wait" => {
            let ms = input["ms"].as_u64().unwrap_or(1000);
            agent.wait(ms).await;
            Ok(format!("Waited {}ms", ms))
        }
        "lookup_context" => {
            let query = input["query"].as_str().unwrap_or("");
            match full_context {
                Some(ctx) => {
                    let query_lower = query.to_lowercase();
                    let lines: Vec<&str> = ctx.lines().collect();
                    let mut matches: Vec<String> = Vec::new();
                    for (i, line) in lines.iter().enumerate() {
                        if line.to_lowercase().contains(&query_lower) {
                            // Return surrounding context (5 lines before/after)
                            let start = i.saturating_sub(5);
                            let end = (i + 6).min(lines.len());
                            let snippet: String = lines[start..end].join("\n");
                            if matches
                                .iter()
                                .all(|m| !m.contains(&snippet[..snippet.len().min(50)]))
                            {
                                matches.push(snippet);
                            }
                            if matches.len() >= 5 {
                                break;
                            }
                        }
                    }
                    if matches.is_empty() {
                        Ok(format!("No matches for '{}' in reference doc.", query))
                    } else {
                        Ok(matches.join("\n---\n"))
                    }
                }
                None => Ok("No full reference document available.".into()),
            }
        }
        _ => Err(anyhow::anyhow!("Unknown tool: {}", name)),
    }
}
