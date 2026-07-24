//! Network request interception via CDP Fetch domain.

use std::path::PathBuf;

use serde_json::{json, Value};

/// A single intercept rule.
#[derive(Clone)]
pub struct InterceptRule {
    pub id: usize,
    pub url_pattern: String,
    pub capture_path: Option<PathBuf>,
    pub respond_path: Option<PathBuf>,
    pub respond_status: u16,
}

impl InterceptRule {
    pub fn matches_url(&self, url: &str) -> bool {
        url_matches(&self.url_pattern, url)
    }
}

/// Log entry for an intercepted request.
#[derive(Clone)]
pub struct InterceptLogEntry {
    pub rule_id: usize,
    pub url: String,
    pub method: String,
    pub has_body: bool,
    pub action: String,
}

pub struct InterceptState {
    rules: Vec<InterceptRule>,
    log: Vec<InterceptLogEntry>,
    next_id: usize,
    pub enabled: bool,
}

impl InterceptState {
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
            log: Vec::new(),
            next_id: 1,
            enabled: false,
        }
    }

    pub fn add_rule(
        &mut self,
        url_pattern: String,
        capture_path: Option<PathBuf>,
        respond_path: Option<PathBuf>,
        respond_status: u16,
    ) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.rules.push(InterceptRule {
            id,
            url_pattern,
            capture_path,
            respond_path,
            respond_status,
        });
        id
    }

    pub fn remove_rule(&mut self, id: usize) -> bool {
        let len = self.rules.len();
        self.rules.retain(|r| r.id != id);
        self.rules.len() < len
    }

    pub fn remove_all(&mut self) {
        self.rules.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    pub fn fetch_patterns_json(&self) -> Value {
        Value::Array(
            self.rules
                .iter()
                .map(|r| {
                    json!({
                        "urlPattern": r.url_pattern,
                        "requestStage": "Request",
                    })
                })
                .collect(),
        )
    }

    pub fn rules_snapshot(&self) -> Vec<InterceptRule> {
        self.rules.clone()
    }

    pub fn add_log(&mut self, entry: InterceptLogEntry) {
        if self.log.len() > 1000 {
            self.log.drain(0..500);
        }
        self.log.push(entry);
    }

    pub fn clear_log(&mut self) {
        self.log.clear();
    }

    pub fn list_json(&self) -> Value {
        let rules: Vec<Value> = self
            .rules
            .iter()
            .map(|r| {
                json!({
                    "id": r.id,
                    "pattern": r.url_pattern,
                    "capture": r.capture_path.as_ref().map(|p| p.to_string_lossy().to_string()),
                    "respond": r.respond_path.as_ref().map(|p| p.to_string_lossy().to_string()),
                    "status": r.respond_status,
                })
            })
            .collect();
        Value::Array(rules)
    }

    pub fn log_json(&self) -> Value {
        let entries: Vec<Value> = self
            .log
            .iter()
            .map(|e| {
                json!({
                    "rule_id": e.rule_id,
                    "url": e.url,
                    "method": e.method,
                    "has_body": e.has_body,
                    "action": e.action,
                })
            })
            .collect();
        Value::Array(entries)
    }
}

/// Simple glob matching: * matches any sequence of chars.
fn url_matches(pattern: &str, url: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return url.contains(pattern);
    }

    let mut pos = 0;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        match url[pos..].find(part) {
            Some(found) => {
                if i == 0 && found != 0 {
                    return false; // first part must match at start if no leading *
                }
                pos += found + part.len();
            }
            None => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_matching() {
        assert!(url_matches(
            "*/api/data*",
            "https://example.com/api/data?foo=1"
        ));
        assert!(url_matches(
            "*/biometrics/*",
            "https://api.facetec.com/biometrics/process-request"
        ));
        assert!(!url_matches("*/api/data*", "https://example.com/other"));
        assert!(url_matches("example.com", "https://example.com/foo"));
    }

    #[test]
    fn fetch_patterns_include_rule_patterns() {
        let mut state = InterceptState::new();
        state.add_rule("*/api/*".into(), None, None, 200);

        assert_eq!(
            state.fetch_patterns_json(),
            json!([{ "urlPattern": "*/api/*", "requestStage": "Request" }])
        );
    }

    #[test]
    fn fetch_patterns_are_empty_when_no_rules_exist() {
        let state = InterceptState::new();

        assert!(state.is_empty());
        assert_eq!(state.fetch_patterns_json(), json!([]));
    }

    #[test]
    fn fetch_patterns_include_all_active_rules() {
        let mut state = InterceptState::new();
        state.add_rule("*/api/*".into(), None, None, 200);
        state.add_rule("*/graphql".into(), None, None, 200);

        assert_eq!(
            state.fetch_patterns_json(),
            json!([
                { "urlPattern": "*/api/*", "requestStage": "Request" },
                { "urlPattern": "*/graphql", "requestStage": "Request" },
            ])
        );
    }
}
