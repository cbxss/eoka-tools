use rmcp::model::ErrorData;

#[cfg(test)]
use rmcp::model::ErrorCode;
use serde_json::Value;
use std::fmt;

pub const ERR_NO_BROWSER: &str = "No browser open. Use navigate first.";
pub const ERR_NO_TAB: &str = "No tab open. Use navigate first.";

/// Typed error for MCP tool handlers.
#[derive(Debug)]
pub enum AgentError {
    Internal(String),
    InvalidInput(String),
}

impl fmt::Display for AgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentError::Internal(msg) => write!(f, "{}", msg),
            AgentError::InvalidInput(msg) => write!(f, "{}", msg),
        }
    }
}

impl From<AgentError> for ErrorData {
    fn from(e: AgentError) -> Self {
        match e {
            AgentError::Internal(msg) => ErrorData::internal_error(msg, None::<Value>),
            AgentError::InvalidInput(msg) => ErrorData::invalid_params(msg, None::<Value>),
        }
    }
}

/// Shorthand: create an `ErrorData::internal_error` from anything that implements Display.
/// Also logs transport errors to stderr.
pub fn internal(e: impl fmt::Display) -> ErrorData {
    let msg = e.to_string();
    if is_transport_error_msg(&msg) {
        eprintln!("[eoka-agent] transport error: {}", msg);
    }
    ErrorData::internal_error(msg, None::<Value>)
}

/// Shorthand: create an `ErrorData::invalid_params`.
pub fn invalid(msg: impl Into<String>) -> ErrorData {
    ErrorData::invalid_params(msg.into(), None::<Value>)
}

/// Check if an error message indicates a broken connection that requires session reset.
pub fn is_transport_error_msg(msg: &str) -> bool {
    let m = msg.as_bytes();
    const NEEDLES: &[&str] = &[
        "websocket",
        "transport",
        "timed out",
        "connection",
        "broken pipe",
        "reset by peer",
    ];
    NEEDLES.iter().any(|n| {
        m.windows(n.len())
            .any(|w| w.eq_ignore_ascii_case(n.as_bytes()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_error_positive() {
        assert!(is_transport_error_msg("WebSocket error: connection reset"));
        assert!(is_transport_error_msg("request timed out after 30s"));
        assert!(is_transport_error_msg("broken pipe"));
        assert!(is_transport_error_msg("BROKEN PIPE")); // case-insensitive
        assert!(is_transport_error_msg("reset by peer"));
        assert!(is_transport_error_msg("transport error occurred"));
        assert!(is_transport_error_msg("connection refused"));
    }

    #[test]
    fn transport_error_negative() {
        assert!(!is_transport_error_msg("element not found"));
        assert!(!is_transport_error_msg("invalid selector"));
        assert!(!is_transport_error_msg(""));
        assert!(!is_transport_error_msg("No browser open"));
    }

    #[test]
    fn agent_error_internal_to_error_data() {
        let e: ErrorData = AgentError::Internal("boom".into()).into();
        assert_eq!(e.code, ErrorCode::INTERNAL_ERROR);
        assert_eq!(e.message.as_ref(), "boom");
    }

    #[test]
    fn agent_error_invalid_to_error_data() {
        let e: ErrorData = AgentError::InvalidInput("bad input".into()).into();
        assert_eq!(e.code, ErrorCode::INVALID_PARAMS);
        assert_eq!(e.message.as_ref(), "bad input");
    }

    #[test]
    fn internal_helper() {
        let e = internal("something failed");
        assert_eq!(e.code, ErrorCode::INTERNAL_ERROR);
        assert_eq!(e.message.as_ref(), "something failed");
    }

    #[test]
    fn invalid_helper() {
        let e = invalid("missing field");
        assert_eq!(e.code, ErrorCode::INVALID_PARAMS);
        assert_eq!(e.message.as_ref(), "missing field");
    }
}
