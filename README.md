# eoka-agent

AI agent interaction layer for [eoka](../eoka) browser automation.

Wraps an eoka `Page` with numbered interactive elements, annotated screenshots, and index-based actions. Designed for minimal token usage when driven by an LLM.

## Core loop

```rust
use eoka::Browser;
use eoka_agent::AgentPage;

let browser = Browser::launch().await?;
let page = browser.new_page("https://example.com").await?;
let mut agent = AgentPage::new(&page);

// Observe → act by index → repeat
agent.observe().await?;
println!("{}", agent.element_list());
agent.click(0).await?;
```

## Features

- **observe()** — enumerates all interactive elements (links, buttons, inputs, selects, etc.) with Shadow DOM support
- **element_list()** — compact text format for LLM consumption: `[0] <button> "Submit"`
- **observe_diff()** — returns only what changed since last observation (saves tokens in multi-step sessions)
- **screenshot()** — annotated PNG with numbered red boxes on each element
- **Index-based actions** — `click(i)`, `fill(i, text)`, `select(i, value)`, `hover(i)`, `scroll_to(i)`, `submit(i)`
- **Human-like variants** — `human_click(i)`, `human_fill(i, text)` for stealth
- **Navigation** — `goto(url)`, `back()`, `forward()`, `reload()`
- **Scrolling** — `scroll_down()`, `scroll_up()`, `scroll_to_top()`, `scroll_to_bottom()`
- **Extraction** — `extract(js)` for structured data, `text()` for visible page text
- **Waiting** — `wait_for_text()`, `wait_for_url()`, `wait_for_idle()`

## Element list format

```
[0] <input type="text"> placeholder="Customer name"
[1] <input type="tel"> placeholder="Telephone"
[2] <input type="email"> placeholder="E-mail address"
[3] <input type="radio"> "Small" [checked]
[4] <input type="radio"> "Medium"
[5] <input type="checkbox"> "Bacon"
[6] <button> "Submit"
```

## Example

```sh
cargo run --example demo
```

Fills out a form on httpbin.org, submits it, then extracts top HN titles. Saves an annotated screenshot to `demo_form.png`.
