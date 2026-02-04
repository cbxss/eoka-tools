//! Recon agent: analyzes a website's JS bundle and dumps findings to a context file.
//!
//! Usage:
//!   ANTHROPIC_API_KEY=... cargo run --example recon_agent -- https://serene-frangipane-7fd25b.netlify.app -o context.txt
//!
//! What it does:
//! 1. Launches browser, navigates to the URL
//! 2. Discovers all JS bundles loaded by the page
//! 3. Fetches and formats each bundle
//! 4. Sends chunks to the LLM asking it to reverse-engineer key logic
//! 5. Writes consolidated findings to the output file
//!
//! The output file can then be passed as --context to generic_agent.

use reqwest::Client;
use serde_json::{json, Value};
use std::time::Instant;

const MODEL: &str = "claude-sonnet-4-20250514";

const RECON_SYSTEM_PROMPT: &str = r#"You are a reverse-engineering agent analyzing JavaScript source code from a web application.

Your job is to extract ALL information that would help an automation agent interact with this site. Be thorough and precise.

Extract and document:
1. ROUTING: How does navigation work? (React Router, hash routing, server-side, etc.) What are the routes/paths?
2. STATE MANAGEMENT: How is state stored? (React state, Redux, sessionStorage, localStorage, cookies, URL params)
3. VALIDATION: Any input validation, code checking, token verification logic. Include the actual functions if short enough.
4. KEY FUNCTIONS: Any deterministic functions (code generators, hash functions, token creators). Include the EXACT source code.
5. ANTI-AUTOMATION: Popups, overlays, decoy buttons, CAPTCHAs, bot detection. How to handle each.
6. INTERACTION PATTERNS: What user interactions does the app expect? (clicks, scrolls, hovers, drag-drop, keyboard)
7. API CALLS: Any fetch/XHR calls, WebSocket connections, service workers.
8. DOM STRUCTURE: Key selectors, class naming patterns, component structure.
9. WORKFLOW: The expected user flow from start to finish.
10. GOTCHAS: Anything that would trip up an automation agent (timers, race conditions, dynamic content).

Format your output as a clean reference document that another AI agent can use as a system prompt.
Be CONCISE but COMPLETE. Include actual code snippets for key functions.
Do NOT include generic advice — only site-specific findings from the actual code.
"#;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let api_key = std::env::var("ANTHROPIC_API_KEY").expect("Set ANTHROPIC_API_KEY env var");

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut url: Option<String> = None;
    let mut output_path = "context.txt".to_string();
    let mut cheatsheet_path: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => {
                i += 1;
                output_path = args.get(i).expect("-o requires a file path").clone();
            }
            "--cheatsheet" => {
                i += 1;
                cheatsheet_path = Some(
                    args.get(i)
                        .expect("--cheatsheet requires a file path")
                        .clone(),
                );
            }
            _ => {
                if url.is_none() {
                    url = Some(args[i].clone());
                }
            }
        }
        i += 1;
    }

    let url = url.unwrap_or_else(|| {
        eprintln!("Usage: recon_agent <URL> [-o output.txt]");
        std::process::exit(1);
    });

    println!("Recon target: {}", url);
    println!("Output: {}", output_path);

    let start = Instant::now();
    let http = Client::new();

    // Phase 1: Discover JS bundles via browser
    println!("\n[1/4] Launching browser and discovering JS bundles...");

    let browser = eoka::Browser::launch().await?;
    let page = browser.new_page(&url).await?;

    // Wait for page to load
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    // Get all script sources
    let scripts_json: String = page.evaluate(r#"
        (() => {
            const scripts = Array.from(document.querySelectorAll('script[src]'))
                .map(s => s.src)
                .filter(s => !s.includes('analytics') && !s.includes('gtag') && !s.includes('hotjar'));
            // Also get inline script content lengths
            const inline = Array.from(document.querySelectorAll('script:not([src])'))
                .map(s => s.textContent.length)
                .filter(l => l > 100);
            return JSON.stringify({ external: scripts, inline_sizes: inline, page_url: location.href });
        })()
    "#).await?;

    let scripts_info: Value = serde_json::from_str(&scripts_json)?;
    let external_scripts: Vec<String> = scripts_info["external"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();

    println!("  Found {} external JS bundles", external_scripts.len());

    // Also grab page HTML structure (simplified)
    let page_structure: String = page
        .evaluate(
            r#"
        (() => {
            // Get a simplified DOM snapshot
            function simplify(el, depth) {
                if (depth > 4) return '';
                const tag = el.tagName?.toLowerCase() || '';
                if (['script','style','svg','path'].includes(tag)) return '';
                const id = el.id ? `#${el.id}` : '';
                const cls = el.className && typeof el.className === 'string'
                    ? '.' + el.className.split(' ').filter(c => c.length > 0).slice(0, 3).join('.')
                    : '';
                const text = el.childNodes.length === 1 && el.childNodes[0].nodeType === 3
                    ? ` "${el.textContent.trim().slice(0, 40)}"` : '';
                const indent = '  '.repeat(depth);
                let result = `${indent}<${tag}${id}${cls}${text}>\n`;
                for (const child of el.children) {
                    result += simplify(child, depth + 1);
                }
                return result;
            }
            return simplify(document.body, 0).slice(0, 3000);
        })()
    "#,
        )
        .await
        .unwrap_or_default();

    browser.close().await?;

    // Phase 2: Fetch and format JS bundles
    println!("\n[2/4] Fetching JS bundles...");

    let mut js_sources: Vec<(String, String)> = Vec::new();
    for script_url in &external_scripts {
        println!("  Fetching: {}", script_url);
        match http.get(script_url).send().await {
            Ok(resp) => {
                if let Ok(body) = resp.text().await {
                    js_sources.push((script_url.clone(), body));
                }
            }
            Err(e) => eprintln!("  Failed: {}", e),
        }
    }

    // Phase 2b: Format with prettier if available
    println!("\n[2b/4] Formatting JS bundles...");
    let mut formatted_sources: Vec<(String, String)> = Vec::new();
    for (script_url, source) in &js_sources {
        // Try prettier, fall back to raw
        let formatted = match try_prettier(source).await {
            Some(f) => {
                println!(
                    "  Formatted {} with prettier ({} → {} bytes)",
                    script_url,
                    source.len(),
                    f.len()
                );
                f
            }
            None => {
                println!(
                    "  Using raw source for {} ({} bytes)",
                    script_url,
                    source.len()
                );
                source.clone()
            }
        };
        formatted_sources.push((script_url.clone(), formatted));
    }

    // Phase 2c: Extract relevant code blocks using keyword search
    println!("\n[2c/4] Extracting app-specific code blocks...");

    // Keywords that indicate app logic vs library code
    let app_keywords = [
        // Domain-specific
        "challenge",
        "step",
        "code",
        "submit",
        "validate",
        "interaction",
        "score",
        "timer",
        "puzzle",
        "reveal",
        "hidden",
        "secret",
        // State/storage
        "sessionStorage",
        "localStorage",
        "cookie",
        // Navigation patterns
        "navigate(",
        "/step",
        "/finish",
        "version",
        // DOM interaction
        "data-challenge",
        "data-code",
        "data-token",
        // Crypto/encoding
        "atob",
        "btoa",
        "randomUUID",
        "crypto.",
        // App structure
        "function App",
        "createBrowserRouter",
        "routes",
        // Anti-automation
        "popup",
        "overlay",
        "decoy",
        "fake",
        "Wrong Button",
        "z-index",
        "zIndex",
        "dismiss",
        // Canvas/media
        "canvas",
        "getContext",
        "AudioContext",
        "WebSocket",
    ];

    let mut total_input_tokens: u64 = 0;
    let mut total_output_tokens: u64 = 0;
    let mut all_findings: Vec<String> = Vec::new();

    // Add page structure as context
    all_findings.push(format!(
        "=== PAGE STRUCTURE ===\nURL: {}\n{}",
        url, page_structure
    ));

    for (script_url, source) in &formatted_sources {
        let lines: Vec<&str> = source.lines().collect();
        println!("  {} has {} lines", script_url, lines.len());

        // Extract blocks around keyword matches with context
        let mut relevant_blocks: Vec<(usize, String)> = Vec::new();
        let mut covered: std::collections::HashSet<usize> = std::collections::HashSet::new();
        let context_lines = 15; // lines of context around each match

        for (line_num, line) in lines.iter().enumerate() {
            let line_lower = line.to_lowercase();
            let is_relevant = app_keywords
                .iter()
                .any(|kw| line_lower.contains(&kw.to_lowercase()));
            if !is_relevant {
                continue;
            }
            if covered.contains(&line_num) {
                continue;
            }

            // Expand to surrounding context, trying to capture full function bodies
            let start = line_num.saturating_sub(context_lines);
            let end = (line_num + context_lines + 1).min(lines.len());

            // Try to extend to function boundaries (find enclosing { })
            let mut block_start = start;
            let mut block_end = end;

            // Walk back to find function/const/class declaration
            for j in (0..start).rev() {
                let l = lines[j].trim();
                if l.starts_with("function ")
                    || l.starts_with("const ")
                    || l.starts_with("class ")
                    || l.starts_with("let ")
                    || l.starts_with("var ")
                    || l.contains("=> {")
                    || l.contains("= function")
                {
                    block_start = j;
                    break;
                }
                if l.is_empty() || l == "}" || l == "}," || l == "});" {
                    block_start = j + 1;
                    break;
                }
            }

            // Walk forward to find closing brace (track nesting)
            let mut depth: i32 = 0;
            for j in block_start..lines.len().min(block_end + 100) {
                for ch in lines[j].chars() {
                    if ch == '{' {
                        depth += 1;
                    }
                    if ch == '}' {
                        depth -= 1;
                    }
                }
                if depth <= 0 && j >= line_num {
                    block_end = j + 1;
                    break;
                }
            }

            // Mark lines as covered
            for j in block_start..block_end {
                covered.insert(j);
            }

            let block: String = lines[block_start..block_end].join("\n");
            // Skip tiny or huge blocks
            if block.len() > 50 && block.len() < 20_000 {
                // Score by keyword density — more keyword hits = more likely app logic
                let block_lower = block.to_lowercase();
                let score: usize = app_keywords
                    .iter()
                    .map(|kw| block_lower.matches(&kw.to_lowercase()).count())
                    .sum();
                relevant_blocks.push((
                    score,
                    format!(
                        "// Lines {}-{} (relevance: {})\n{}",
                        block_start + 1,
                        block_end,
                        score,
                        block
                    ),
                ));
            }
        }

        // Sort by relevance score descending — densest app logic first
        relevant_blocks.sort_by(|a, b| b.0.cmp(&a.0));

        // Deduplicate overlapping blocks and cap total size
        let mut combined = String::new();
        let max_size = 120_000; // ~120KB of relevant code to send
        for (_score, block) in &relevant_blocks {
            if combined.len() + block.len() > max_size {
                break;
            }
            combined.push_str(block);
            combined.push_str("\n\n");
        }

        println!(
            "  Extracted {} relevant blocks ({} bytes from {} total)",
            relevant_blocks.len(),
            combined.len(),
            source.len()
        );

        // Split into ~40KB batches so the LLM can focus on each chunk
        let batch_size = 40_000;
        let mut batches: Vec<String> = Vec::new();

        if combined.is_empty() {
            // No relevant blocks found, fall back to last 100KB split into batches
            println!("  No relevant blocks found, falling back to last 100KB");
            let fallback_start = source.len().saturating_sub(100_000);
            let fallback = &source[fallback_start..];
            for chunk in fallback.as_bytes().chunks(batch_size) {
                if let Ok(s) = std::str::from_utf8(chunk) {
                    batches.push(s.to_string());
                }
            }
        } else {
            let mut current_batch = String::new();
            for (_score, block) in &relevant_blocks {
                if current_batch.len() + block.len() > batch_size && !current_batch.is_empty() {
                    batches.push(current_batch.clone());
                    current_batch.clear();
                }
                if current_batch.len() + block.len() <= batch_size * 3 {
                    // don't skip huge blocks
                    current_batch.push_str(block);
                    current_batch.push_str("\n\n");
                }
            }
            if !current_batch.is_empty() {
                batches.push(current_batch);
            }
        }

        // Cap at 4 batches to stay within budget
        batches.truncate(4);

        println!("  Sending {} batches to LLM for analysis...", batches.len());

        for (bi, batch) in batches.iter().enumerate() {
            println!(
                "    Batch {}/{} ({} bytes)...",
                bi + 1,
                batches.len(),
                batch.len()
            );

            let user_msg = format!(
                "Analyze this extracted application code (batch {}/{}, filtered from {} to keep only app logic). \
                 Focus on: functions, validation, code generation, tokens, navigation, anti-automation.\n\
                 Include COMPLETE function source code for anything important.\n\n\
                 Source URL: {}\nPage URL: {}\n\n```javascript\n{}\n```",
                bi + 1, batches.len(), script_url, script_url, url, batch
            );

            let body = json!({
                "model": MODEL,
                "max_tokens": 8192,
                "system": RECON_SYSTEM_PROMPT,
                "messages": [{ "role": "user", "content": user_msg }],
            });

            let resp_json = call_api(&http, &api_key, &body).await?;

            if let Some(usage) = resp_json.get("usage") {
                total_input_tokens += usage["input_tokens"].as_u64().unwrap_or(0);
                total_output_tokens += usage["output_tokens"].as_u64().unwrap_or(0);
            }

            if let Some(content) = resp_json["content"].as_array() {
                for block in content {
                    if let Some(text) = block["text"].as_str() {
                        all_findings.push(text.to_string());
                    }
                }
            }
        }
    }

    // Phase 2d: Extract verbatim string literals and short functions from source
    // These are appended RAW to prevent LLM hallucination during consolidation
    println!("\n[2d/4] Extracting verbatim strings and functions...");
    let mut verbatim_section =
        String::from("\n=== VERBATIM EXTRACTIONS (DO NOT MODIFY — COPY EXACTLY) ===\n\n");

    for (_script_url, source) in &formatted_sources {
        // Extract quoted string literals that look like charsets, keys, or identifiers
        let string_re = regex::Regex::new(r#""([A-Z0-9]{10,})""#).unwrap();
        for cap in string_re.captures_iter(source) {
            let s = &cap[1];
            if s.len() >= 10 && s.len() <= 50 {
                verbatim_section.push_str(&format!("String literal: \"{}\"\n", s));
            }
        }

        // Extract sessionStorage/localStorage key patterns
        let storage_re =
            regex::Regex::new(r#"(?:sessionStorage|localStorage)\.\w+\(\s*[`"']([^`"']+)[`"']"#)
                .unwrap();
        for cap in storage_re.captures_iter(source) {
            verbatim_section.push_str(&format!("Storage key pattern: {}\n", &cap[1]));
        }

        // Extract template literal storage keys
        let template_re = regex::Regex::new(r#"(?:setItem|getItem)\(`([^`]+)`"#).unwrap();
        for cap in template_re.captures_iter(source) {
            verbatim_section.push_str(&format!("Storage key template: {}\n", &cap[1]));
        }

        // Extract short named functions (< 500 chars) that contain key patterns
        let lines: Vec<&str> = source.lines().collect();
        let func_patterns = [
            "function Rl",
            "function Re(",
            "function Ev(",
            "function Jr(",
            "function he(",
            "function Cv(",
            "function vv(",
            "function gv(",
            "function ke(",
            "function Sv(",
            "function Sl(",
            "function Pf(",
            "function Tf(",
            "function bv(",
        ];
        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            for pat in &func_patterns {
                if trimmed.contains(pat) {
                    // Extract until matching brace
                    let mut depth: i32 = 0;
                    let mut end = i;
                    for j in i..lines.len().min(i + 50) {
                        for ch in lines[j].chars() {
                            if ch == '{' {
                                depth += 1;
                            }
                            if ch == '}' {
                                depth -= 1;
                            }
                        }
                        if depth <= 0 && j > i {
                            end = j + 1;
                            break;
                        }
                    }
                    let func_body: String = lines[i..end].join("\n");
                    if func_body.len() < 1000 {
                        verbatim_section.push_str(&format!(
                            "\nVerbatim function (line {}):\n{}\n",
                            i + 1,
                            func_body
                        ));
                    }
                }
            }
        }

        // Extract const declarations with string values that look like storage keys or identifiers
        let const_re = regex::Regex::new(r#"const\s+\w+\s*=\s*"([^"]{5,80})""#).unwrap();
        for cap in const_re.captures_iter(source) {
            let val = &cap[1];
            if val.contains("challenge")
                || val.contains("step")
                || val.contains("token")
                || val.contains("interaction")
                || val.contains("storage")
                || val.contains("code")
            {
                verbatim_section.push_str(&format!("Const string: {}\n", &cap[0]));
            }
        }

        // Extract array literals that look like challenge method lists
        let array_re = regex::Regex::new(r#"\[(?:\s*"[a-z_]+"\s*,\s*){3,}[^\]]*\]"#).unwrap();
        for mat in array_re.find_iter(source) {
            let s = mat.as_str();
            if s.len() < 500
                && (s.contains("visible")
                    || s.contains("hidden")
                    || s.contains("click")
                    || s.contains("scroll"))
            {
                verbatim_section.push_str(&format!("\nChallenge method array:\n{}\n", s));
            }
        }
    }

    all_findings.push(verbatim_section.clone());
    println!("  Verbatim section: {} bytes", verbatim_section.len());

    // Phase 3: Consolidate findings into a single context file
    println!("\n[4/4] Consolidating findings...");

    // Ask LLM to consolidate all findings into a clean reference doc
    let consolidation_prompt = format!(
        "Below are raw analysis findings from reverse-engineering a website's JavaScript.\n\
         Consolidate into a SINGLE reference document for a browser automation agent.\n\n\
         CRITICAL REQUIREMENTS:\n\
         - Include the COMPLETE source code of ALL key functions (validation, code generation, \
         token creation, navigation, state management). Do NOT summarize or truncate function bodies.\n\
         - Include exact variable names, selectors, class names, z-index values.\n\
         - Include the exact workflow: what must happen in what order for each step.\n\
         - Describe every anti-automation obstacle and how to defeat it.\n\
         - If a function generates codes/tokens, include the FULL implementation so the agent can recompute them.\n\
         - Format as plain text, no markdown headers. Suitable for LLM system prompt injection.\n\n\
         Target URL: {}\n\n{}",
        url,
        all_findings.join("\n\n---\n\n")
    );

    let body = json!({
        "model": MODEL,
        "max_tokens": 16384,
        "system": "You consolidate technical analysis into reference documents. \
                   Output ONLY the document. NEVER truncate function bodies — include complete source code \
                   for all important functions. The automation agent needs exact code to recompute values.",
        "messages": [{ "role": "user", "content": consolidation_prompt }],
    });

    let resp_json = call_api(&http, &api_key, &body).await?;

    if let Some(usage) = resp_json.get("usage") {
        total_input_tokens += usage["input_tokens"].as_u64().unwrap_or(0);
        total_output_tokens += usage["output_tokens"].as_u64().unwrap_or(0);
    }

    let mut final_doc = String::new();
    if let Some(content) = resp_json["content"].as_array() {
        for block in content {
            if let Some(text) = block["text"].as_str() {
                final_doc.push_str(text);
            }
        }
    }

    // Generate cheatsheet (compact summary for every-turn context)
    let cheatsheet_out = if cheatsheet_path.is_some() {
        println!("\n[5/5] Generating cheatsheet...");
        let cs_prompt = format!(
            "Below is a full reference document for a website. Create a COMPACT cheatsheet (under 1500 bytes) \
             that contains ONLY:\n\
             1. Key function signatures and their purpose (1 line each)\n\
             2. Exact storage key patterns\n\
             3. Core workflow steps (numbered, 1 line each)\n\
             4. Critical selectors/patterns for interacting with the page\n\
             5. Any hardcoded values (charsets, constants, magic numbers)\n\n\
             Do NOT include full function bodies — just signatures and what they return.\n\
             The agent has access to a lookup_context tool to read the full doc when needed.\n\
             Format as plain text, dense, no markdown.\n\n{}",
            final_doc
        );

        let body = json!({
            "model": MODEL,
            "max_tokens": 2048,
            "system": "Output ONLY the cheatsheet. Keep it under 1500 bytes. Be extremely dense and precise.",
            "messages": [{ "role": "user", "content": cs_prompt }],
        });

        let resp_json = call_api(&http, &api_key, &body).await?;
        if let Some(usage) = resp_json.get("usage") {
            total_input_tokens += usage["input_tokens"].as_u64().unwrap_or(0);
            total_output_tokens += usage["output_tokens"].as_u64().unwrap_or(0);
        }

        let mut cs = String::new();
        if let Some(content) = resp_json["content"].as_array() {
            for block in content {
                if let Some(text) = block["text"].as_str() {
                    cs.push_str(text);
                }
            }
        }
        Some(cs)
    } else {
        None
    };

    // Write output
    std::fs::write(&output_path, &final_doc)?;
    if let (Some(path), Some(cs)) = (&cheatsheet_path, &cheatsheet_out) {
        std::fs::write(path, cs)?;
        println!("Cheatsheet: {} ({} bytes)", path, cs.len());
    }
    let elapsed = start.elapsed();

    println!("\n=== RECON COMPLETE ===");
    println!("Output: {} ({} bytes)", output_path, final_doc.len());
    println!("Time: {:.1}s", elapsed.as_secs_f64());
    println!("Input tokens: {}", total_input_tokens);
    println!("Output tokens: {}", total_output_tokens);
    let cost = (total_input_tokens as f64 * 3.0 + total_output_tokens as f64 * 15.0) / 1_000_000.0;
    println!("Est. cost: ${:.4}", cost);

    Ok(())
}

async fn try_prettier(source: &str) -> Option<String> {
    use tokio::process::Command;
    let mut child = Command::new("npx")
        .args([
            "-y",
            "prettier",
            "--parser",
            "babel",
            "--print-width",
            "120",
            "--stdin-filepath",
            "bundle.js",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;

    use tokio::io::AsyncWriteExt;
    let mut stdin = child.stdin.take()?;
    let src = source.to_string();
    tokio::spawn(async move {
        let _ = stdin.write_all(src.as_bytes()).await;
        let _ = stdin.shutdown().await;
    });

    let output = tokio::time::timeout(std::time::Duration::from_secs(30), child.wait_with_output())
        .await
        .ok()?
        .ok()?;

    if output.status.success() {
        String::from_utf8(output.stdout).ok()
    } else {
        None
    }
}

async fn call_api(http: &Client, api_key: &str, body: &Value) -> anyhow::Result<Value> {
    for attempt in 0..10 {
        let resp = http
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(body)
            .send()
            .await?;

        let status = resp.status();
        let json: Value = resp.json().await?;

        if status == 429
            || (json.get("error").is_some() && json["error"]["type"] == "rate_limit_error")
        {
            let wait = (attempt + 1) * 5;
            eprintln!("  Rate limited, waiting {}s...", wait);
            tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
            continue;
        }

        if let Some(err) = json.get("error") {
            anyhow::bail!("API error: {}", err);
        }

        return Ok(json);
    }
    anyhow::bail!("Rate limited after 10 retries")
}
