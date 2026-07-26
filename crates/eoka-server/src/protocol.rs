use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Serialize)]
pub struct Response {
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<ErrorBody>,
}

impl Response {
    pub fn ok(id: Value, result: Value) -> Self {
        Self {
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: Value, error: ServerError) -> Self {
        Self {
            id,
            result: None,
            error: Some(ErrorBody {
                code: error.code,
                message: error.message,
            }),
        }
    }
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    code: ErrorCode,
    message: String,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub enum ErrorCode {
    ElementNotFound,
    ElementNotVisible,
    Timeout,
    RetryExhausted,
    Cdp,
    InvalidPage,
    InvalidParams,
    UnknownMethod,
    Internal,
}

#[derive(Debug)]
pub struct ServerError {
    pub code: ErrorCode,
    pub message: String,
}

impl ServerError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidParams, message)
    }

    pub fn invalid_page(page_id: &str) -> Self {
        Self::new(
            ErrorCode::InvalidPage,
            format!("unknown pageId \"{page_id}\""),
        )
    }

    pub fn unknown_method(method: &str) -> Self {
        Self::new(
            ErrorCode::UnknownMethod,
            format!("unknown method \"{method}\""),
        )
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, message)
    }
}

impl From<eoka::Error> for ServerError {
    fn from(err: eoka::Error) -> Self {
        let code = match &err {
            eoka::Error::ElementNotFound(_) => ErrorCode::ElementNotFound,
            eoka::Error::ElementNotVisible { .. } => ErrorCode::ElementNotVisible,
            eoka::Error::Timeout(_) => ErrorCode::Timeout,
            eoka::Error::RetryExhausted { .. } => ErrorCode::RetryExhausted,
            eoka::Error::Cdp { .. } => ErrorCode::Cdp,
            _ => ErrorCode::Internal,
        };
        Self {
            code,
            message: err.to_string(),
        }
    }
}
