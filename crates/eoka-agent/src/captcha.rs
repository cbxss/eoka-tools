// Anti-captcha integration for automatic CAPTCHA solving
// Supports: hCaptcha, reCAPTCHA v2, reCAPTCHA v3

use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct CaptchaConfig {
    pub api_key: String,
    pub client_id: u32,
}

#[derive(Debug, Serialize)]
pub struct CreateTaskRequest {
    pub clientKey: String,
    pub task: CaptchaTask,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum CaptchaTask {
    #[serde(rename = "HCaptchaTaskProxyless")]
    HCaptchaProxyless {
        websiteURL: String,
        websiteKey: String,
    },
    #[serde(rename = "NoCaptchaTaskProxyless")]
    ReCaptchaV2Proxyless {
        websiteURL: String,
        websiteKey: String,
    },
    #[serde(rename = "RecaptchaV3TaskProxyless")]
    ReCaptchaV3Proxyless {
        websiteURL: String,
        websiteKey: String,
        minScore: f32,
        pageAction: String,
    },
}

#[derive(Debug, Deserialize)]
pub struct CreateTaskResponse {
    pub errorId: u32,
    pub errorCode: Option<String>,
    pub taskId: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct GetResultRequest {
    pub clientKey: String,
    pub taskId: u32,
}

#[derive(Debug, Deserialize)]
pub struct GetResultResponse {
    pub errorId: u32,
    pub errorCode: Option<String>,
    pub ready: bool,
    pub solution: Option<CaptchaSolution>,
}

#[derive(Debug, Deserialize)]
pub struct CaptchaSolution {
    pub gRecaptchaResponse: Option<String>,
    pub gRecaptchaResponseWithoutSpaces: Option<String>,
    pub text: Option<String>,
    pub expireTime: Option<i64>,
}

pub struct AntiCaptcha {
    client: reqwest::Client,
    api_key: String,
}

impl AntiCaptcha {
    pub fn new(api_key: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
        }
    }

    /// Solve hCaptcha
    pub async fn solve_hcaptcha(
        &self,
        website_url: &str,
        website_key: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        self.solve_captcha(CaptchaTask::HCaptchaProxyless {
            websiteURL: website_url.to_string(),
            websiteKey: website_key.to_string(),
        })
        .await
    }

    /// Solve reCAPTCHA v2
    pub async fn solve_recaptcha_v2(
        &self,
        website_url: &str,
        website_key: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        self.solve_captcha(CaptchaTask::ReCaptchaV2Proxyless {
            websiteURL: website_url.to_string(),
            websiteKey: website_key.to_string(),
        })
        .await
    }

    /// Solve reCAPTCHA v3
    pub async fn solve_recaptcha_v3(
        &self,
        website_url: &str,
        website_key: &str,
        page_action: &str,
        min_score: f32,
    ) -> Result<String, Box<dyn std::error::Error>> {
        self.solve_captcha(CaptchaTask::ReCaptchaV3Proxyless {
            websiteURL: website_url.to_string(),
            websiteKey: website_key.to_string(),
            minScore: min_score,
            pageAction: page_action.to_string(),
        })
        .await
    }

    /// Generic captcha solver
    async fn solve_captcha(
        &self,
        task: CaptchaTask,
    ) -> Result<String, Box<dyn std::error::Error>> {
        // Create task
        let create_req = CreateTaskRequest {
            clientKey: self.api_key.clone(),
            task,
        };

        let response = self
            .client
            .post("https://api.anti-captcha.com/createTask")
            .json(&create_req)
            .send()
            .await?;

        let create_resp: CreateTaskResponse = response.json().await?;

        if create_resp.errorId != 0 {
            return Err(format!(
                "Failed to create task: {} - {}",
                create_resp.errorId,
                create_resp.errorCode.unwrap_or_default()
            )
            .into());
        }

        let task_id = create_resp.taskId.ok_or("No task ID returned")?;

        // Poll for result (max 5 minutes)
        let max_attempts = 300;
        for attempt in 0..max_attempts {
            tokio::time::sleep(Duration::from_millis(500)).await;

            let result_req = GetResultRequest {
                clientKey: self.api_key.clone(),
                taskId: task_id,
            };

            let response = self
                .client
                .post("https://api.anti-captcha.com/getTaskResult")
                .json(&result_req)
                .send()
                .await?;

            let result_resp: GetResultResponse = response.json().await?;

            if result_resp.errorId != 0 {
                return Err(format!(
                    "Failed to get result: {} - {}",
                    result_resp.errorId,
                    result_resp.errorCode.unwrap_or_default()
                )
                .into());
            }

            if result_resp.ready {
                if let Some(solution) = result_resp.solution {
                    return Ok(solution
                        .gRecaptchaResponse
                        .or(solution.gRecaptchaResponseWithoutSpaces)
                        .or(solution.text)
                        .ok_or("No solution in response")?);
                }
                return Err("No solution data returned".into());
            }

            // Log progress occasionally
            if attempt % 10 == 0 && attempt > 0 {
                eprintln!("Captcha solving in progress... ({}/{}s)", attempt / 2, max_attempts / 2);
            }
        }

        Err("Captcha solving timeout (5 minutes)".into())
    }

    /// Detect captcha on page and return sitekey
    pub async fn detect_captcha_on_page(
        page: &eoka::Page,
    ) -> Option<CaptchaInfo> {
        // Check for hCaptcha
        let hcaptcha_script = r#"
            (function() {
                const elem = document.querySelector('[data-sitekey]');
                if (elem && elem.getAttribute('data-sitekey')) {
                    return elem.getAttribute('data-sitekey');
                }
                return null;
            })()
        "#;

        if let Ok(result) = page.evaluate::<serde_json::Value>(hcaptcha_script).await {
            if let Some(key_str) = result.as_str() {
                if !key_str.is_empty() {
                    return Some(CaptchaInfo {
                        captcha_type: "hcaptcha".to_string(),
                        sitekey: key_str.to_string(),
                    });
                }
            }
        }

        // Check for reCAPTCHA
        let recaptcha_script = r#"
            (function() {
                const scripts = document.querySelectorAll('script');
                for (const script of scripts) {
                    if (script.src && script.src.includes('recaptcha')) {
                        const matches = document.documentElement.innerHTML.match(/"sitekey"\s*:\s*"([^"]+)"/);
                        if (matches) return matches[1];
                    }
                }
                return null;
            })()
        "#;

        if let Ok(result) = page.evaluate::<serde_json::Value>(recaptcha_script).await {
            if let Some(key_str) = result.as_str() {
                if !key_str.is_empty() {
                    return Some(CaptchaInfo {
                        captcha_type: "recaptcha".to_string(),
                        sitekey: key_str.to_string(),
                    });
                }
            }
        }

        None
    }
}

#[derive(Debug, Clone)]
pub struct CaptchaInfo {
    pub captcha_type: String,
    pub sitekey: String,
}
