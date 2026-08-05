use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eoka::cdp::{Session as CdpSession, Transport};
use serde_json::{json, Value};
use tokio::sync::{mpsc, Mutex, Notify, Semaphore};

mod body;
mod har;
mod model;
mod runtime;

use har::{atomic_write_json, build_har, build_json_export};
pub use model::NetworkConfig;
use model::{
    entry_key, now_secs, BodyCapture, NetworkEntry, NetworkStore, DEFAULT_TOTAL_BUFFER_SIZE,
};
use runtime::{network_enable, run_recorder, NetworkControl};

#[derive(Clone)]
pub struct NetworkRecorder {
    namespace: String,
    store: Arc<Mutex<NetworkStore>>,
    control: mpsc::Sender<NetworkControl>,
    notify: Arc<Notify>,
}

impl NetworkRecorder {
    pub fn spawn(namespace: impl Into<String>, transport: Arc<Transport>) -> Self {
        let store = Arc::new(Mutex::new(NetworkStore {
            config: NetworkConfig::default(),
            ..NetworkStore::default()
        }));
        let notify = Arc::new(Notify::new());
        let (control, control_rx) = mpsc::channel(32);
        tokio::spawn(run_recorder(
            store.clone(),
            transport.clone(),
            transport.subscribe(),
            control_rx,
            Arc::new(Semaphore::new(8)),
            notify.clone(),
        ));
        Self {
            namespace: namespace.into(),
            store,
            control,
            notify,
        }
    }

    pub async fn start(
        &self,
        session: &CdpSession,
        config: NetworkConfig,
    ) -> Result<Value, String> {
        network_enable(session, &config).await?;
        self.control
            .send(NetworkControl::Start {
                session_id: session.session_id().to_string(),
                target_id: session.target_id().to_string(),
                config,
            })
            .await
            .map_err(|e| e.to_string())?;
        Ok(self.status().await)
    }

    pub async fn update_session(&self, session: &CdpSession) -> Result<(), String> {
        let config = { self.store.lock().await.config.clone() };
        network_enable(session, &config).await?;
        self.control
            .send(NetworkControl::SetSession {
                session_id: session.session_id().to_string(),
                target_id: session.target_id().to_string(),
            })
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn stop(&self, session: Option<CdpSession>) -> Result<Value, String> {
        if let Some(session) = session {
            let _ = session
                .send::<_, Value>("Network.disable", &json!({}))
                .await;
        }
        self.control
            .send(NetworkControl::Stop)
            .await
            .map_err(|e| e.to_string())?;
        Ok(self.status().await)
    }

    pub async fn clear(&self) -> Result<Value, String> {
        self.control
            .send(NetworkControl::Clear)
            .await
            .map_err(|e| e.to_string())?;
        let meta = self.meta_with_warnings(Vec::new()).await;
        Ok(json!({ "cleared": true, "meta": meta }))
    }

    pub async fn status(&self) -> Value {
        let status = self.store.lock().await.status_json();
        json!({
            "meta": self.meta_with_warnings(Vec::new()).await,
            "status": status,
        })
    }

    pub async fn is_active(&self) -> bool {
        self.store.lock().await.active
    }

    pub async fn log(
        &self,
        limit: usize,
        pattern: Option<&str>,
        method: Option<&str>,
        status: Option<u16>,
        since: Option<u64>,
        compact: bool,
    ) -> Value {
        let store = self.store.lock().await;
        let host = pattern.and_then(pattern_exact_host);
        let mut rows: Vec<Value> = store
            .candidate_ids(method, status, None, host.as_deref())
            .into_iter()
            .filter_map(|id| store.entries.get(&id))
            .filter(|entry| since.map(|id| entry.id > id).unwrap_or(true))
            .filter(|entry| pattern.map(|p| url_matches(p, &entry.url)).unwrap_or(true))
            .rev()
            .take(limit)
            .map(|entry| {
                if compact {
                    json!({
                        "id": entry.id,
                        "url": entry.url,
                        "method": entry.method,
                        "status": entry.status,
                        "complete": entry.complete,
                    })
                } else {
                    entry.log_json()
                }
            })
            .collect();
        rows.reverse();
        let next_since = rows
            .iter()
            .filter_map(|entry| entry["id"].as_u64())
            .max()
            .or(since)
            .unwrap_or(0);
        json!({
            "meta": self.meta_from_store(&store, next_since, Vec::new()),
            "count": rows.len(),
            "entries": rows,
            "filters": {
                "limit": limit,
                "pattern": pattern,
                "method": method,
                "status": status,
                "since": since,
                "compact": compact,
            },
        })
    }

    pub async fn show(
        &self,
        id: u64,
        include_body: bool,
        max_body: Option<usize>,
    ) -> Option<Value> {
        let store = self.store.lock().await;
        store.entry_by_id(id).map(|entry| {
            let entry = if include_body {
                entry.full_json_with_body_limit(max_body)
            } else {
                entry.metadata_json()
            };
            json!({
                "meta": self.meta_from_store(&store, id, Vec::new()),
                "entry": entry,
            })
        })
    }

    pub async fn entry_id_for_network(
        &self,
        session_id: Option<&str>,
        request_id: Option<&str>,
        url: &str,
        method: &str,
    ) -> Option<u64> {
        let store = self.store.lock().await;
        if let (Some(session_id), Some(request_id)) = (session_id, request_id) {
            if let Some(id) = store.active_by_key.get(&entry_key(session_id, request_id)) {
                return Some(*id);
            }
            if let Some(entry) = store
                .entries
                .values()
                .rev()
                .find(|entry| entry.session_id == session_id && entry.request_id == request_id)
            {
                return Some(entry.id);
            }
        }
        store
            .entries
            .values()
            .rev()
            .find(|entry| entry.url == url && entry.method.eq_ignore_ascii_case(method))
            .map(|entry| entry.id)
    }

    pub async fn export(
        &self,
        path: &Path,
        format: &str,
        settle_ms: Option<u64>,
    ) -> Result<Value, String> {
        self.settle(settle_ms.unwrap_or(5000)).await;
        let snapshot = {
            let store = self.store.lock().await;
            store.entries.values().cloned().collect::<Vec<_>>()
        };
        let output = match format {
            "har" => build_har(&snapshot),
            "json" => build_json_export(&snapshot),
            other => return Err(format!("Unknown network export format '{}'", other)),
        };
        atomic_write_json(path, &output)?;
        let in_flight = snapshot.iter().filter(|entry| !entry.complete).count();
        let exported = snapshot.len().saturating_sub(in_flight);
        let store = self.store.lock().await;
        let body_pending = store.body_pending;
        let body_omitted = snapshot
            .iter()
            .flat_map(|entry| {
                [
                    entry.request_post_data.as_ref(),
                    entry.response_body.as_ref(),
                ]
            })
            .flatten()
            .filter(|body| body.omitted.is_some())
            .count();
        Ok(json!({
            "meta": self.meta_from_store(&store, self.highest_snapshot_id(&snapshot), Vec::new()),
            "path": path.to_string_lossy(),
            "format": format,
            "entries": snapshot.len(),
            "exported": exported,
            "skipped_in_flight": in_flight,
            "body_pending": body_pending,
            "body_omitted": body_omitted,
            "warnings": [],
        }))
    }

    pub async fn high_water_id(&self) -> u64 {
        self.store.lock().await.next_id
    }

    pub async fn wait(
        &self,
        pattern: Option<&str>,
        method: Option<&str>,
        status: Option<u16>,
        since: Option<u64>,
        include_existing: bool,
        timeout_ms: u64,
    ) -> Option<Value> {
        let floor = if include_existing {
            since.unwrap_or(0)
        } else {
            match since {
                Some(id) => id,
                None => self.high_water_id().await,
            }
        };
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            if let Some(entry) = self.find_match(pattern, method, status, floor).await {
                let next_since = entry["id"].as_u64().unwrap_or(floor);
                return Some(json!({
                    "meta": self.meta_with_next_since(next_since).await,
                    "entry": entry,
                    "matched": true,
                }));
            }
            if !wait_for_notify(&self.notify, deadline).await {
                return None;
            }
        }
    }

    async fn find_match(
        &self,
        pattern: Option<&str>,
        method: Option<&str>,
        status: Option<u16>,
        since: u64,
    ) -> Option<Value> {
        let store = self.store.lock().await;
        let host = pattern.and_then(pattern_exact_host);
        store
            .candidate_ids(method, status, None, host.as_deref())
            .into_iter()
            .filter_map(|id| store.entries.get(&id))
            .find(|entry| {
                entry.id > since
                    && pattern.map(|p| url_matches(p, &entry.url)).unwrap_or(true)
                    && method
                        .map(|m| entry.method.eq_ignore_ascii_case(m))
                        .unwrap_or(true)
                    && status.map(|s| entry.status == Some(s)).unwrap_or(true)
            })
            .map(NetworkEntry::log_json)
    }

    pub async fn settle(&self, timeout_ms: u64) {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            if self.store.lock().await.body_pending == 0 {
                return;
            }
            if !wait_for_notify(&self.notify, deadline).await {
                return;
            }
        }
    }

    pub async fn meta_with_warnings(&self, warnings: Vec<String>) -> Value {
        let store = self.store.lock().await;
        self.meta_from_store(&store, store.next_id, warnings)
    }

    async fn meta_with_next_since(&self, next_since: u64) -> Value {
        let store = self.store.lock().await;
        self.meta_from_store(&store, next_since, Vec::new())
    }

    fn meta_from_store(
        &self,
        store: &NetworkStore,
        next_since: u64,
        warnings: Vec<String>,
    ) -> Value {
        json!({
            "namespace": self.namespace,
            "active": store.active,
            "entry_count": store.entries.len(),
            "in_flight": store.active_by_key.len(),
            "last_id": store.next_id,
            "next_since": next_since,
            "body_pending": store.body_pending,
            "body_failed": store.body_failed,
            "warnings": warnings,
            "suggested_commands": suggested_commands(next_since),
        })
    }

    fn highest_snapshot_id(&self, entries: &[NetworkEntry]) -> u64 {
        entries.iter().map(|entry| entry.id).max().unwrap_or(0)
    }
}

async fn wait_for_notify(notify: &Notify, deadline: Instant) -> bool {
    let now = Instant::now();
    if now >= deadline {
        return false;
    }
    tokio::time::timeout(deadline.saturating_duration_since(now), notify.notified())
        .await
        .is_ok()
}

pub fn url_matches(pattern: &str, url: &str) -> bool {
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
                    return false;
                }
                pos += found + part.len();
            }
            None => return false,
        }
    }
    true
}

fn pattern_exact_host(pattern: &str) -> Option<String> {
    if pattern.contains('*') || pattern.contains('/') {
        return None;
    }
    Some(pattern.to_string())
}

fn suggested_commands(next_since: u64) -> Vec<String> {
    vec![
        format!("eoka network log --since {} --compact", next_since),
        "eoka network show <id> --body".to_string(),
        "eoka network har /tmp/session.har".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use serde_json::Map;

    use super::*;
    use crate::handler::network::body::capture_text_body;

    fn sample_entry() -> NetworkEntry {
        let mut request_headers = Map::new();
        request_headers.insert("Content-Type".into(), json!("application/json"));
        request_headers.insert("Accept".into(), json!("application/json"));

        let mut response_headers = Map::new();
        response_headers.insert("content-type".into(), json!("application/json"));
        response_headers.insert("x-trace".into(), json!("abc123"));

        NetworkEntry {
            id: 7,
            session_id: "session-1".into(),
            target_id: "target-1".into(),
            request_id: "request-1".into(),
            hop: 0,
            url: "https://example.com/api/data?x=1&y=two".into(),
            method: "POST".into(),
            resource_type: Some("Fetch".into()),
            started_at: 2.0,
            wall_time: Some(1_700_000_000.123),
            request_headers,
            request_post_data: Some(BodyCapture {
                bytes: br#"{"hello":"world"}"#.to_vec(),
                base64_encoded: false,
                mime_type: Some("application/json".into()),
                omitted: None,
            }),
            status: Some(201),
            status_text: Some("Created".into()),
            protocol: Some("h2".into()),
            mime_type: Some("application/json".into()),
            response_headers,
            response_headers_text: None,
            request_headers_text: None,
            remote_ip: Some("127.0.0.1".into()),
            remote_port: Some(443),
            encoded_data_length: Some(42.0),
            response_timing: None,
            redirect_url: None,
            response_body: Some(BodyCapture {
                bytes: br#"{"ok":true}"#.to_vec(),
                base64_encoded: false,
                mime_type: Some("application/json".into()),
                omitted: None,
            }),
            finished_at: Some(2.25),
            failed_at: None,
            failure_text: None,
            canceled: false,
            complete: true,
        }
    }

    fn recorder_with_entries(entries: Vec<NetworkEntry>) -> NetworkRecorder {
        let mut store = NetworkStore::default();
        let body_bytes = entries.iter().map(NetworkEntry::body_bytes).sum();
        for mut entry in entries {
            entry.id = 0;
            store.push_entry(entry);
        }
        store.body_bytes = body_bytes;
        let (control, _rx) = mpsc::channel(1);
        NetworkRecorder {
            namespace: "session:test".into(),
            store: Arc::new(Mutex::new(store)),
            control,
            notify: Arc::new(Notify::new()),
        }
    }

    #[test]
    fn glob_matching() {
        assert!(url_matches("*/api/*", "https://example.com/api/data"));
        assert!(url_matches("example.com", "https://example.com/foo"));
        assert!(!url_matches("*/api/*", "https://example.com/static/app.js"));
    }

    #[test]
    fn body_limit_omits_large_bodies() {
        let body = capture_text_body("abcdef", 3, true);

        assert_eq!(body.omitted.as_deref(), Some("too large"));
    }

    #[test]
    fn har_skips_incomplete_entries() {
        let mut entry = sample_entry();
        entry.complete = false;

        assert_eq!(
            build_har(&[entry])["log"]["entries"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
    }

    #[test]
    fn har_exports_request_response_and_eoka_metadata() {
        let har = build_har(&[sample_entry()]);
        let entry = &har["log"]["entries"][0];

        assert_eq!(entry["startedDateTime"], "2023-11-14T22:13:20.123Z");
        assert_eq!(entry["time"], 250.0);
        assert_eq!(entry["request"]["method"], "POST");
        assert_eq!(
            entry["request"]["url"],
            "https://example.com/api/data?x=1&y=two"
        );
        assert_eq!(entry["request"]["postData"]["mimeType"], "application/json");
        assert_eq!(entry["request"]["postData"]["text"], r#"{"hello":"world"}"#);
        assert_eq!(entry["response"]["status"], 201);
        assert_eq!(entry["response"]["statusText"], "Created");
        assert_eq!(entry["response"]["content"]["mimeType"], "application/json");
        assert_eq!(entry["response"]["content"]["text"], r#"{"ok":true}"#);
        assert_eq!(entry["serverIPAddress"], "127.0.0.1");
        assert_eq!(entry["_eoka"]["id"], 7);
        assert_eq!(entry["_eoka"]["resource_type"], "Fetch");
    }

    #[test]
    fn har_exports_query_string_pairs() {
        let har = build_har(&[sample_entry()]);
        let query = har["log"]["entries"][0]["request"]["queryString"]
            .as_array()
            .unwrap();

        assert_eq!(query[0], json!({ "name": "x", "value": "1" }));
        assert_eq!(query[1], json!({ "name": "y", "value": "two" }));
    }

    #[test]
    fn har_marks_binary_response_content_base64() {
        let mut entry = sample_entry();
        entry.response_body = Some(BodyCapture {
            bytes: vec![0, 159, 146, 150],
            base64_encoded: true,
            mime_type: Some("application/octet-stream".into()),
            omitted: None,
        });

        let har = build_har(&[entry]);
        let content = &har["log"]["entries"][0]["response"]["content"];

        assert_eq!(content["encoding"], "base64");
        assert_eq!(content["text"], "AJ+Slg==");
        assert_eq!(content["size"], 4);
    }

    #[test]
    fn har_exports_cookie_redirect_timing_and_sizes() {
        let mut entry = sample_entry();
        entry
            .request_headers
            .insert("Cookie".into(), json!("sid=abc; theme=dark"));
        entry
            .response_headers
            .insert("Set-Cookie".into(), json!("next=one; Path=/"));
        entry.response_headers_text = Some("HTTP/1.1 302 Found\r\nLocation: /next\r\n\r\n".into());
        entry.request_headers_text = Some("POST /api HTTP/1.1\r\nCookie: sid=abc\r\n\r\n".into());
        entry.redirect_url = Some("/next".into());
        entry.response_timing = Some(json!({
            "dnsStart": 1.0,
            "dnsEnd": 2.0,
            "connectStart": 2.0,
            "connectEnd": 4.0,
            "sslStart": 2.5,
            "sslEnd": 3.5,
            "sendStart": 5.0,
            "sendEnd": 6.0,
            "receiveHeadersEnd": 10.0,
        }));

        let har = build_har(&[entry]);
        let entry = &har["log"]["entries"][0];

        assert_eq!(
            entry["request"]["cookies"][0],
            json!({ "name": "sid", "value": "abc" })
        );
        assert_eq!(
            entry["response"]["cookies"][0],
            json!({ "name": "next", "value": "one" })
        );
        assert_eq!(entry["response"]["redirectURL"], "/next");
        assert_eq!(entry["timings"]["dns"], 1.0);
        assert_eq!(entry["timings"]["connect"], 2.0);
        assert!(entry["request"]["headersSize"].as_i64().unwrap() > 0);
        assert!(entry["response"]["headersSize"].as_i64().unwrap() > 0);
    }

    #[test]
    fn body_pool_eviction_preserves_metadata_and_marks_omission() {
        let mut store = NetworkStore {
            config: NetworkConfig {
                body_pool_bytes: 5,
                ..NetworkConfig::default()
            },
            ..NetworkStore::default()
        };
        let entry = sample_entry();
        let body_bytes = entry.body_bytes();

        store.push_entry(entry);
        store.add_body_bytes(body_bytes);

        let entry = store.entry_by_id(1).unwrap();
        assert_eq!(entry.url, "https://example.com/api/data?x=1&y=two");
        assert_eq!(
            entry.request_post_data.as_ref().unwrap().omitted.as_deref(),
            Some("evicted")
        );
        assert_eq!(
            entry.response_body.as_ref().unwrap().omitted.as_deref(),
            Some("evicted")
        );
    }

    #[test]
    fn entry_limit_evicts_oldest_metadata() {
        let mut store = NetworkStore {
            config: NetworkConfig {
                entry_limit: 1,
                ..NetworkConfig::default()
            },
            ..NetworkStore::default()
        };
        let mut first = sample_entry();
        first.request_id = "first".into();
        let mut second = sample_entry();
        second.request_id = "second".into();
        second.url = "https://example.com/api/second".into();

        store.push_entry(first);
        store.push_entry(second);

        assert!(store.entry_by_id(1).is_none());
        assert_eq!(
            store.entry_by_id(2).unwrap().url,
            "https://example.com/api/second"
        );
    }

    #[tokio::test]
    async fn log_filters_since_and_compacts_entries() {
        let mut first = sample_entry();
        first.id = 1;
        first.method = "GET".into();
        first.status = Some(200);
        let mut second = sample_entry();
        second.id = 2;
        second.method = "GET".into();
        second.status = Some(200);
        second.url = "https://example.com/api/second".into();
        let recorder = recorder_with_entries(vec![first, second]);

        let log = recorder
            .log(10, Some("*/api/*"), Some("GET"), Some(200), Some(1), true)
            .await;
        let rows = log["entries"].as_array().unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["id"], 2);
        assert!(rows[0].get("duration_ms").is_none());
        assert_eq!(log["meta"]["next_since"], 2);
    }

    #[tokio::test]
    async fn show_omits_body_until_requested_and_can_truncate() {
        let mut entry = sample_entry();
        entry.id = 1;
        let recorder = recorder_with_entries(vec![entry]);

        let metadata = recorder.show(1, false, None).await.unwrap();
        assert!(metadata["entry"]["response"]["body"].get("text").is_none());

        let full = recorder.show(1, true, Some(4)).await.unwrap();
        assert_eq!(full["entry"]["response"]["body"]["text"], r#"{"ok"#);
        assert_eq!(full["entry"]["response"]["body"]["truncated"], true);
    }

    #[tokio::test]
    async fn wait_ignores_existing_entries_by_default() {
        let mut entry = sample_entry();
        entry.id = 1;
        let recorder = recorder_with_entries(vec![entry]);

        assert!(recorder
            .wait(Some("*/api/*"), None, None, None, false, 1)
            .await
            .is_none());
        assert!(recorder
            .wait(Some("*/api/*"), None, None, None, true, 1)
            .await
            .is_some());
    }

    #[tokio::test]
    async fn wait_wakes_when_new_matching_entry_is_notified() {
        let recorder = recorder_with_entries(Vec::new());
        let waiting = {
            let recorder = recorder.clone();
            tokio::spawn(async move {
                recorder
                    .wait(
                        Some("*/api/*"),
                        Some("POST"),
                        Some(201),
                        Some(0),
                        false,
                        1000,
                    )
                    .await
            })
        };
        let mut entry = sample_entry();
        entry.id = 0;
        {
            let mut store = recorder.store.lock().await;
            store.push_entry(entry);
        }
        recorder.notify.notify_waiters();

        let result = waiting.await.unwrap().unwrap();
        assert_eq!(result["matched"], true);
        assert_eq!(result["entry"]["status"], 201);
    }

    #[test]
    fn status_index_updates_candidates() {
        let mut store = NetworkStore::default();
        let mut entry = sample_entry();
        entry.status = None;
        store.push_entry(entry);

        assert!(store.candidate_ids(None, Some(201), None, None).is_empty());
        store.set_status(1, Some(201));
        assert_eq!(store.candidate_ids(None, Some(201), None, None), vec![1]);
        store.set_status(1, Some(500));
        assert!(store.candidate_ids(None, Some(201), None, None).is_empty());
        assert_eq!(store.candidate_ids(None, Some(500), None, None), vec![1]);
    }

    #[tokio::test]
    async fn export_writes_json_snapshot() {
        let dir =
            std::env::temp_dir().join(format!("eoka-json-export-test-{}", std::process::id()));
        let path = dir.join("capture.json");
        let mut entry = sample_entry();
        entry.id = 1;
        let recorder = recorder_with_entries(vec![entry]);

        let result = recorder.export(&path, "json", Some(0)).await.unwrap();
        let parsed: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();

        assert_eq!(result["format"], "json");
        assert_eq!(result["entries"], 1);
        assert_eq!(parsed["version"], 1);
        assert_eq!(parsed["entries"][0]["id"], 1);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn atomic_har_write_creates_parent_and_valid_json() {
        let dir = std::env::temp_dir().join(format!("eoka-har-test-{}", std::process::id()));
        let path = dir.join("nested").join("capture.har");
        let har = build_har(&[sample_entry()]);

        atomic_write_json(&path, &har).unwrap();

        let parsed: Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).expect("har should parse");
        assert_eq!(parsed["log"]["version"], "1.2");
        assert_eq!(parsed["log"]["entries"].as_array().unwrap().len(), 1);

        let _ = std::fs::remove_dir_all(dir);
    }
}
