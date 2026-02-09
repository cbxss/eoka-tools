# Anti-Captcha Integration for eoka-agent

Automatically solve hCaptcha and reCAPTCHA using anti-captcha.com API.

## Setup

### 1. Get Anti-Captcha API Key

1. Sign up at https://anti-captcha.com/
2. Get your API key from dashboard
3. Add credits (pay-as-you-go, typically $0.30-0.50 per captcha solve)

### 2. Build eoka-agent with Captcha Support

```bash
cd /home/cbass/Code/eoka-tools
cargo build -p eoka-agent
```

### 3. Use the New Tools

Three new tools are now available:
- **solve_captcha** — Solve hCaptcha or reCAPTCHA
- **detect_captcha** — Detect captchas on page
- **inject_captcha_token** — Inject solved token back into page

---

## Usage Examples

### Example 1: Detect and Solve hCaptcha

```python
# Step 1: Navigate to page with captcha
mcp__eoka-agent__navigate(url: "https://opencorporates.com/search?q=Southern+Trust")

# Step 2: Detect captcha on page
mcp__eoka-agent__detect_captcha(auto_detect: true)
# Returns: {"captcha_type": "hcaptcha", "sitekey": "1234567890abcdef"}

# Step 3: Solve it
mcp__eoka-agent__solve_captcha(
    api_key: "YOUR_ANTI_CAPTCHA_API_KEY",
    captcha_type: "hcaptcha",
    website_url: "https://opencorporates.com/search?q=Southern+Trust",
    website_key: "1234567890abcdef"
)
# Returns: Solved token

# Step 4: Inject token and submit
mcp__eoka-agent__inject_captcha_token(
    js: """
    // For hCaptcha v2
    document.querySelector('[name="h-captcha-response"]').value = 'SOLVED_TOKEN_HERE';
    document.querySelector('form').submit();
    """
)
```

### Example 2: Auto-Detect and Solve reCAPTCHA v2

```python
mcp__eoka-agent__navigate(url: "https://example.com/form")

# Auto-detect
response = mcp__eoka-agent__detect_captcha(auto_detect: true)
# If reCAPTCHA detected:

mcp__eoka-agent__solve_captcha(
    api_key: "YOUR_API_KEY",
    captcha_type: "recaptcha_v2",
    website_url: "https://example.com/form",
    website_key: "6Ld_lskaoiwjd..._abc"
)

# Inject and continue
mcp__eoka-agent__inject_captcha_token(
    js: """
    document.querySelector('textarea[name="g-recaptcha-response"]').innerHTML = 'SOLVED_TOKEN_HERE';
    // Manually click submit or trigger form submission
    document.querySelector('button[type="submit"]').click();
    """
)
```

### Example 3: reCAPTCHA v3 (Invisible)

```python
mcp__eoka-agent__solve_captcha(
    api_key: "YOUR_API_KEY",
    captcha_type: "recaptcha_v3",
    website_url: "https://example.com/form",
    website_key: "6Ld_lskaoiwjd..._abc",
    page_action: "submit",  # or "login", "homepage", etc
    min_score: 0.3
)
```

---

## Tool Reference

### solve_captcha

Solve a CAPTCHA using anti-captcha.com API.

**Parameters:**
- `api_key` (required) — Your anti-captcha.com API key
- `captcha_type` (required) — One of: `hcaptcha`, `recaptcha_v2`, `recaptcha_v3`
- `website_url` (required) — Full URL of the page with captcha
- `website_key` (required) — Sitekey from the captcha
- `page_action` (optional) — For reCAPTCHA v3, the action name
- `min_score` (optional) — For reCAPTCHA v3, minimum score (default 0.3)

**Returns:**
- Success: Solved captcha token (can be 100+ characters)
- Error: Error message with details

**Example:**
```
solve_captcha(
    api_key: "abc123def456",
    captcha_type: "hcaptcha",
    website_url: "https://opencorporates.com/search",
    website_key: "10000000-ffff-ffff-ffff-000000000001"
)
```

### detect_captcha

Auto-detect hCaptcha or reCAPTCHA on the current page.

**Parameters:**
- `auto_detect` (optional) — Enable/disable auto-detection (default true)

**Returns:**
- If captcha found: `{ "type": "hcaptcha" or "recaptcha", "sitekey": "..." }`
- If no captcha: `"No captcha detected on current page"`

**Example:**
```
detect_captcha(auto_detect: true)
```

### inject_captcha_token

Execute custom JavaScript to inject a solved captcha token into the page.

**Parameters:**
- `js` (required) — JavaScript code to execute

**Common injection patterns:**

For hCaptcha:
```javascript
document.querySelector('[name="h-captcha-response"]').value = 'TOKEN_HERE';
document.querySelector('form').submit();
```

For reCAPTCHA v2:
```javascript
document.querySelector('textarea[name="g-recaptcha-response"]').innerHTML = 'TOKEN_HERE';
document.querySelector('button[type="submit"]').click();
```

For reCAPTCHA v3:
```javascript
// Often the page handles it automatically, but you can trigger:
document.querySelector('form').submit();
```

---

## Workflow: OpenCorporates Entity Lookup with Captcha Bypass

```
1. Navigate to OpenCorporates search
   → Hits hCaptcha wall

2. Detect captcha
   → Get sitekey

3. Solve with anti-captcha
   → Get token (takes ~2-5 seconds)

4. Inject token
   → Form submits

5. Parse results
   → Extract entity status
```

### Full Example Script

```python
# Login & navigate
mcp__eoka-agent__navigate(url: "https://opencorporates.com")
# (login flow)

# Search with captcha wall
url = "https://opencorporates.com/search?q=Southern+Trust+Company+Inc&jurisdiction_code=us_vi"
mcp__eoka-agent__navigate(url: url)

# Detect captcha
detected = mcp__eoka-agent__detect_captcha(auto_detect: true)
# Returns: hCaptcha with sitekey

# Solve it
token = mcp__eoka-agent__solve_captcha(
    api_key: "YOUR_KEY",
    captcha_type: "hcaptcha",
    website_url: url,
    website_key: "10000000-ffff-ffff-ffff-000000000001"
)

# Inject & submit
mcp__eoka-agent__inject_captcha_token(
    js: """
    const token = '""" + token + """';
    document.querySelector('[name="h-captcha-response"]').value = token;
    // Usually auto-submits, but if not:
    document.querySelector('form').submit();
    """
)

# Wait for redirect
time.sleep(3)

# Screenshot results
mcp__eoka-agent__screenshot()

# Extract company info
data = mcp__eoka-agent__extract(
    js: """
    Array.from(document.querySelectorAll('.company-result')).map(el => ({
        name: el.querySelector('h2')?.innerText,
        status: el.querySelector('.status')?.innerText,
        link: el.querySelector('a')?.href
    }))
    """
)
```

---

## Cost Estimation

- hCaptcha solve: ~$0.30-0.50 per solve
- reCAPTCHA v2: ~$0.30-0.50 per solve
- reCAPTCHA v3: ~$0.30-0.50 per solve

For investigating **28 shell companies** on OpenCorporates:
- Budget: $15-20 USD
- Time: ~2-3 minutes with parallel batching

---

## Troubleshooting

### "Captcha solving timeout"
- The anti-captcha service took >5 minutes
- Usually means server is overloaded
- Try again or check anti-captcha.com status

### "Invalid sitekey"
- Sitekey detection failed
- Manual detection: Open browser DevTools, search for `data-sitekey=` or `recaptcha`
- Pass it manually to solve_captcha

### "API key invalid"
- Double-check your anti-captcha.com API key
- Check account has credits remaining

### "Token injection failed"
- JavaScript selector may be wrong
- Different sites use different HTML structures
- Use `mcp__eoka-agent__observe()` to find the right element

---

## Building from Source

The anti-captcha integration is in:
- `crates/eoka-agent/src/captcha.rs` — Core solver logic
- `crates/eoka-agent/src/mcp.rs` — Tool definitions

To rebuild:
```bash
cd /home/cbass/Code/eoka-tools
cargo build -p eoka-agent --release
```

Binary location: `target/release/eoka-agent`

---

## Next: Run the OpenCorporates Investigation

Now you can batch search the 28 shell companies on OpenCorporates without getting blocked!

```bash
# Extract entities (if not done)
bash /home/cbass/Code/osint-recon/plugins/osint-recon/skills/osint-recon/extract_entities.sh \
  /home/cbass/Code/the_stein/10-properties-entities.md shell_companies.json

# Use the browser + captcha tools to batch lookup status
```
