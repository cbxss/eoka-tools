//! Agentic loop: Claude API reasons, eoka-agent acts.
//!
//! Two-tier: deterministic tools handle the grunt work (scan for codes,
//! dismiss popups, submit+navigate). The LLM only decides strategy.
//! Set ANTHROPIC_API_KEY env var before running.

use eoka::Browser;
use eoka_agent::AgentPage;
use reqwest::Client;
use serde_json::{json, Value};

const MODEL: &str = "claude-3-5-haiku-20241022";
const MAX_TURNS: usize = 300;

const SYSTEM_PROMPT: &str = r#"You are a browser automation agent. Be EXTREMELY concise — just tool calls, minimal text.

Goal: Solve all 30 steps of the Browser Navigation Challenge.

=== REVERSED APP INTERNALS ===
The app is a React SPA on Netlify. Key internal details from reversing the JS bundle:

CODE GENERATION: Deterministic function Rl(stepNum+1, version) using charset "ABCDEFGHJKLMNPQRSTUVWXYZ23456789" (no I, O, 0, 1). Codes are always 6 chars. You can compute any code with:
  function Rl(o, l) { const s="ABCDEFGHJKLMNPQRSTUVWXYZ23456789"; let i=""; const d=(o*7919+12345)*l, f=(o*1237+67890)*l, p=(o*4567+98765)*l; for(let h=0;h<6;h++){const y=((d*(h+1)+f*(h*2+1)+p*(h*3+2))%2147483647)%s.length; i+=s[Math.abs(y)];} return i; }
  Code for step N with version V = Rl(N+1, V). Version is in the URL query param "version" (1-3).

VALIDATION: Two checks must BOTH pass:
1. Input code must match Rl(stepNum+1, version)
2. sessionStorage must have key "challenge_interaction_step_N" with a JSON token {token, interactionType, completedAt}
   → submit_code_and_next sets this automatically, but the challenge component ALSO sets it when you interact (click reveal, scroll, hover, etc.)
   → If you skip the interaction, submit will say "Complete the challenge before submitting"

CHALLENGE METHODS by step (version 1):
Steps 1-5: visible, hidden_dom, click_reveal, scroll_reveal, delayed_reveal
Steps 6-10: drag_drop, keyboard_sequence, memory, hover_reveal, click_reveal
Steps 11-15: timing, canvas, audio, video, split_parts (then encoded_base64, rotating, obfuscated cycle)
Steps 16-20: multi_tab, gesture, sequence, puzzle_solve, calculated
Steps 21-30: shadow_dom, websocket, service_worker, mutation, recursive_iframe, conditional_reveal, multi_tab, sequence, calculated (cycling)
Method = methods_array[(stepNum - offset + version - 1) % array.length]

NAVIGATION: On correct submit, React Router navigates to /step{N+1}?version={V} after 500ms. Do NOT navigate() directly — Netlify has no catch-all redirect so direct URLs 404.

POPUPS: Random popups/overlays/cookie banners spawn on timers. Many have fake close buttons. All have a real "Dismiss" or "Close" button. The main content is z-index 10002-10005. Fake nav buttons call a "Wrong Button!" handler.

DECOY BUTTONS: 8-18 fake nav buttons with random text from: Next, Continue, Proceed, Go Forward, Next Page, Click Here, Continue Reading, Next Step, Move On, Advance, Keep Going, Next Section, Proceed Forward, Continue Journey. ALL of these are decoys — clicking them shows "Wrong Button! Try Again!". There is NO real navigation button — the app auto-navigates via React Router after correct code submit.

=== WORKFLOW ===
1. scan_for_code to find the code (checks data attrs, storage, shadow DOM, visible text, etc.)
2. If code not found, you need to trigger the interaction first:
   - Read the hint from scan_for_code to know the challenge type
   - click_reveal: observe, click "Reveal Code" button, scan again
   - scroll_reveal: scroll down 500px+, scan again
   - hover_reveal: observe, hover over the target element, scan again
   - hidden_dom: scan_for_code already checks data attributes
   - delayed_reveal: wait 3s, scan again
   - visible: code is already visible, scan again
   - For complex types (canvas, audio, keyboard, etc.): use extract with custom JS
3. ALTERNATIVELY: compute the code directly with extract:
   extract({js: "const v = new URLSearchParams(location.search).get('version') || 1; function Rl(o,l){const s='ABCDEFGHJKLMNPQRSTUVWXYZ23456789';let i='';const d=(o*7919+12345)*l,f=(o*1237+67890)*l,p=(o*4567+98765)*l;for(let h=0;h<6;h++){const y=((d*(h+1)+f*(h*2+1)+p*(h*3+2))%2147483647)%s.length;i+=s[Math.abs(y)];}return i;} const step=parseInt(location.pathname.match(/step(\\d+)/)[1]); return Rl(step+1, parseInt(v));"})
   This gives the correct code for ANY step without needing to find it on the page.
4. submit_code_and_next with the code — it also sets the interaction token.
5. After submit succeeds and says "Now on step X", immediately scan_for_code or compute code for the next step.

CRITICAL RULES:
- NEVER stop early. NEVER ask "would you like me to continue". FULLY AUTONOMOUS.
- NEVER use end_turn. Always make a tool call.
- Keep going until all 30 steps are done, then call done.
- If scan_for_code returns empty after 2 attempts, use the extract+Rl approach to compute the code directly.
- After submit_code_and_next succeeds, IMMEDIATELY proceed to the next step.
"#;

fn tool_definitions() -> Value {
    json!([
        {
            "name": "scan_for_code",
            "description": "Deterministic scan of ALL code hiding spots: data attributes, localStorage, sessionStorage, cookies, URL params, shadow DOM (3 levels), iframes, hidden elements, base64 in text, visible text, HTML comments, CSS ::before/::after content. Returns candidate 6-char alphanumeric codes with their source. Also dismisses popups first. Call this FIRST on every step.",
            "input_schema": { "type": "object", "properties": {} }
        },
        {
            "name": "submit_code_and_next",
            "description": "Enter a code into the input field, click Submit Code, dismiss popups, and navigate to the next step. Returns the new page URL and step number.",
            "input_schema": {
                "type": "object",
                "properties": { "code": { "type": "string", "description": "The 6-character code to submit" } },
                "required": ["code"]
            }
        },
        {
            "name": "navigate",
            "description": "Navigate to a URL.",
            "input_schema": {
                "type": "object",
                "properties": { "url": { "type": "string", "description": "URL" } },
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
            "description": "Scroll: 'up', 'down', 'top', 'bottom', or element index.",
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
            "description": "Run JS in the page, return result. For custom interaction logic only — scan_for_code covers standard searches.",
            "input_schema": {
                "type": "object",
                "properties": { "js": { "type": "string" } },
                "required": ["js"]
            }
        },
        {
            "name": "page_text",
            "description": "Get visible page text (truncated to 1500 chars).",
            "input_schema": { "type": "object", "properties": {} }
        },
        {
            "name": "screenshot",
            "description": "Annotated screenshot. Use only for visual challenges (canvas, images).",
            "input_schema": { "type": "object", "properties": {} }
        },
        {
            "name": "wait",
            "description": "Wait N milliseconds (for timed/delayed reveals).",
            "input_schema": {
                "type": "object",
                "properties": { "ms": { "type": "integer", "description": "Milliseconds to wait" } },
                "required": ["ms"]
            }
        },
        {
            "name": "done",
            "description": "Signal completion or giving up.",
            "input_schema": {
                "type": "object",
                "properties": { "reason": { "type": "string" } },
                "required": ["reason"]
            }
        }
    ])
}

// The big JS that searches everywhere for codes
const SCAN_JS: &str = r#"(() => {
    // Dismiss popups (hide, don't remove — removing destroys React tree)
    document.querySelectorAll('[class*=modal],[class*=popup],[class*=overlay],[class*=cookie],[class*=consent],[class*=notice]').forEach(e => {
        if (e.offsetHeight > 50) e.style.display = 'none';
    });

    const found = [];
    const seen = new Set();
    function add(code, source) {
        if (seen.has(code)) return;
        seen.add(code);
        found.push({code, source});
    }

    const CODE_RE = /\b[A-Z0-9]{6}\b/g;
    // Words that match the pattern but aren't codes
    const SKIP = new Set(['SUBMIT','BUTTON','SCROLL','REVEAL','HIDDEN','SELECT','COOKIE','ACCEPT','CANCEL','CHANGE','DELETE','UPDATE','SEARCH','CREATE','RETURN','SIMPLE','PUZZLE','DECODE','ENCODE','SHADOW','BORDER','INLINE','NOWRAP','MEDIUM','COLORS','EVENTS','SHRINK','CURSOR','CENTER','LENGTH','MODULE','DEVICE','ASSETS','SCREEN','YELLOW','WIDEST','IFRAME','LOADED','APPEAR','BEFORE','NORMAL','CUSTOM','RANDOM','FINISH','FILTER','HEADER','LAYOUT','RESIZE','SCRIPT','ORIGIN','DESIGN','CHROME','SAFARI','MOBILE','WEBKIT']);

    function scan(text, source) {
        const matches = text.match(CODE_RE);
        if (matches) matches.forEach(m => { if (!SKIP.has(m)) add(m, source); });
    }

    // 1. Data attributes (most common hiding spot)
    document.querySelectorAll('*').forEach(el => {
        for (const attr of el.attributes) {
            if (attr.name.startsWith('data-') && CODE_RE.test(attr.value)) {
                const m = attr.value.match(CODE_RE);
                if (m) m.forEach(c => { if (!SKIP.has(c)) add(c, 'data-attr:' + attr.name); });
            }
        }
    });

    // 2. localStorage
    for (let i = 0; i < localStorage.length; i++) {
        const k = localStorage.key(i);
        scan(k, 'localStorage-key');
        scan(localStorage.getItem(k) || '', 'localStorage:' + k);
    }

    // 3. sessionStorage
    for (let i = 0; i < sessionStorage.length; i++) {
        const k = sessionStorage.key(i);
        scan(k, 'sessionStorage-key');
        scan(sessionStorage.getItem(k) || '', 'sessionStorage:' + k);
    }

    // 4. Cookies
    scan(document.cookie, 'cookie');

    // 5. URL params
    for (const [k, v] of new URLSearchParams(location.search)) {
        scan(v, 'url-param:' + k);
    }

    // 6. Shadow DOM (3 levels)
    function walkShadow(root, depth) {
        if (depth > 3) return;
        root.querySelectorAll('*').forEach(el => {
            if (el.shadowRoot) {
                scan(el.shadowRoot.textContent || '', 'shadow-dom-L' + depth);
                el.shadowRoot.querySelectorAll('*').forEach(sel => {
                    for (const attr of sel.attributes) {
                        if (attr.name.startsWith('data-') && CODE_RE.test(attr.value)) {
                            const m = attr.value.match(CODE_RE);
                            if (m) m.forEach(c => { if (!SKIP.has(c)) add(c, 'shadow-data-attr'); });
                        }
                    }
                });
                walkShadow(el.shadowRoot, depth + 1);
            }
        });
    }
    walkShadow(document, 0);

    // 7. Iframes
    document.querySelectorAll('iframe').forEach((f, i) => {
        try { scan(f.contentDocument.body.innerText || '', 'iframe-' + i); } catch(e) {}
    });

    // 8. Hidden elements
    document.querySelectorAll('[hidden],[style*="display:none"],[style*="display: none"],[style*="visibility:hidden"]').forEach(el => {
        scan(el.textContent || '', 'hidden-el');
    });

    // 9. Base64 in visible text
    const b64matches = document.body.innerText.match(/[A-Za-z0-9+/]{4,}={0,2}/g) || [];
    for (const b of b64matches) {
        try {
            const decoded = atob(b);
            if (/^[A-Z0-9]{6}$/.test(decoded) && !SKIP.has(decoded)) add(decoded, 'base64');
        } catch(e) {}
    }

    // 10. HTML comments
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_COMMENT);
    let node;
    while (node = walker.nextNode()) scan(node.textContent, 'html-comment');

    // 11. CSS ::before/::after (sample first 200 elements)
    const els = document.querySelectorAll('*');
    for (let i = 0; i < Math.min(els.length, 200); i++) {
        const before = getComputedStyle(els[i], '::before').content;
        const after = getComputedStyle(els[i], '::after').content;
        if (before && before !== 'none' && before !== 'normal') scan(before.replace(/"/g, ''), 'css-before');
        if (after && after !== 'none' && after !== 'normal') scan(after.replace(/"/g, ''), 'css-after');
    }

    // 12. Visible text — scan ALL lines for 6-char codes
    const visibleText = document.body.innerText;
    const lines = visibleText.split('\n');
    for (let i = 0; i < lines.length; i++) {
        const line = lines[i].trim();
        // Scan every line for codes
        scan(line, 'visible-text');
    }

    // Also get challenge description for LLM context
    const h2 = document.querySelector('h2');
    const desc = h2 ? h2.textContent : '';
    const stepMatch = visibleText.match(/Step (\d+) of 30/);
    const step = stepMatch ? stepMatch[1] : '?';

    // Get first meaningful paragraph
    const hint = visibleText.split('\n').filter(l => l.length > 20 && !/^(Next|Continue|Click|Go |Keep|Proceed|Advance|Move|Submit)/.test(l)).slice(0, 3).join('\n');

    return JSON.stringify({step, desc, hint: hint.substring(0, 300), codes: found});
})()
"#;

const SUBMIT_JS: &str = r#"(() => {
    const code = __CODE__;

    // Dismiss popups (hide, don't remove — removing destroys React tree)
    document.querySelectorAll('*').forEach(e => {
        const s = getComputedStyle(e);
        if ((s.position === 'fixed' || s.position === 'absolute') && parseInt(s.zIndex) > 999 && e.offsetHeight > 50 && (e.querySelector('button') || e.querySelector('[class*="close"]')))
            e.style.display = 'none';
    });

    // Ensure interaction token exists in sessionStorage (required for validation)
    const stepMatch = document.body.innerText.match(/Step (\d+) of 30/);
    if (stepMatch) {
        const step = stepMatch[1];
        const key = 'challenge_interaction_step_' + step;
        if (!sessionStorage.getItem(key)) {
            sessionStorage.setItem(key, JSON.stringify({token: crypto.randomUUID(), interactionType: 'agent', completedAt: Date.now()}));
        }
    }

    // Find input
    const input = document.querySelector('input[placeholder*="code" i], input[placeholder*="enter" i], input[type="text"]:not([hidden])');
    if (!input) return JSON.stringify({ok: false, error: 'no input found'});

    // Fill using native setter to trigger React state
    const nativeSetter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;
    nativeSetter.call(input, code);
    input.dispatchEvent(new Event('input', {bubbles: true}));
    input.dispatchEvent(new Event('change', {bubbles: true}));

    // Click Submit Code button
    const submitBtn = Array.from(document.querySelectorAll('button')).find(b => b.textContent.trim() === 'Submit Code');
    if (submitBtn) submitBtn.click();

    return JSON.stringify({ok: true, filled: code, clicked: submitBtn ? 'Submit Code' : 'none'});
})()
"#;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let api_key = std::env::var("ANTHROPIC_API_KEY").expect("Set ANTHROPIC_API_KEY env var");

    let http = Client::new();

    let browser = Browser::launch().await?;
    let page = browser.new_page("about:blank").await?;
    let mut agent = AgentPage::new(&page);

    let mut messages: Vec<Value> = vec![json!({
        "role": "user",
        "content": "Navigate to https://serene-frangipane-7fd25b.netlify.app/ and solve all 30 steps. Click START, then for each step: scan_for_code, submit_code_and_next. If scan returns empty, investigate and retry."
    })];

    for turn in 0..MAX_TURNS {
        println!("\n--- Turn {} ---", turn);

        let body = json!({
            "model": MODEL,
            "max_tokens": 2048,
            "system": SYSTEM_PROMPT,
            "tools": tool_definitions(),
            "messages": messages,
        });

        let resp_json = call_api_with_retry(&http, &api_key, &body).await?;

        if let Some(err) = resp_json.get("error") {
            eprintln!("API error: {}", err);
            break;
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
            // Don't stop — inject a continuation message
            println!("  (end_turn — injecting continuation)");
            messages.push(json!({
                "role": "user",
                "content": "Keep going. Do not stop. Call scan_for_code for the current step."
            }));
            continue;
        }

        let tool_uses: Vec<&Value> = content.iter().filter(|b| b["type"] == "tool_use").collect();
        if tool_uses.is_empty() {
            println!("No tool calls, stopping.");
            break;
        }

        let mut tool_results = Vec::new();

        for tool_use in &tool_uses {
            let name = tool_use["name"].as_str().unwrap_or("");
            let id = tool_use["id"].as_str().unwrap_or("");
            let input = &tool_use["input"];

            println!(
                "  Tool: {}({})",
                name,
                serde_json::to_string(input).unwrap_or_default()
            );

            let result = execute_tool(&mut agent, name, input).await;
            let (text_result, is_error) = match result {
                Ok(r) => (r, false),
                Err(e) => (format!("Error: {}", e), true),
            };

            // Truncate
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

        if tool_uses.iter().any(|t| t["name"] == "done") {
            println!("Agent signaled done.");
            break;
        }

        messages.push(json!({ "role": "user", "content": tool_results }));

        // Trim conversation — keep first message + last 30 messages
        if messages.len() > 40 {
            let first = messages[0].clone();
            let keep_from = messages.len() - 30;
            let tail: Vec<Value> = messages.drain(1..).skip(keep_from - 1).collect();
            messages = vec![first];
            messages.extend(tail);
        }
    }

    println!("\nAgent loop finished.");
    agent.wait(3000).await;
    browser.close().await?;
    Ok(())
}

async fn call_api_with_retry(http: &Client, api_key: &str, body: &Value) -> anyhow::Result<Value> {
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

        return Ok(json);
    }
    anyhow::bail!("Rate limited after 10 retries")
}

async fn execute_tool(
    agent: &mut AgentPage<'_>,
    name: &str,
    input: &Value,
) -> anyhow::Result<String> {
    match name {
        "scan_for_code" => {
            // Dismiss popups FIRST, then scroll to top so we see the challenge
            let _ = agent.exec(r#"
                document.querySelectorAll('[class*=modal],[class*=popup],[class*=overlay],[class*=cookie],[class*=consent],[class*=notice]').forEach(e => {
                    if (e.offsetHeight > 50) e.style.display = 'none';
                });
            "#).await;
            agent.wait(200).await;
            let _ = agent.exec("window.scrollTo(0, 0)").await;
            agent.wait(200).await;
            let result: String = agent.page().evaluate(SCAN_JS).await?;
            Ok(result)
        }
        "submit_code_and_next" => {
            let code = input["code"].as_str().unwrap_or("");

            // Dismiss popups first
            let _ = agent.exec(r#"
                document.querySelectorAll('[class*=modal],[class*=popup],[class*=overlay],[class*=cookie],[class*=consent],[class*=notice]').forEach(e => {
                    if (e.offsetHeight > 50) e.style.display = 'none';
                });
            "#).await;
            agent.wait(200).await;

            // Scroll the input into view first
            let _ = agent.exec(r#"
                const input = document.querySelector('input[placeholder*="code" i], input[placeholder*="enter" i], input[type="text"]:not([hidden])');
                if (input) input.scrollIntoView({behavior: 'instant', block: 'center'});
            "#).await;
            agent.wait(200).await;

            // Fill and submit
            let js = SUBMIT_JS.replace("__CODE__", &serde_json::to_string(code).unwrap());
            let submit_result: String = agent.page().evaluate(&js).await?;
            println!("    submit: {}", submit_result);
            // Wait for React to process submit and navigate (500ms internal delay + buffer)
            agent.wait(1500).await;

            let url = agent.url().await?;
            let step_check: String = agent.page().evaluate(
                "(() => { const m = document.body.innerText.match(/Step (\\d+) of 30/); return m ? m[1] : '?'; })()"
            ).await.unwrap_or_else(|_| "?".into());
            println!("    now on step: {}, url: {}", step_check, url);

            Ok(format!(
                "Submitted '{}'. Now on step {} at {}. Call scan_for_code to continue.",
                code, step_check, url
            ))
        }
        "navigate" => {
            let url = input["url"].as_str().unwrap_or("about:blank");
            agent.goto(url).await?;
            agent.wait(1500).await;
            let u = agent.url().await?;
            Ok(format!("At: {}", u))
        }
        "observe" => {
            agent.observe().await?;
            let list = agent.element_list();
            // Truncate to first 30 elements
            let lines: Vec<&str> = list.lines().take(30).collect();
            let truncated = lines.join("\n");
            let total = agent.len();
            Ok(if truncated.is_empty() {
                "No elements.".into()
            } else if total > 30 {
                format!("{}\n[...{} total elements]", truncated, total)
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
            // Dispatch scroll event so React components detect it
            let _ = agent
                .exec("window.dispatchEvent(new Event('scroll'))")
                .await;
            agent.wait(200).await;
            let scroll_y: String = agent
                .page()
                .evaluate("(() => { return String(Math.round(window.scrollY)); })()")
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
            Ok(text.chars().take(1500).collect())
        }
        "screenshot" => {
            let png = agent.screenshot().await?;
            let _b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &png);
            Ok(format!(
                "[Screenshot: {} bytes, {} elements]",
                png.len(),
                agent.len()
            ))
        }
        "wait" => {
            let ms = input["ms"].as_u64().unwrap_or(1000);
            agent.wait(ms).await;
            Ok(format!("Waited {}ms", ms))
        }
        "done" => Ok(format!("Done: {}", input["reason"].as_str().unwrap_or(""))),
        _ => Err(anyhow::anyhow!("Unknown tool: {}", name)),
    }
}
