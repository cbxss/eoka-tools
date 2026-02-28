# eoka-agent: Post-Refactor Hardening Plan

4 parallel workstreams. Each team member owns one tier end-to-end.

---

## Team 1: Actually Dangerous (fix now)

**Goal:** Eliminate silent data loss and incorrect behavior under failure.

### 1A. Fix error swallowing in `wait_for_stable()`
**File:** `src/mcp/helpers.rs:69-86`

`wait_for_stable()` returns `Ok(())` when the 10s timeout expires — callers think the page is ready when it isn't. This causes flaky observe/click/fill on slow pages.

**Fix:**
```rust
pub(crate) async fn wait_for_stable(page: &Page) -> eoka::Result<()> {
    // ... same polling loop ...
    if start.elapsed() > max_wait {
        return Err(eoka::Error::CdpSimple(
            "Page did not reach interactive state within 10s".into()
        ));
    }
    // ...
}
```

Update callers in `mod.rs` that call `wait_for_stable` — decide per-handler whether to propagate or log-and-continue.

### 1B. Element cache invalidation consistency
**File:** `src/mcp/mod.rs` — navigate, back, forward, spa_navigate handlers

After navigation, element indices go stale. Currently `navigate` clears `tab.elements`, but `back`, `forward`, and `spa_navigate` don't.

**Fix:** Clear `tab.elements = vec![]` after every navigation action. Grep for all handlers that change the page URL and add the clear.

Search: `tab.page.navigate`, `tab.page.back`, `tab.page.forward`, `spa::navigate::spa_navigate`, `spa::navigate::history_go`

### 1C. Transport error false positives
**File:** `src/mcp/error.rs:52-66`

`is_transport_error_msg` matches "connection" as a substring — this fires on legitimate errors like "connection refused by target" or "WebSocket connection to devtools". The needle is too broad.

**Fix:** Tighten needles:
```rust
const NEEDLES: &[&str] = &[
    "websocket error",      // was "websocket"
    "transport error",      // was "transport"
    "timed out",
    "connection closed",    // was "connection" — too broad
    "connection reset",
    "broken pipe",
    "reset by peer",
];
```

Update the existing tests to match new needles. Add negative test: `"connection refused by target"` should NOT be transport error.

### 1D. `wait_ms` silent cap
**File:** `src/mcp/mod.rs` — `wait_ms` handler

Currently caps at 30s silently. If someone asks for 60s they get 30s with no indication.

**Fix:** Return error if > 30s, or include actual capped value in response text:
```rust
let capped = ms.min(30_000);
if capped < ms {
    return text_ok(format!("Waited {}ms (capped from {}ms, max 30s)", capped, ms));
}
```

### Acceptance criteria
- [ ] `wait_for_stable()` returns Err on timeout
- [ ] `back`, `forward`, `spa_navigate`, `history_go` clear element cache
- [ ] `is_transport_error_msg("connection refused")` returns false
- [ ] `wait_ms(60000)` tells the caller it was capped
- [ ] All existing tests still pass, new tests for each fix

---

## Team 2: Architecture Debt

**Goal:** Real error types, kill `lock()` boilerplate, simplify state access.

### 2A. Expand `AgentError` to 6+ variants
**File:** `src/mcp/error.rs`

Current enum has only `Internal` and `InvalidInput`. Add:
```rust
pub(crate) enum AgentError {
    Internal(String),
    InvalidInput(String),
    NoBrowser,              // replaces ERR_NO_BROWSER string
    NoTab,                  // replaces ERR_NO_TAB string
    Transport(String),      // connection lost errors
    ElementNotFound(String), // target resolution failures
    Timeout(String),        // wait_for_stable, wait_for_text, etc.
    NavigationFailed(String),
}
```

Implement `From<AgentError> for ErrorData` with appropriate error codes. Remove `ERR_NO_BROWSER`/`ERR_NO_TAB` constants.

### 2B. Add `lock_browser()` helpers
**File:** `src/mcp/mod.rs` — add to `impl EokaServer` (plain, not `#[tool_router]`)

```rust
async fn lock_browser(&self) -> Result<tokio::sync::MutexGuard<'_, Option<BrowserState>>, ErrorData> {
    let guard = self.state.lock().await;
    if guard.is_none() {
        return Err(AgentError::NoBrowser.into());
    }
    Ok(guard)
}
```

Then replace the ~36 instances of:
```rust
let guard = self.state.lock().await;
let state = guard.as_ref().ok_or_else(|| internal(ERR_NO_BROWSER))?;
```
with:
```rust
let guard = self.lock_browser().await?;
let state = guard.as_ref().unwrap(); // safe — lock_browser checked
```

This saves 1 line per handler and removes the scattered `ok_or_else` calls.

### 2C. Audit dual API surface (Session vs AgentPage)
**File:** `src/lib.rs`

`Session` and `AgentPage` exist as two ways to use the library. `Session` owns the browser, `AgentPage` borrows a page. They have divergent APIs and neither is used by MCP (which has its own `BrowserState`).

**Action:** Add `#[deprecated]` to `Session` with a message pointing to `AgentPage` or MCP. If nothing outside this crate uses `Session`, mark it `pub(crate)` or remove it entirely.

Check: `cargo doc -p eoka-agent` and `grep -r "Session" ../` outside the crate.

### Acceptance criteria
- [ ] AgentError has 6+ variants with From<AgentError> for ErrorData
- [ ] ERR_NO_BROWSER/ERR_NO_TAB constants removed
- [ ] lock_browser() used in all handlers, no more raw lock+ok_or_else
- [ ] Session is deprecated or removed
- [ ] `cargo clippy -p eoka-agent -- -Dwarnings` clean

---

## Team 3: Robustness

**Goal:** Fix subtle correctness issues that cause wrong results silently.

### 3A. Fingerprint truncation bug
**File:** `src/lib.rs` — `compute_fingerprint()`

Currently truncates selector to 50 chars before hashing. Two elements with selectors differing only after char 50 get the same fingerprint. This breaks `ObserveDiff`.

**Fix:** Hash the full selector. Remove the truncation:
```rust
// Before:
let sel_truncated = &selector[..selector.len().min(50)];
// After:
hasher.write(selector.as_bytes());
```

Add test: two elements with selectors identical up to char 50 but different after should have different fingerprints.

### 3B. Graceful Session Drop
**File:** `src/lib.rs`

`Session` owns a `Browser` but has no `Drop` impl. The `Browser::Drop` in eoka kills the process, but doesn't do graceful CDP shutdown (`Browser::close()` is async and can't run in Drop).

**Fix:** Add a `close()` method to Session that calls `browser.close().await`, and document that callers should use it. Or use `tokio::runtime::Handle::current().spawn()` in a custom Drop to fire-and-forget the cleanup. Prefer explicit `close()`.

### 3C. `click_with_retry` only retries 2 error strings
**File:** `src/mcp/helpers.rs:104-125`

Only retries on "not found" or "not visible". Other stale-element errors (e.g., "node not connected", "stale element reference") silently fail.

**Fix:** Add more retry-worthy error substrings:
```rust
Err(e) if is_stale_element_error(&e.to_string()) => { ... }

fn is_stale_element_error(msg: &str) -> bool {
    ["not found", "not visible", "node not connected", "stale element", "detached"]
        .iter().any(|n| msg.contains(n))
}
```

### 3D. `observe` filter validation
**File:** `src/mcp/mod.rs` — observe handler

The `filter` field on ObserveRequest accepts any string. Invalid filters silently return all elements.

**Fix:** Validate against known values ("inputs", "buttons", "links", etc.) and return `InvalidInput` for unknown filters.

### Acceptance criteria
- [ ] Fingerprints differ for elements with long selectors differing after char 50
- [ ] Session has explicit `close()` method
- [ ] click_with_retry retries on all stale-element variants
- [ ] Invalid observe filter returns error
- [ ] Tests for each fix

---

## Team 4: Quality of Life

**Goal:** CI quality gates, test coverage, cleanup.

### 4A. JS linting in CI
**Files:** `src/js/*.js`, CI config

Add ESLint or Biome check for the 6 extracted JS files. Create a minimal config that catches syntax errors and obvious bugs.

```bash
# In CI:
npx biome check crates/eoka-agent/src/js/
```

Add a `biome.json` or `.eslintrc` in `crates/eoka-agent/` scoped to `src/js/`.

### 4B. Test coverage for error paths
**File:** `src/mcp/error.rs`, `src/mcp/helpers.rs`, new test file `src/mcp/tests.rs`

Add unit tests (no Chrome needed):
- `resolve_target` with empty elements + index → error
- `resolve_target` with out-of-range index → error
- `resolve_js` with no file and no js → error
- `resolve_js` with nonexistent file path → error
- `element_list` with empty vec → empty string
- `element_list` with multiple elements → correct format
- `auto_observe_if_needed` with non-index target → no-op (needs mock or skip)

### 4C. ObserveConfig simplification
**File:** `src/lib.rs`

`ObserveConfig` has `viewport_only`, `max_elements`, `filter` — but MCP passes these as separate tool params and constructs the config inline. The struct exists but isn't used consistently.

**Fix:** Either use `ObserveConfig` in MCP handlers (replacing inline params) or remove it from the public API if only MCP uses observe.

### 4D. Captcha module cleanup
**Files:** `src/captcha.rs`, `src/mcp/mod.rs` (captcha handlers)

The captcha module has:
- `solve_captcha` that delegates to external solver
- `detect_captcha` that runs JS detection
- `inject_captcha_token` that injects solutions

Review: Are these tested? Are the JS snippets in captcha.rs also candidates for extraction to `src/js/`? Add basic unit tests for the detection JS parsing.

### Acceptance criteria
- [ ] CI runs JS lint on `src/js/*.js`
- [ ] 8+ new unit tests for error/edge cases
- [ ] ObserveConfig either consistently used or removed from public API
- [ ] Captcha detection JS either extracted or has tests
- [ ] `cargo test -p eoka-agent` passes with 30+ tests (currently 24)

---

## Coordination

- Teams work in parallel on feature branches: `fix/tier1-dangerous`, `fix/tier2-arch`, `fix/tier3-robust`, `fix/tier4-quality`
- Merge order: Tier 1 first (safety), then Tier 2 (most files touched), then 3 and 4 (independent)
- If Tier 2 changes error.rs significantly, Tier 1 should rebase after Tier 2's error.rs changes land
- Recommended: Tier 1 and Tier 2 coordinate on `AgentError` — Tier 1 can use the new variants Tier 2 creates

## Verification (all tiers)

```bash
cargo build -p eoka-agent
cargo test -p eoka-agent
cargo clippy -p eoka-agent -- -Dwarnings
cargo fmt -p eoka-agent -- --check
```
