use base64::Engine;
use serde_json::{json, Value};

use super::BodyCapture;

pub(super) trait BodyCaptureExt {
    fn omitted(reason: impl Into<String>) -> Self;
    fn len(&self) -> usize;
    fn to_text(&self) -> Option<String>;
}

impl BodyCaptureExt for BodyCapture {
    fn omitted(reason: impl Into<String>) -> Self {
        Self {
            bytes: Vec::new(),
            base64_encoded: false,
            mime_type: None,
            omitted: Some(reason.into()),
        }
    }

    fn len(&self) -> usize {
        self.bytes.len()
    }

    fn to_text(&self) -> Option<String> {
        if self.omitted.is_some() {
            return None;
        }
        if self.base64_encoded {
            return Some(base64::engine::general_purpose::STANDARD.encode(&self.bytes));
        }
        String::from_utf8(self.bytes.clone()).ok()
    }
}

pub(super) fn capture_text_body(data: &str, limit: usize, enabled: bool) -> BodyCapture {
    if !enabled {
        return BodyCapture::omitted("disabled");
    }
    if data.len() > limit {
        return BodyCapture::omitted("too large");
    }
    BodyCapture {
        bytes: data.as_bytes().to_vec(),
        base64_encoded: false,
        mime_type: None,
        omitted: None,
    }
}

pub(super) fn decode_response_body(value: Value, limit: usize) -> BodyCapture {
    let Some(text) = value.get("body").and_then(Value::as_str) else {
        return BodyCapture::omitted("empty");
    };
    let encoded = value
        .get("base64Encoded")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let bytes = if encoded {
        match base64::engine::general_purpose::STANDARD.decode(text) {
            Ok(bytes) => bytes,
            Err(error) => return BodyCapture::omitted(format!("decode failed: {}", error)),
        }
    } else {
        text.as_bytes().to_vec()
    };
    if bytes.len() > limit {
        return BodyCapture::omitted("too large");
    }
    BodyCapture {
        bytes,
        base64_encoded: encoded,
        mime_type: None,
        omitted: None,
    }
}

pub(super) fn body_summary(body: Option<&BodyCapture>) -> Value {
    match body {
        Some(body) => json!({
            "bytes": body.len(),
            "base64": body.base64_encoded,
            "omitted": body.omitted,
        }),
        None => Value::Null,
    }
}

pub(super) fn body_json(body: Option<&BodyCapture>) -> Value {
    match body {
        Some(body) => json!({
            "bytes": body.len(),
            "base64": body.base64_encoded,
            "text": body.to_text(),
            "omitted": body.omitted,
            "mime_type": body.mime_type,
        }),
        None => Value::Null,
    }
}

pub(super) fn limit_body_json(body: &mut Value, max_body: usize) {
    let Some(text) = body
        .get_mut("text")
        .and_then(|value| value.as_str())
        .map(str::to_string)
    else {
        return;
    };
    if text.len() <= max_body {
        return;
    }
    body["text"] = Value::String(text.chars().take(max_body).collect());
    body["truncated"] = Value::Bool(true);
}
