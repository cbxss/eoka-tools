use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Duration;

const API_BASE: &str = "https://api.anti-captcha.com";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug)]
pub struct CaptchaError(String);
impl fmt::Display for CaptchaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for CaptchaError {}
type Result<T> = std::result::Result<T, CaptchaError>;

#[derive(Debug, Serialize)]
struct CreateTaskRequest {
    #[serde(rename = "clientKey")]
    client_key: String,
    task: CaptchaTask,
}

/// Task types supported by Anti-Captcha.
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum CaptchaTask {
    #[serde(rename = "HCaptchaTaskProxyless")]
    HCaptcha {
        #[serde(rename = "websiteURL")]
        website_url: String,
        #[serde(rename = "websiteKey")]
        website_key: String,
    },
    #[serde(rename = "NoCaptchaTaskProxyless")]
    ReCaptchaV2 {
        #[serde(rename = "websiteURL")]
        website_url: String,
        #[serde(rename = "websiteKey")]
        website_key: String,
    },
    #[serde(rename = "RecaptchaV3TaskProxyless")]
    ReCaptchaV3 {
        #[serde(rename = "websiteURL")]
        website_url: String,
        #[serde(rename = "websiteKey")]
        website_key: String,
        #[serde(rename = "minScore")]
        min_score: f32,
        #[serde(rename = "pageAction")]
        page_action: String,
    },
    #[serde(rename = "AmazonTaskProxyless")]
    AmazonWaf {
        #[serde(rename = "websiteURL")]
        website_url: String,
        #[serde(rename = "websiteKey")]
        website_key: String,
        iv: String,
        context: String,
        #[serde(rename = "captchaScript", skip_serializing_if = "Option::is_none")]
        captcha_script: Option<String>,
        #[serde(rename = "challengeScript", skip_serializing_if = "Option::is_none")]
        challenge_script: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
struct CreateTaskResponse {
    #[serde(rename = "errorId")]
    error_id: u32,
    #[serde(rename = "errorCode")]
    error_code: Option<String>,
    #[serde(rename = "errorDescription")]
    error_description: Option<String>,
    #[serde(rename = "taskId")]
    task_id: Option<u64>,
}
#[derive(Debug, Serialize)]
struct GetResultRequest {
    #[serde(rename = "clientKey")]
    client_key: String,
    #[serde(rename = "taskId")]
    task_id: u64,
}
#[derive(Debug, Deserialize)]
struct GetResultResponse {
    #[serde(rename = "errorId")]
    error_id: u32,
    #[serde(rename = "errorCode")]
    error_code: Option<String>,
    #[serde(rename = "errorDescription")]
    error_description: Option<String>,
    status: Option<String>,
    solution: Option<CaptchaSolution>,
}

/// The normalized answer returned by Anti-Captcha.
#[derive(Debug, Deserialize)]
pub struct CaptchaSolution {
    #[serde(rename = "gRecaptchaResponse")]
    pub g_recaptcha_response: Option<String>,
    #[serde(rename = "gRecaptchaResponseWithoutSpaces")]
    pub g_recaptcha_response_without_spaces: Option<String>,
    pub text: Option<String>,
    pub token: Option<String>,
    #[serde(rename = "userAgent")]
    pub user_agent: Option<String>,
    #[serde(rename = "expireTime")]
    pub expire_time: Option<i64>,
}
impl CaptchaSolution {
    pub fn token(&self) -> Option<&str> {
        self.token
            .as_deref()
            .or(self.g_recaptcha_response.as_deref())
            .or(self.g_recaptcha_response_without_spaces.as_deref())
            .or(self.text.as_deref())
    }
}

/// Anti-Captcha API client.
pub struct AntiCaptcha {
    client: reqwest::Client,
    api_key: String,
}
impl AntiCaptcha {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::builder()
                .connect_timeout(CONNECT_TIMEOUT)
                .timeout(REQUEST_TIMEOUT)
                .build()
                .expect("static HTTP client configuration is valid"),
            api_key: api_key.into(),
        }
    }
    pub async fn solve_hcaptcha(&self, url: &str, key: &str) -> Result<CaptchaSolution> {
        self.validate_page(url, key)?;
        self.solve(CaptchaTask::HCaptcha {
            website_url: url.into(),
            website_key: key.into(),
        })
        .await
    }
    pub async fn solve_recaptcha_v2(&self, url: &str, key: &str) -> Result<CaptchaSolution> {
        self.validate_page(url, key)?;
        self.solve(CaptchaTask::ReCaptchaV2 {
            website_url: url.into(),
            website_key: key.into(),
        })
        .await
    }
    pub async fn solve_recaptcha_v3(
        &self,
        url: &str,
        key: &str,
        action: &str,
        score: f32,
    ) -> Result<CaptchaSolution> {
        self.validate_page(url, key)?;
        required("page action", action)?;
        if !score.is_finite() || !(0.0..=1.0).contains(&score) {
            return Err(CaptchaError(
                "minimum score must be between 0.0 and 1.0".into(),
            ));
        }
        self.solve(CaptchaTask::ReCaptchaV3 {
            website_url: url.into(),
            website_key: key.into(),
            page_action: action.into(),
            min_score: score,
        })
        .await
    }
    pub async fn solve_amazon_waf(
        &self,
        url: &str,
        key: &str,
        iv: &str,
        context: &str,
        captcha_script: Option<&str>,
        challenge_script: Option<&str>,
    ) -> Result<CaptchaSolution> {
        self.validate_page(url, key)?;
        required("AWS WAF iv", iv)?;
        required("AWS WAF context", context)?;
        validate_optional_url("captcha script", captcha_script)?;
        validate_optional_url("challenge script", challenge_script)?;
        self.solve(CaptchaTask::AmazonWaf {
            website_url: url.into(),
            website_key: key.into(),
            iv: iv.into(),
            context: context.into(),
            captcha_script: captcha_script.map(str::to_owned),
            challenge_script: challenge_script.map(str::to_owned),
        })
        .await
    }

    fn validate_page(&self, url: &str, key: &str) -> Result<()> {
        required("Anti-Captcha API key", &self.api_key)?;
        required("website key", key)?;
        validate_url("website URL", url)
    }

    async fn solve(&self, task: CaptchaTask) -> Result<CaptchaSolution> {
        let response = self
            .client
            .post(format!("{API_BASE}/createTask"))
            .json(&CreateTaskRequest {
                client_key: self.api_key.clone(),
                task,
            })
            .send()
            .await
            .map_err(|e| CaptchaError(e.to_string()))?
            .error_for_status()
            .map_err(|e| CaptchaError(e.to_string()))?;
        let created: CreateTaskResponse = response
            .json()
            .await
            .map_err(|e| CaptchaError(e.to_string()))?;
        if created.error_id != 0 {
            return Err(provider_error(
                "createTask",
                created.error_code,
                created.error_description,
            ));
        }
        let task_id = created
            .task_id
            .ok_or_else(|| CaptchaError("createTask returned no task ID".into()))?;
        for _ in 0..150 {
            tokio::time::sleep(Duration::from_secs(2)).await;
            let response = self
                .client
                .post(format!("{API_BASE}/getTaskResult"))
                .json(&GetResultRequest {
                    client_key: self.api_key.clone(),
                    task_id,
                })
                .send()
                .await
                .map_err(|e| CaptchaError(e.to_string()))?
                .error_for_status()
                .map_err(|e| CaptchaError(e.to_string()))?;
            let result: GetResultResponse = response
                .json()
                .await
                .map_err(|e| CaptchaError(e.to_string()))?;
            if result.error_id != 0 {
                return Err(provider_error(
                    "getTaskResult",
                    result.error_code,
                    result.error_description,
                ));
            }
            if result.status.as_deref() == Some("ready") {
                let solution = result
                    .solution
                    .ok_or_else(|| CaptchaError("task returned no solution".into()))?;
                return solution
                    .token()
                    .is_some()
                    .then_some(solution)
                    .ok_or_else(|| CaptchaError("task returned an empty solution".into()));
            }
        }
        Err(CaptchaError(
            "CAPTCHA solving timed out after 5 minutes".into(),
        ))
    }
}

fn required(name: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(CaptchaError(format!("{name} is required")));
    }
    Ok(())
}

fn validate_url(name: &str, value: &str) -> Result<()> {
    let parsed = url::Url::parse(value)
        .map_err(|_| CaptchaError(format!("{name} must be an absolute HTTP(S) URL")))?;
    match parsed.scheme() {
        "http" | "https" => Ok(()),
        _ => Err(CaptchaError(format!("{name} must use HTTP or HTTPS"))),
    }
}

fn validate_optional_url(name: &str, value: Option<&str>) -> Result<()> {
    value.map_or(Ok(()), |url| validate_url(name, url))
}

fn provider_error(
    operation: &str,
    code: Option<String>,
    description: Option<String>,
) -> CaptchaError {
    let detail = description
        .or(code)
        .unwrap_or_else(|| "unknown error".into());
    CaptchaError(format!("{operation} failed: {detail}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn serializes_amazon_waf_task() {
        let task = CaptchaTask::AmazonWaf {
            website_url: "https://example.test".into(),
            website_key: "key".into(),
            iv: "iv".into(),
            context: "context".into(),
            captcha_script: None,
            challenge_script: None,
        };
        let value = serde_json::to_value(task).unwrap();
        assert_eq!(value["type"], "AmazonTaskProxyless");
        assert_eq!(value["websiteKey"], "key");
        assert!(value.get("captchaScript").is_none());
    }

    #[test]
    fn rejects_malformed_or_incomplete_challenges() {
        assert!(validate_url("website URL", "file:///tmp/page").is_err());
        assert!(validate_url("website URL", "https://example.test").is_ok());
        assert!(required("website key", "  ").is_err());
        assert!(validate_optional_url("script", Some("not a URL")).is_err());
    }
}
