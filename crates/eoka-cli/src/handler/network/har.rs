use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use super::body::BodyCaptureExt;
use super::NetworkEntry;

pub(super) fn build_har(entries: &[NetworkEntry]) -> Value {
    let har_entries: Vec<Value> = entries
        .iter()
        .filter(|entry| entry.complete)
        .map(har_entry)
        .collect();
    json!({
        "log": {
            "version": "1.2",
            "creator": {
                "name": "eoka",
                "version": env!("CARGO_PKG_VERSION"),
            },
            "entries": har_entries,
        }
    })
}

pub(super) fn build_json_export(entries: &[NetworkEntry]) -> Value {
    json!({
        "version": 1,
        "creator": {
            "name": "eoka",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "entries": entries.iter().map(NetworkEntry::full_json).collect::<Vec<_>>(),
    })
}

fn har_entry(entry: &NetworkEntry) -> Value {
    json!({
        "startedDateTime": har_time(entry),
        "time": entry.duration_ms(),
        "request": {
            "method": entry.method,
            "url": entry.url,
            "httpVersion": "HTTP/1.1",
            "cookies": request_cookies(entry),
            "headers": har_headers(&entry.request_headers),
            "queryString": har_query(&entry.url),
            "headersSize": header_size(entry.request_headers_text.as_deref()),
            "bodySize": request_body_size(entry),
            "postData": har_post_data(entry),
        },
        "response": {
            "status": entry.status.unwrap_or(0),
            "statusText": entry.status_text.clone().unwrap_or_default(),
            "httpVersion": entry.protocol.clone().unwrap_or_else(|| "HTTP/1.1".to_string()),
            "cookies": response_cookies(entry),
            "headers": har_headers(&entry.response_headers),
            "content": har_content(entry),
            "redirectURL": entry.redirect_url.clone().unwrap_or_default(),
            "headersSize": header_size(entry.response_headers_text.as_deref()),
            "bodySize": response_body_size(entry),
        },
        "cache": {},
        "timings": har_timings(entry),
        "serverIPAddress": entry.remote_ip,
        "_eoka": {
            "id": entry.id,
            "session_id": entry.session_id,
            "target_id": entry.target_id,
            "request_id": entry.request_id,
            "hop": entry.hop,
            "resource_type": entry.resource_type,
            "failure": entry.failure_text,
            "request_body_omitted": entry.request_post_data.as_ref().and_then(|body| body.omitted.clone()),
            "response_body_omitted": entry.response_body.as_ref().and_then(|body| body.omitted.clone()),
        }
    })
}

fn har_headers(headers: &Map<String, Value>) -> Vec<Value> {
    headers
        .iter()
        .map(|(name, value)| {
            json!({
                "name": name,
                "value": value.as_str().map(str::to_string).unwrap_or_else(|| value.to_string()),
            })
        })
        .collect()
}

fn request_cookies(entry: &NetworkEntry) -> Vec<Value> {
    header_value(&entry.request_headers, "cookie")
        .map(parse_cookie_header)
        .unwrap_or_default()
}

fn response_cookies(entry: &NetworkEntry) -> Vec<Value> {
    let Some(header) = header_value(&entry.response_headers, "set-cookie") else {
        return Vec::new();
    };
    header
        .split(',')
        .filter_map(|cookie| cookie.split(';').next())
        .filter_map(cookie_pair)
        .collect()
}

fn parse_cookie_header(header: &str) -> Vec<Value> {
    header.split(';').filter_map(cookie_pair).collect()
}

fn cookie_pair(raw: &str) -> Option<Value> {
    let (name, value) = raw.trim().split_once('=')?;
    Some(json!({ "name": name.trim(), "value": value.trim() }))
}

fn header_size(text: Option<&str>) -> i64 {
    text.map(|text| text.len() as i64).unwrap_or(-1)
}

fn request_body_size(entry: &NetworkEntry) -> i64 {
    entry
        .request_post_data
        .as_ref()
        .map(|body| body.len() as i64)
        .unwrap_or(0)
}

fn response_body_size(entry: &NetworkEntry) -> i64 {
    entry
        .encoded_data_length
        .map(|length| length.round() as i64)
        .or_else(|| entry.response_body.as_ref().map(|body| body.len() as i64))
        .unwrap_or(-1)
}

fn har_timings(entry: &NetworkEntry) -> Value {
    let Some(timing) = entry.response_timing.as_ref().and_then(Value::as_object) else {
        return json!({
            "send": 0,
            "wait": entry.duration_ms(),
            "receive": 0,
        });
    };
    let send_start = timing
        .get("sendStart")
        .and_then(Value::as_f64)
        .unwrap_or(-1.0);
    let send_end = timing
        .get("sendEnd")
        .and_then(Value::as_f64)
        .unwrap_or(-1.0);
    let receive_headers_end = timing
        .get("receiveHeadersEnd")
        .and_then(Value::as_f64)
        .unwrap_or(-1.0);
    let wait = if send_end >= 0.0 && receive_headers_end >= send_end {
        receive_headers_end - send_end
    } else {
        entry.duration_ms()
    };
    json!({
        "blocked": -1,
        "dns": timing_span(timing, "dnsStart", "dnsEnd"),
        "connect": timing_span(timing, "connectStart", "connectEnd"),
        "ssl": timing_span(timing, "sslStart", "sslEnd"),
        "send": if send_start >= 0.0 && send_end >= send_start { send_end - send_start } else { 0.0 },
        "wait": wait,
        "receive": 0,
    })
}

fn timing_span(timing: &Map<String, Value>, start: &str, end: &str) -> f64 {
    let start = timing.get(start).and_then(Value::as_f64).unwrap_or(-1.0);
    let end = timing.get(end).and_then(Value::as_f64).unwrap_or(-1.0);
    if start >= 0.0 && end >= start {
        end - start
    } else {
        -1.0
    }
}

fn har_query(url: &str) -> Vec<Value> {
    url::Url::parse(url)
        .ok()
        .map(|url| {
            url.query_pairs()
                .map(|(name, value)| json!({ "name": name, "value": value }))
                .collect()
        })
        .unwrap_or_default()
}

fn har_post_data(entry: &NetworkEntry) -> Value {
    match entry
        .request_post_data
        .as_ref()
        .and_then(BodyCaptureExt::to_text)
    {
        Some(text) => json!({
            "mimeType": header_value(&entry.request_headers, "content-type")
                .unwrap_or("application/octet-stream"),
            "text": text,
        }),
        None => Value::Null,
    }
}

fn har_content(entry: &NetworkEntry) -> Value {
    let body = entry.response_body.as_ref();
    let mime_type = entry
        .mime_type
        .as_deref()
        .or_else(|| body.and_then(|body| body.mime_type.as_deref()))
        .unwrap_or("application/octet-stream");
    let text = body.and_then(BodyCaptureExt::to_text).unwrap_or_default();
    let encoding = body
        .filter(|body| body.base64_encoded)
        .map(|_| "base64".to_string());
    json!({
        "size": body.map(|body| body.len() as i64).unwrap_or(0),
        "mimeType": mime_type,
        "text": text,
        "encoding": encoding,
    })
}

fn har_time(entry: &NetworkEntry) -> String {
    let secs = entry.wall_time.unwrap_or_default();
    if secs == 0.0 {
        return "1970-01-01T00:00:00.000Z".to_string();
    }
    let millis = (secs * 1000.0).round() as i64;
    let whole = millis.div_euclid(1000);
    let ms = millis.rem_euclid(1000);
    format!(
        "{}.{:03}Z",
        chrono::DateTime::from_timestamp(whole, 0)
            .unwrap_or_default()
            .format("%Y-%m-%dT%H:%M:%S"),
        ms
    )
}

pub(super) fn atomic_write_json(path: &Path, value: &Value) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let temp = temp_path(path);
    let json = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    std::fs::write(&temp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&temp, path).map_err(|e| e.to_string())?;
    Ok(())
}

fn temp_path(path: &Path) -> PathBuf {
    let mut temp = path.to_path_buf();
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("network.har");
    temp.set_file_name(format!(".{}.{}.tmp", name, std::process::id()));
    temp
}

fn header_value<'a>(headers: &'a Map<String, Value>, lowercase_name: &str) -> Option<&'a str> {
    headers
        .get(lowercase_name)
        .or_else(|| {
            headers
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(lowercase_name))
                .map(|(_, value)| value)
        })
        .and_then(Value::as_str)
}
