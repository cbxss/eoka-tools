use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ErrorDetail {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retryable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl ErrorDetail {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable: None,
            hint: None,
        }
    }

    pub fn retryable(mut self, retryable: bool) -> Self {
        self.retryable = Some(retryable);
        self
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ResponseMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cmd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub socket: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log: Option<String>,
}

impl ResponseMeta {
    pub fn is_empty(&self) -> bool {
        self.session.is_none() && self.cmd.is_none() && self.socket.is_none() && self.log.is_none()
    }
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Response {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_detail: Option<ErrorDetail>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<ResponseMeta>,
}

impl Response {
    pub fn ok(data: impl Into<serde_json::Value>) -> Self {
        Self {
            ok: true,
            data: Some(data.into()),
            error: None,
            error_detail: None,
            meta: None,
        }
    }

    pub fn ok_text(msg: impl Into<String>) -> Self {
        Self::ok(serde_json::Value::String(msg.into()))
    }

    pub fn err(msg: impl Into<String>) -> Self {
        let msg = msg.into();
        Self {
            ok: false,
            data: None,
            error: Some(msg.clone()),
            error_detail: Some(classify_error(&msg)),
            meta: None,
        }
    }

    pub fn err_detail(detail: ErrorDetail) -> Self {
        Self {
            ok: false,
            data: None,
            error: Some(detail.message.clone()),
            error_detail: Some(detail),
            meta: None,
        }
    }

    pub fn with_meta(mut self, meta: ResponseMeta) -> Self {
        if !meta.is_empty() {
            self.meta = Some(meta);
        }
        self
    }
}

fn classify_error(message: &str) -> ErrorDetail {
    let lower = message.to_ascii_lowercase();
    if lower.contains("no browser open") {
        return ErrorDetail::new("no_browser_open", message)
            .retryable(true)
            .hint("Run eoka open before browser observation or interaction commands.");
    }
    if lower.contains("unknown") || lower.contains("invalid") {
        return ErrorDetail::new("invalid_input", message).retryable(false);
    }
    if lower.contains("daemon failed to start") || lower.contains("transport") {
        return ErrorDetail::new("transport_error", message).retryable(true);
    }
    ErrorDetail::new("eoka_error", message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_error_keeps_legacy_error_string() {
        let response = Response::err("No browser open. Use 'open' first.");

        assert_eq!(
            response.error.as_deref(),
            Some("No browser open. Use 'open' first.")
        );
        let detail = response.error_detail.unwrap();
        assert_eq!(detail.code, "no_browser_open");
        assert_eq!(detail.retryable, Some(true));
        assert!(detail.hint.unwrap().contains("eoka open"));
    }

    #[test]
    fn empty_meta_is_not_serialized() {
        let value =
            serde_json::to_value(Response::ok("ok").with_meta(ResponseMeta::default())).unwrap();

        assert!(value.get("meta").is_none());
    }
}
