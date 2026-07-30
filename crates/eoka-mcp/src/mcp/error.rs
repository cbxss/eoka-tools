use rmcp::model::ErrorData;

#[cfg(test)]
use rmcp::model::ErrorCode;
use serde_json::Value;
use std::fmt;

#[derive(Debug)]
pub enum AgentError {
    Internal(String),
    InvalidInput(String),
    NoBrowser,
    NoTab,
    Transport(String),
    ElementNotFound(String),
    Timeout(String),
    NavigationFailed(String),
}

impl fmt::Display for AgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentError::Internal(msg) => write!(f, "{}", msg),
            AgentError::InvalidInput(msg) => write!(f, "{}", msg),
            AgentError::NoBrowser => write!(f, "No browser open. Use navigate first."),
            AgentError::NoTab => write!(f, "No tab open. Use navigate first."),
            AgentError::Transport(msg) => write!(f, "{}", msg),
            AgentError::ElementNotFound(msg) => write!(f, "{}", msg),
            AgentError::Timeout(msg) => write!(f, "{}", msg),
            AgentError::NavigationFailed(msg) => write!(f, "{}", msg),
        }
    }
}

impl From<AgentError> for ErrorData {
    fn from(e: AgentError) -> Self {
        match e {
            AgentError::NoBrowser | AgentError::NoTab | AgentError::InvalidInput(_) => {
                ErrorData::invalid_params(e.to_string(), None::<Value>)
            }
            AgentError::ElementNotFound(msg) => ErrorData::invalid_params(msg, None::<Value>),
            AgentError::Internal(msg) => ErrorData::internal_error(msg, None::<Value>),
            AgentError::Transport(msg) => ErrorData::internal_error(msg, None::<Value>),
            AgentError::Timeout(msg) => ErrorData::internal_error(msg, None::<Value>),
            AgentError::NavigationFailed(msg) => ErrorData::internal_error(msg, None::<Value>),
        }
    }
}

pub fn internal(e: impl fmt::Display) -> ErrorData {
    let msg = e.to_string();
    if is_transport_error_msg(&msg) {
        eprintln!("[eoka-mcp] transport error: {}", msg);
    }
    ErrorData::internal_error(msg, None::<Value>)
}

pub fn invalid(msg: impl Into<String>) -> ErrorData {
    ErrorData::invalid_params(msg.into(), None::<Value>)
}

pub fn is_transport_error_msg(msg: &str) -> bool {
    let m = msg.as_bytes();
    const NEEDLES: &[&str] = &[
        "websocket error",
        "transport error",
        "timed out",
        "connection closed",
        "connection reset",
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
        assert!(is_transport_error_msg("WebSocket Error: connection reset"));
        assert!(is_transport_error_msg("request timed out after 30s"));
        assert!(is_transport_error_msg("broken pipe"));
        assert!(is_transport_error_msg("BROKEN PIPE"));
        assert!(is_transport_error_msg("reset by peer"));
        assert!(is_transport_error_msg("transport error occurred"));
        assert!(is_transport_error_msg("connection closed unexpectedly"));
        assert!(is_transport_error_msg("connection reset by remote"));
    }

    #[test]
    fn transport_error_negative() {
        assert!(!is_transport_error_msg("element not found"));
        assert!(!is_transport_error_msg("invalid selector"));
        assert!(!is_transport_error_msg(""));
        assert!(!is_transport_error_msg("No browser open"));
        assert!(!is_transport_error_msg("connection refused by target"));
        assert!(!is_transport_error_msg("WebSocket connection to devtools"));
        assert!(!is_transport_error_msg("transport layer initialized"));
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
    fn agent_error_no_browser() {
        let e: ErrorData = AgentError::NoBrowser.into();
        assert_eq!(e.code, ErrorCode::INVALID_PARAMS);
        assert!(e.message.as_ref().contains("No browser"));
    }

    #[test]
    fn agent_error_no_tab() {
        let e: ErrorData = AgentError::NoTab.into();
        assert_eq!(e.code, ErrorCode::INVALID_PARAMS);
        assert!(e.message.as_ref().contains("No tab"));
    }

    #[test]
    fn agent_error_transport() {
        let e: ErrorData = AgentError::Transport("conn lost".into()).into();
        assert_eq!(e.code, ErrorCode::INTERNAL_ERROR);
        assert_eq!(e.message.as_ref(), "conn lost");
    }

    #[test]
    fn agent_error_element_not_found() {
        let e: ErrorData = AgentError::ElementNotFound("idx 5".into()).into();
        assert_eq!(e.code, ErrorCode::INVALID_PARAMS);
        assert_eq!(e.message.as_ref(), "idx 5");
    }

    #[test]
    fn agent_error_timeout() {
        let e: ErrorData = AgentError::Timeout("10s elapsed".into()).into();
        assert_eq!(e.code, ErrorCode::INTERNAL_ERROR);
        assert_eq!(e.message.as_ref(), "10s elapsed");
    }

    #[test]
    fn agent_error_navigation_failed() {
        let e: ErrorData = AgentError::NavigationFailed("404".into()).into();
        assert_eq!(e.code, ErrorCode::INTERNAL_ERROR);
        assert_eq!(e.message.as_ref(), "404");
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
