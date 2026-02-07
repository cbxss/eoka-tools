# eoka-email

IMAP helpers for email-based automation (OTP codes, verification links).

## Install (workspace)

Add to your crate's `Cargo.toml`:

```toml
[dependencies]
eoka-email = { path = "../eoka-tools/crates/eoka-email" }
eoka = { path = "../eoka" }
regex = "1" # for code extraction patterns
```

## Example: verification link + eoka

```rust
use chrono::Duration;
use eoka::Browser;
use eoka_email::{
    extract_first_link, ImapClient, ImapConfig, LinkFilter, SearchCriteria, WaitOptions,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Launch browser
    let browser = Browser::launch().await?;
    let page = browser.new_page("https://example.com/signup").await?;

    // ... fill/signup steps here ...

    // Wait for the email
    let config = ImapConfig::new("imap.gmail.com", 993, "user@gmail.com", "app-password")
        .mailbox("INBOX")
        .tls(true);

    let criteria = SearchCriteria::new()
        .from("no-reply@example.com")
        .subject_contains("Confirm your email")
        .unseen_only(true)
        .since_minutes(10);

    let options = WaitOptions::new(Duration::minutes(2), Duration::seconds(2));

    let mut client = ImapClient::connect(&config)?;
    let msg = client.wait_for_message(&criteria, &options)?;

    let link = extract_first_link(&msg, &LinkFilter {
        allow_domains: Some(vec!["example.com".into()]),
    })
    .expect("no link found");

    page.goto(&link).await?;
    Ok(())
}
```

## Example: OTP code + eoka

```rust
use chrono::Duration;
use eoka::Browser;
use eoka_email::{extract_code, ImapClient, ImapConfig, SearchCriteria, WaitOptions};
use regex::Regex;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let browser = Browser::launch().await?;
    let page = browser.new_page("https://example.com/login").await?;

    // ... login steps ...

    let config = ImapConfig::new("imap.gmail.com", 993, "user@gmail.com", "app-password");
    let criteria = SearchCriteria::new()
        .subject_contains("Your verification code")
        .unseen_only(true)
        .since_minutes(10);
    let options = WaitOptions::new(Duration::minutes(2), Duration::seconds(2));

    let mut client = ImapClient::connect(&config)?;
    let msg = client.wait_for_message(&criteria, &options)?;

    let code = extract_code(&msg, &Regex::new(r"(\d{6})").unwrap())
        .expect("no code found");

    page.fill("input[name=code]", &code).await?;
    Ok(())
}
```

## Async usage (Tokio)

Enable the async feature:

```toml
[dependencies]
eoka-email = { path = "../eoka-tools/crates/eoka-email", features = ["async"] }
```

```rust
use chrono::Duration;
use eoka_email::{AsyncImapClient, ImapConfig, SearchCriteria, WaitOptions};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = ImapConfig::new("imap.gmail.com", 993, "user@gmail.com", "app-password");
    let criteria = SearchCriteria::new()
        .subject_contains("Your verification code")
        .unseen_only(true)
        .since_minutes(10);
    let options = WaitOptions::new(Duration::minutes(2), Duration::seconds(2));

    let mut client = AsyncImapClient::connect(&config).await?;
    let msg = client.wait_for_message(&criteria, &options).await?;
    println!("Subject: {:?}", msg.subject);
    Ok(())
}
```

## Notes

- Use app passwords when IMAP access is required (Gmail/Outlook).
- `SearchCriteria::mark_seen(true)` will set `\\Seen` after fetching.
- Filtering by `since_minutes` is recommended to avoid picking older emails.
