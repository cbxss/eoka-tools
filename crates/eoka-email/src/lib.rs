use chrono::{Duration, Utc};
use mailparse::MailHeaderMap;
use regex::Regex;

#[derive(Debug, Clone)]
pub struct ImapConfig {
    pub host: String,
    pub port: u16,
    pub tls: bool,
    pub username: String,
    pub password: String,
    pub mailbox: String,
}

impl ImapConfig {
    pub fn new(
        host: impl Into<String>,
        port: u16,
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        Self {
            host: host.into(),
            port,
            tls: true,
            username: username.into(),
            password: password.into(),
            mailbox: "INBOX".into(),
        }
    }

    pub fn mailbox(mut self, mailbox: impl Into<String>) -> Self {
        self.mailbox = mailbox.into();
        self
    }

    pub fn tls(mut self, tls: bool) -> Self {
        self.tls = tls;
        self
    }
}

#[derive(Debug, Clone, Default)]
pub struct SearchCriteria {
    pub from: Option<String>,
    pub subject_contains: Option<String>,
    pub unseen_only: bool,
    pub since_minutes: Option<i64>,
    pub mark_seen: bool,
}

impl SearchCriteria {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from(mut self, v: impl Into<String>) -> Self {
        self.from = Some(v.into());
        self
    }

    pub fn subject_contains(mut self, v: impl Into<String>) -> Self {
        self.subject_contains = Some(v.into());
        self
    }

    pub fn unseen_only(mut self, v: bool) -> Self {
        self.unseen_only = v;
        self
    }

    pub fn since_minutes(mut self, v: i64) -> Self {
        self.since_minutes = Some(v);
        self
    }

    pub fn mark_seen(mut self, v: bool) -> Self {
        self.mark_seen = v;
        self
    }
}

#[derive(Debug, Clone)]
pub struct WaitOptions {
    pub timeout: Duration,
    pub poll_interval: Duration,
}

impl WaitOptions {
    pub fn new(timeout: Duration, poll_interval: Duration) -> Self {
        Self {
            timeout,
            poll_interval,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EmailMessage {
    pub uid: u32,
    pub subject: Option<String>,
    pub from: Option<String>,
    pub date: Option<String>,
    pub body_text: Option<String>,
    pub body_html: Option<String>,
    pub raw: Vec<u8>,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("IMAP error: {0}")]
    Imap(#[from] imap::Error),
    #[error("TLS error: {0}")]
    Tls(#[from] native_tls::Error),
    #[error("Parse error: {0}")]
    Parse(#[from] mailparse::MailParseError),
    #[error("Timeout waiting for email")]
    Timeout,
    #[error("No message found")]
    NotFound,
    #[cfg(feature = "async")]
    #[error("Join error: {0}")]
    Join(String),
}

pub type Result<T> = std::result::Result<T, Error>;

pub struct ImapClient {
    session: imap::Session<imap::Connection>,
}

impl Drop for ImapClient {
    fn drop(&mut self) {
        let _ = self.session.logout();
    }
}

impl ImapClient {
    pub fn connect(config: &ImapConfig) -> Result<Self> {
        let mut builder = imap::ClientBuilder::new(&config.host, config.port);
        if config.tls {
            builder = builder.mode(imap::ConnectionMode::AutoTls);
        } else {
            builder = builder.mode(imap::ConnectionMode::Plaintext);
        }

        let client = builder.connect()?;

        let mut session = client
            .login(&config.username, &config.password)
            .map_err(|e| e.0)?;

        session.select(&config.mailbox)?;

        Ok(Self { session })
    }

    pub fn wait_for_message(
        &mut self,
        criteria: &SearchCriteria,
        options: &WaitOptions,
    ) -> Result<EmailMessage> {
        let start = Utc::now();
        let deadline = start + options.timeout;

        loop {
            if Utc::now() > deadline {
                return Err(Error::Timeout);
            }

            if let Some(msg) = self.fetch_latest(criteria)? {
                return Ok(msg);
            }

            std::thread::sleep(options.poll_interval.to_std().unwrap_or_default());
        }
    }

    pub fn fetch_latest(&mut self, criteria: &SearchCriteria) -> Result<Option<EmailMessage>> {
        let query = build_search_query(criteria);
        let uids = self.session.uid_search(query)?;
        let uid = match uids.iter().max() {
            Some(u) => *u,
            None => return Ok(None),
        };

        let fetches = self.session.uid_fetch(uid.to_string(), "RFC822")?;
        let fetch = fetches.iter().next().ok_or(Error::NotFound)?;
        let raw = fetch.body().ok_or(Error::NotFound)?.to_vec();

        if criteria.mark_seen {
            let _ = self.session.uid_store(uid.to_string(), "+FLAGS (\\Seen)");
        }

        Ok(Some(parse_message(uid, raw)?))
    }
}

fn build_search_query(criteria: &SearchCriteria) -> String {
    let mut parts: Vec<String> = Vec::new();

    if criteria.unseen_only {
        parts.push("UNSEEN".into());
    }

    if let Some(ref from) = criteria.from {
        parts.push(format!("FROM \"{}\"", escape_imap(from)));
    }

    if let Some(ref subject) = criteria.subject_contains {
        parts.push(format!("SUBJECT \"{}\"", escape_imap(subject)));
    }

    if let Some(minutes) = criteria.since_minutes {
        let since = Utc::now() - Duration::minutes(minutes);
        let date = since.format("%d-%b-%Y").to_string();
        parts.push(format!("SINCE {}", date));
    }

    if parts.is_empty() {
        "ALL".to_string()
    } else {
        parts.join(" ")
    }
}

fn escape_imap(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control())
        .flat_map(|c| match c {
            '\\' => vec!['\\', '\\'],
            '"' => vec!['\\', '"'],
            other => vec![other],
        })
        .collect()
}

fn parse_message(uid: u32, raw: Vec<u8>) -> Result<EmailMessage> {
    let parsed = mailparse::parse_mail(&raw)?;

    let headers = parsed.get_headers();
    let subject = headers.get_first_value("Subject");
    let from = headers.get_first_value("From");
    let date = headers.get_first_value("Date");

    let mut body_text: Option<String> = None;
    let mut body_html: Option<String> = None;

    if parsed.subparts.is_empty() {
        let ct = parsed.ctype.mimetype.to_lowercase();
        let body = parsed.get_body()?;
        if ct == "text/html" {
            body_html = Some(body);
        } else {
            body_text = Some(body);
        }
    } else {
        for part in parsed.subparts.iter() {
            let ct = part.ctype.mimetype.to_lowercase();
            if ct == "text/plain" && body_text.is_none() {
                body_text = Some(part.get_body()?);
            } else if ct == "text/html" && body_html.is_none() {
                body_html = Some(part.get_body()?);
            }
        }
    }

    Ok(EmailMessage {
        uid,
        subject,
        from,
        date,
        body_text,
        body_html,
        raw,
    })
}

#[derive(Debug, Clone, Default)]
pub struct LinkFilter {
    pub allow_domains: Option<Vec<String>>,
}

pub fn extract_first_link(msg: &EmailMessage, filter: &LinkFilter) -> Option<String> {
    let hay = msg
        .body_html
        .as_deref()
        .or(msg.body_text.as_deref())?;

    let re = Regex::new(r#"https?://[^\s"'<>)]+"#).ok()?;
    for m in re.find_iter(hay) {
        let link = m.as_str().trim_end_matches(['.', ',', ';', ':', '!', '?']);
        if link_allowed(link, filter) {
            return Some(link.to_string());
        }
    }
    None
}

fn link_allowed(link: &str, filter: &LinkFilter) -> bool {
    let allow = match filter.allow_domains.as_ref() {
        Some(v) if !v.is_empty() => v,
        _ => return true,
    };

    if let Ok(url) = url::Url::parse(link) {
        if let Some(host) = url.host_str() {
            return allow.iter().any(|d| host.ends_with(d));
        }
    }

    false
}

pub fn extract_code(msg: &EmailMessage, regex: &Regex) -> Option<String> {
    let hay = msg
        .body_text
        .as_deref()
        .or(msg.body_html.as_deref())?;

    regex
        .captures(hay)
        .and_then(|c| c.get(1).or_else(|| c.get(0)))
        .map(|m| m.as_str().to_string())
}

#[cfg(feature = "async")]
pub mod async_client {
    use super::*;
    use std::sync::{Arc, Mutex};

    pub struct AsyncImapClient {
        inner: Arc<Mutex<ImapClient>>,
    }

    impl AsyncImapClient {
        pub async fn connect(config: &ImapConfig) -> Result<Self> {
            let cfg = config.clone();
            let client = tokio::task::spawn_blocking(move || ImapClient::connect(&cfg))
                .await
                .map_err(|e| Error::Join(e.to_string()))??;
            Ok(Self {
                inner: Arc::new(Mutex::new(client)),
            })
        }

        /// Poll for a matching message with async sleep between attempts.
        /// Unlike the sync version, this releases the mutex between polls.
        pub async fn wait_for_message(
            &mut self,
            criteria: &SearchCriteria,
            options: &WaitOptions,
        ) -> Result<EmailMessage> {
            let deadline = Utc::now() + options.timeout;

            loop {
                if Utc::now() > deadline {
                    return Err(Error::Timeout);
                }

                if let Some(msg) = self.fetch_latest(criteria).await? {
                    return Ok(msg);
                }

                let sleep_ms = options
                    .poll_interval
                    .num_milliseconds()
                    .max(100) as u64;
                tokio::time::sleep(std::time::Duration::from_millis(sleep_ms)).await;
            }
        }

        pub async fn fetch_latest(
            &mut self,
            criteria: &SearchCriteria,
        ) -> Result<Option<EmailMessage>> {
            let criteria = criteria.clone();
            let inner = self.inner.clone();
            tokio::task::spawn_blocking(move || {
                let mut guard = inner.lock().unwrap();
                guard.fetch_latest(&criteria)
            })
            .await
            .map_err(|e| Error::Join(e.to_string()))?
        }
    }
}

#[cfg(feature = "async")]
pub use async_client::AsyncImapClient;

#[cfg(test)]
mod tests {
    use super::*;

    fn make_msg(body_text: Option<&str>, body_html: Option<&str>) -> EmailMessage {
        EmailMessage {
            uid: 1,
            subject: Some("Test".into()),
            from: Some("sender@example.com".into()),
            date: Some("Mon, 1 Jan 2024 00:00:00 +0000".into()),
            body_text: body_text.map(String::from),
            body_html: body_html.map(String::from),
            raw: Vec::new(),
        }
    }

    // --- extract_first_link ---

    #[test]
    fn extract_link_from_html() {
        let msg = make_msg(None, Some(r#"<a href="https://example.com/verify?t=abc">Click</a>"#));
        let link = extract_first_link(&msg, &LinkFilter::default()).unwrap();
        assert_eq!(link, "https://example.com/verify?t=abc");
    }

    #[test]
    fn extract_link_from_text_fallback() {
        let msg = make_msg(Some("Visit https://example.com/link here"), None);
        let link = extract_first_link(&msg, &LinkFilter::default()).unwrap();
        assert_eq!(link, "https://example.com/link");
    }

    #[test]
    fn extract_link_trims_trailing_punctuation() {
        let msg = make_msg(Some("Go to https://example.com/page."), None);
        let link = extract_first_link(&msg, &LinkFilter::default()).unwrap();
        assert_eq!(link, "https://example.com/page");
    }

    #[test]
    fn extract_link_domain_filter_allows() {
        let msg = make_msg(Some("https://allowed.com/ok https://blocked.com/no"), None);
        let filter = LinkFilter {
            allow_domains: Some(vec!["allowed.com".into()]),
        };
        let link = extract_first_link(&msg, &filter).unwrap();
        assert_eq!(link, "https://allowed.com/ok");
    }

    #[test]
    fn extract_link_domain_filter_blocks() {
        let msg = make_msg(Some("https://blocked.com/no"), None);
        let filter = LinkFilter {
            allow_domains: Some(vec!["allowed.com".into()]),
        };
        assert!(extract_first_link(&msg, &filter).is_none());
    }

    #[test]
    fn extract_link_subdomain_match() {
        let msg = make_msg(Some("https://sub.example.com/verify"), None);
        let filter = LinkFilter {
            allow_domains: Some(vec!["example.com".into()]),
        };
        let link = extract_first_link(&msg, &filter).unwrap();
        assert_eq!(link, "https://sub.example.com/verify");
    }

    #[test]
    fn extract_link_none_when_no_body() {
        let msg = make_msg(None, None);
        assert!(extract_first_link(&msg, &LinkFilter::default()).is_none());
    }

    // --- extract_code ---

    #[test]
    fn extract_6digit_code() {
        let msg = make_msg(Some("Your code is 482913. Please enter it."), None);
        let re = Regex::new(r"(\d{6})").unwrap();
        let code = extract_code(&msg, &re).unwrap();
        assert_eq!(code, "482913");
    }

    #[test]
    fn extract_code_capture_group() {
        let msg = make_msg(Some("Code: ABC-1234"), None);
        let re = Regex::new(r"Code: ([A-Z]+-\d+)").unwrap();
        let code = extract_code(&msg, &re).unwrap();
        assert_eq!(code, "ABC-1234");
    }

    #[test]
    fn extract_code_falls_back_to_group0() {
        let msg = make_msg(Some("token 99887766"), None);
        let re = Regex::new(r"\d{8}").unwrap();
        let code = extract_code(&msg, &re).unwrap();
        assert_eq!(code, "99887766");
    }

    #[test]
    fn extract_code_prefers_text_over_html() {
        let msg = make_msg(Some("text 111111"), Some("html 222222"));
        let re = Regex::new(r"(\d{6})").unwrap();
        let code = extract_code(&msg, &re).unwrap();
        assert_eq!(code, "111111");
    }

    #[test]
    fn extract_code_none_when_no_match() {
        let msg = make_msg(Some("no digits here"), None);
        let re = Regex::new(r"(\d{6})").unwrap();
        assert!(extract_code(&msg, &re).is_none());
    }

    // --- build_search_query ---

    #[test]
    fn search_query_all() {
        let criteria = SearchCriteria::new().unseen_only(false);
        assert_eq!(build_search_query(&criteria), "ALL");
    }

    #[test]
    fn search_query_unseen() {
        let criteria = SearchCriteria::new().unseen_only(true);
        assert_eq!(build_search_query(&criteria), "UNSEEN");
    }

    #[test]
    fn search_query_combined() {
        let criteria = SearchCriteria::new()
            .unseen_only(true)
            .from("noreply@test.com")
            .subject_contains("Verify");
        let q = build_search_query(&criteria);
        assert!(q.contains("UNSEEN"));
        assert!(q.contains(r#"FROM "noreply@test.com""#));
        assert!(q.contains(r#"SUBJECT "Verify""#));
    }

    #[test]
    fn search_query_since() {
        let criteria = SearchCriteria::new().unseen_only(false).since_minutes(10);
        let q = build_search_query(&criteria);
        assert!(q.starts_with("SINCE "));
    }

    // --- escape_imap ---

    #[test]
    fn escape_imap_quotes_and_backslash() {
        assert_eq!(escape_imap(r#"test"val"#), r#"test\"val"#);
        assert_eq!(escape_imap(r"back\slash"), r"back\\slash");
    }

    #[test]
    fn escape_imap_strips_control_chars() {
        assert_eq!(escape_imap("hello\x00world\nok"), "helloworldok");
    }

    // --- parse_message ---

    #[test]
    fn parse_plain_text_message() {
        let raw = b"From: sender@test.com\r\nSubject: Hello\r\nContent-Type: text/plain\r\n\r\nBody text here";
        let msg = parse_message(42, raw.to_vec()).unwrap();
        assert_eq!(msg.uid, 42);
        assert_eq!(msg.subject.as_deref(), Some("Hello"));
        assert_eq!(msg.from.as_deref(), Some("sender@test.com"));
        assert!(msg.body_text.as_ref().unwrap().contains("Body text here"));
        assert!(msg.body_html.is_none());
    }

    #[test]
    fn parse_html_message() {
        let raw = b"Subject: Hi\r\nContent-Type: text/html\r\n\r\n<b>bold</b>";
        let msg = parse_message(1, raw.to_vec()).unwrap();
        assert!(msg.body_html.as_ref().unwrap().contains("<b>bold</b>"));
        assert!(msg.body_text.is_none());
    }
}
