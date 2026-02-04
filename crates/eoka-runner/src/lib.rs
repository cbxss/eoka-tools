//! # eoka-runner
//!
//! Config-based browser automation. Define actions in YAML, execute deterministically.
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use eoka_runner::{Config, Runner};
//!
//! # #[tokio::main]
//! # async fn main() -> eoka_runner::Result<()> {
//! let config = Config::load("automation.yaml")?;
//! let mut runner = Runner::new(&config.browser).await?;
//! let result = runner.run(&config).await?;
//! println!("Success: {}", result.success);
//! # Ok(())
//! # }
//! ```

mod config;
mod runner;

pub use config::{
    Action, BrowserConfig, Config, ParamDef, Params, SuccessCondition, Target, TargetUrl,
};
pub use runner::{RunResult, Runner};

/// Result type for eoka-runner operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur during config loading or execution.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("config error: {0}")]
    Config(String),

    #[error("yaml parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("browser error: {0}")]
    Browser(#[from] eoka::Error),

    #[error("action failed: {0}")]
    ActionFailed(String),

    #[error("timeout: {0}")]
    Timeout(String),

    #[error("assertion failed: {0}")]
    AssertionFailed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_config() {
        let yaml = r#"
name: "Test"
target:
  url: "https://example.com"
"#;
        let config = Config::parse(yaml).unwrap();
        assert_eq!(config.name, "Test");
        assert_eq!(config.target.url, "https://example.com");
        assert!(config.actions.is_empty());
        assert!(!config.browser.headless);
    }

    #[test]
    fn test_parse_browser_config() {
        let yaml = r#"
name: "Test"
browser:
  headless: true
  proxy: "http://localhost:8080"
  user_agent: "Custom UA"
target:
  url: "https://example.com"
"#;
        let config = Config::parse(yaml).unwrap();
        assert!(config.browser.headless);
        assert_eq!(config.browser.proxy, Some("http://localhost:8080".into()));
        assert_eq!(config.browser.user_agent, Some("Custom UA".into()));
    }

    #[test]
    fn test_parse_navigation_actions() {
        let yaml = r#"
name: "Test"
target:
  url: "https://example.com"
actions:
  - goto:
      url: "https://other.com"
  - back
  - forward
  - reload
"#;
        let config = Config::parse(yaml).unwrap();
        assert_eq!(config.actions.len(), 4);

        assert!(matches!(config.actions[0], Action::Goto(_)));
        assert!(matches!(config.actions[1], Action::Back));
        assert!(matches!(config.actions[2], Action::Forward));
        assert!(matches!(config.actions[3], Action::Reload));
    }

    #[test]
    fn test_parse_wait_actions() {
        let yaml = r#"
name: "Test"
target:
  url: "https://example.com"
actions:
  - wait:
      ms: 1000
  - wait_for_network_idle:
      idle_ms: 500
      timeout_ms: 5000
  - wait_for_text:
      text: "Hello"
      timeout_ms: 3000
  - wait_for_url:
      contains: "/success"
"#;
        let config = Config::parse(yaml).unwrap();
        assert_eq!(config.actions.len(), 4);

        if let Action::Wait(a) = &config.actions[0] {
            assert_eq!(a.ms, 1000);
        } else {
            panic!("Expected Wait action");
        }

        if let Action::WaitForNetworkIdle(a) = &config.actions[1] {
            assert_eq!(a.idle_ms, 500);
            assert_eq!(a.timeout_ms, 5000);
        } else {
            panic!("Expected WaitForNetworkIdle action");
        }

        if let Action::WaitForText(a) = &config.actions[2] {
            assert_eq!(a.text, "Hello");
            assert_eq!(a.timeout_ms, 3000);
        } else {
            panic!("Expected WaitForText action");
        }
    }

    #[test]
    fn test_parse_click_actions() {
        let yaml = r##"
name: "Test"
target:
  url: "https://example.com"
actions:
  - click:
      selector: "#btn"
      human: true
      scroll_into_view: true
  - click:
      text: "Submit"
  - try_click:
      selector: ".optional"
  - try_click_any:
      texts: ["Accept", "OK", "Close"]
"##;
        let config = Config::parse(yaml).unwrap();
        assert_eq!(config.actions.len(), 4);

        if let Action::Click(a) = &config.actions[0] {
            assert_eq!(a.target.selector, Some("#btn".into()));
            assert!(a.human);
            assert!(a.scroll_into_view);
        } else {
            panic!("Expected Click action");
        }

        if let Action::Click(a) = &config.actions[1] {
            assert_eq!(a.target.text, Some("Submit".into()));
            assert!(!a.human);
        } else {
            panic!("Expected Click action");
        }

        if let Action::TryClickAny(a) = &config.actions[3] {
            assert_eq!(
                a.texts,
                Some(vec!["Accept".into(), "OK".into(), "Close".into()])
            );
        } else {
            panic!("Expected TryClickAny action");
        }
    }

    #[test]
    fn test_parse_input_actions() {
        let yaml = r##"
name: "Test"
target:
  url: "https://example.com"
actions:
  - fill:
      selector: "#email"
      value: "test@example.com"
      human: true
  - type:
      text: "Search"
      value: "query"
  - clear:
      selector: "#input"
"##;
        let config = Config::parse(yaml).unwrap();
        assert_eq!(config.actions.len(), 3);

        if let Action::Fill(a) = &config.actions[0] {
            assert_eq!(a.target.selector, Some("#email".into()));
            assert_eq!(a.value, "test@example.com");
            assert!(a.human);
        } else {
            panic!("Expected Fill action");
        }
    }

    #[test]
    fn test_parse_scroll_actions() {
        let yaml = r##"
name: "Test"
target:
  url: "https://example.com"
actions:
  - scroll:
      direction: down
      amount: 3
  - scroll_to:
      selector: "#footer"
"##;
        let config = Config::parse(yaml).unwrap();
        assert_eq!(config.actions.len(), 2);

        if let Action::Scroll(a) = &config.actions[0] {
            assert!(matches!(
                a.direction,
                config::actions::ScrollDirection::Down
            ));
            assert_eq!(a.amount, 3);
        } else {
            panic!("Expected Scroll action");
        }
    }

    #[test]
    fn test_parse_debug_actions() {
        let yaml = r#"
name: "Test"
target:
  url: "https://example.com"
actions:
  - screenshot:
      path: "test.png"
  - log:
      message: "Step completed"
  - assert_text:
      text: "Success"
  - assert_url:
      contains: "/done"
"#;
        let config = Config::parse(yaml).unwrap();
        assert_eq!(config.actions.len(), 4);

        if let Action::Screenshot(a) = &config.actions[0] {
            assert_eq!(a.path, "test.png");
        } else {
            panic!("Expected Screenshot action");
        }

        if let Action::Log(a) = &config.actions[1] {
            assert_eq!(a.message, "Step completed");
        } else {
            panic!("Expected Log action");
        }
    }

    #[test]
    fn test_parse_control_flow_actions() {
        let yaml = r#"
name: "Test"
target:
  url: "https://example.com"
actions:
  - if_text_exists:
      text: "Cookie banner"
      then:
        - click:
            text: "Accept"
      else:
        - log:
            message: "No banner"
  - repeat:
      times: 3
      actions:
        - scroll:
            direction: down
"#;
        let config = Config::parse(yaml).unwrap();
        assert_eq!(config.actions.len(), 2);

        if let Action::IfTextExists(a) = &config.actions[0] {
            assert_eq!(a.text, "Cookie banner");
            assert_eq!(a.then_actions.len(), 1);
            assert_eq!(a.else_actions.len(), 1);
        } else {
            panic!("Expected IfTextExists action");
        }

        if let Action::Repeat(a) = &config.actions[1] {
            assert_eq!(a.times, 3);
            assert_eq!(a.actions.len(), 1);
        } else {
            panic!("Expected Repeat action");
        }
    }

    #[test]
    fn test_parse_success_conditions() {
        let yaml = r#"
name: "Test"
target:
  url: "https://example.com"
success:
  any:
    - url_contains: "/cart"
    - text_contains: "Added to cart"
"#;
        let config = Config::parse(yaml).unwrap();
        let success = config.success.unwrap();
        let any = success.any.unwrap();
        assert_eq!(any.len(), 2);
    }

    #[test]
    fn test_parse_on_failure() {
        let yaml = r#"
name: "Test"
target:
  url: "https://example.com"
on_failure:
  screenshot: "error.png"
  retry:
    attempts: 3
    delay_ms: 1000
"#;
        let config = Config::parse(yaml).unwrap();
        let on_failure = config.on_failure.unwrap();
        assert_eq!(on_failure.screenshot, Some("error.png".into()));
        let retry = on_failure.retry.unwrap();
        assert_eq!(retry.attempts, 3);
        assert_eq!(retry.delay_ms, 1000);
    }

    #[test]
    fn test_validation_missing_name() {
        let yaml = r#"
target:
  url: "https://example.com"
"#;
        let result = Config::parse(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_validation_missing_url() {
        let yaml = r#"
name: "Test"
target:
  url: ""
"#;
        let result = Config::parse(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_validation_empty_name() {
        let yaml = r#"
name: ""
target:
  url: "https://example.com"
"#;
        let result = Config::parse(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_default_values() {
        let yaml = r##"
name: "Test"
target:
  url: "https://example.com"
actions:
  - wait_for_network_idle: {}
  - click:
      selector: "#btn"
"##;
        let config = Config::parse(yaml).unwrap();

        if let Action::WaitForNetworkIdle(a) = &config.actions[0] {
            assert_eq!(a.idle_ms, 500); // default
            assert_eq!(a.timeout_ms, 10000); // default
        } else {
            panic!("Expected WaitForNetworkIdle");
        }

        if let Action::Click(a) = &config.actions[1] {
            assert!(!a.human); // default false
            assert!(!a.scroll_into_view); // default false
        } else {
            panic!("Expected Click");
        }
    }

    #[test]
    fn test_load_example_config() {
        let config = Config::load("configs/example.yaml").unwrap();
        assert_eq!(config.name, "Example Automation");
        assert_eq!(config.target.url, "https://example.com");
    }

    #[test]
    fn test_parse_new_actions() {
        let yaml = r##"
name: "Test"
target:
  url: "https://example.com"
actions:
  - select:
      selector: "#country"
      value: "US"
  - press_key:
      key: "Enter"
  - hover:
      text: "Menu"
  - set_cookie:
      name: "session"
      value: "abc123"
      domain: ".example.com"
  - delete_cookie:
      name: "tracking"
  - execute:
      js: "window.scrollTo(0, 0)"
"##;
        let config = Config::parse(yaml).unwrap();
        assert_eq!(config.actions.len(), 6);

        if let Action::Select(a) = &config.actions[0] {
            assert_eq!(a.target.selector, Some("#country".into()));
            assert_eq!(a.value, "US");
        } else {
            panic!("Expected Select action");
        }

        if let Action::PressKey(a) = &config.actions[1] {
            assert_eq!(a.key, "Enter");
        } else {
            panic!("Expected PressKey action");
        }

        if let Action::Hover(a) = &config.actions[2] {
            assert_eq!(a.target.text, Some("Menu".into()));
        } else {
            panic!("Expected Hover action");
        }

        if let Action::SetCookie(a) = &config.actions[3] {
            assert_eq!(a.name, "session");
            assert_eq!(a.value, "abc123");
            assert_eq!(a.domain, Some(".example.com".into()));
        } else {
            panic!("Expected SetCookie action");
        }

        if let Action::DeleteCookie(a) = &config.actions[4] {
            assert_eq!(a.name, "tracking");
        } else {
            panic!("Expected DeleteCookie action");
        }

        if let Action::Execute(a) = &config.actions[5] {
            assert_eq!(a.js, "window.scrollTo(0, 0)");
        } else {
            panic!("Expected Execute action");
        }
    }

    #[test]
    fn test_parse_viewport_config() {
        let yaml = r#"
name: "Test"
browser:
  headless: true
  viewport:
    width: 1920
    height: 1080
  proxy: "http://localhost:8080"
  user_agent: "Custom UA"
target:
  url: "https://example.com"
"#;
        let config = Config::parse(yaml).unwrap();
        assert!(config.browser.headless);
        assert_eq!(config.browser.proxy, Some("http://localhost:8080".into()));
        assert_eq!(config.browser.user_agent, Some("Custom UA".into()));
        let viewport = config.browser.viewport.unwrap();
        assert_eq!(viewport.width, 1920);
        assert_eq!(viewport.height, 1080);
    }

    #[test]
    fn test_validation_both_any_and_all() {
        let yaml = r#"
name: "Test"
target:
  url: "https://example.com"
success:
  any:
    - url_contains: "/success"
  all:
    - text_contains: "Done"
"#;
        let result = Config::parse(yaml);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("either 'any' or 'all'"));
    }

    #[test]
    fn test_validation_zero_retry_attempts() {
        let yaml = r#"
name: "Test"
target:
  url: "https://example.com"
on_failure:
  retry:
    attempts: 0
    delay_ms: 1000
"#;
        let result = Config::parse(yaml);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("at least 1"));
    }

    #[test]
    fn test_params_substitution() {
        let yaml = r##"
name: "Login"
params:
  email:
    required: true
  password:
    required: true
target:
  url: "https://example.com/login"
actions:
  - fill:
      selector: "#email"
      value: "${email}"
  - fill:
      selector: "#password"
      value: "${password}"
"##;
        let params = Params::new()
            .set("email", "test@example.com")
            .set("password", "secret123");
        let config = Config::parse_with_params(yaml, &params).unwrap();

        if let Action::Fill(a) = &config.actions[0] {
            assert_eq!(a.value, "test@example.com");
        } else {
            panic!("Expected Fill action");
        }

        if let Action::Fill(a) = &config.actions[1] {
            assert_eq!(a.value, "secret123");
        } else {
            panic!("Expected Fill action");
        }
    }

    #[test]
    fn test_params_default_value() {
        let yaml = r##"
name: "Test"
params:
  search_text:
    default: "default query"
target:
  url: "https://example.com"
actions:
  - fill:
      selector: "#search"
      value: "${search_text}"
"##;
        // No params provided - should use default
        let config = Config::parse(yaml).unwrap();
        if let Action::Fill(a) = &config.actions[0] {
            assert_eq!(a.value, "default query");
        } else {
            panic!("Expected Fill action");
        }
    }

    #[test]
    fn test_params_missing_required() {
        let yaml = r##"
name: "Test"
params:
  api_key:
    required: true
target:
  url: "https://example.com/${api_key}"
"##;
        let result = Config::parse(yaml);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("api_key"));
    }

    #[test]
    fn test_params_in_target_url() {
        let yaml = r##"
name: "Test"
params:
  env:
    default: "staging"
target:
  url: "https://${env}.example.com"
"##;
        let params = Params::new().set("env", "production");
        let config = Config::parse_with_params(yaml, &params).unwrap();
        assert_eq!(config.target.url, "https://production.example.com");
    }

    #[test]
    fn test_parse_include_action() {
        let yaml = r##"
name: "Test"
target:
  url: "https://example.com"
actions:
  - include:
      path: "flows/login.yaml"
      params:
        email: "test@example.com"
        password: "secret"
  - click:
      text: "Continue"
"##;
        let config = Config::parse(yaml).unwrap();
        assert_eq!(config.actions.len(), 2);

        if let Action::Include(a) = &config.actions[0] {
            assert_eq!(a.path, "flows/login.yaml");
            assert_eq!(a.params.get("email"), Some(&"test@example.com".to_string()));
            assert_eq!(a.params.get("password"), Some(&"secret".to_string()));
        } else {
            panic!("Expected Include action");
        }
    }

    #[test]
    fn test_parse_include_simple() {
        let yaml = r##"
name: "Test"
target:
  url: "https://example.com"
actions:
  - include:
      path: "common/setup.yaml"
"##;
        let config = Config::parse(yaml).unwrap();

        if let Action::Include(a) = &config.actions[0] {
            assert_eq!(a.path, "common/setup.yaml");
            assert!(a.params.is_empty());
        } else {
            panic!("Expected Include action");
        }
    }
}
