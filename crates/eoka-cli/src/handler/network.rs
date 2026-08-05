use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use eoka::cdp::{transport::CdpMessage, Session as CdpSession, Transport};
use serde_json::{json, Map, Value};
use tokio::sync::{broadcast, mpsc, Mutex};

const DEFAULT_BODY_LIMIT: usize = 10 * 1024 * 1024;
const DEFAULT_BODY_POOL_LIMIT: usize = 512 * 1024 * 1024;
const DEFAULT_ENTRY_LIMIT: usize = 10_000;
const DEFAULT_TOTAL_BUFFER_SIZE: usize = 128 * 1024 * 1024;

#[derive(Clone)]
pub struct NetworkConfig {
    pub patterns: Vec<String>,
    pub capture_bodies: bool,
    pub max_body_bytes: usize,
    pub body_pool_bytes: usize,
    pub entry_limit: usize,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            patterns: vec!["http://*".to_string(), "https://*".to_string()],
            capture_bodies: true,
            max_body_bytes: DEFAULT_BODY_LIMIT,
            body_pool_bytes: DEFAULT_BODY_POOL_LIMIT,
            entry_limit: DEFAULT_ENTRY_LIMIT,
        }
    }
}

#[derive(Clone)]
pub struct BodyCapture {
    pub bytes: Vec<u8>,
    pub base64_encoded: bool,
    pub mime_type: Option<String>,
    pub omitted: Option<String>,
}

impl BodyCapture {
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

#[derive(Clone)]
pub struct NetworkEntry {
    pub id: u64,
    pub session_id: String,
    pub target_id: String,
    pub request_id: String,
    pub hop: u32,
    pub url: String,
    pub method: String,
    pub resource_type: Option<String>,
    pub started_at: f64,
    pub wall_time: Option<f64>,
    pub request_headers: Map<String, Value>,
    pub request_post_data: Option<BodyCapture>,
    pub status: Option<u16>,
    pub status_text: Option<String>,
    pub protocol: Option<String>,
    pub mime_type: Option<String>,
    pub response_headers: Map<String, Value>,
    pub response_headers_text: Option<String>,
    pub request_headers_text: Option<String>,
    pub remote_ip: Option<String>,
    pub remote_port: Option<u64>,
    pub encoded_data_length: Option<f64>,
    pub response_body: Option<BodyCapture>,
    pub finished_at: Option<f64>,
    pub failed_at: Option<f64>,
    pub failure_text: Option<String>,
    pub canceled: bool,
    pub complete: bool,
}

impl NetworkEntry {
    fn duration_ms(&self) -> f64 {
        let end = self
            .finished_at
            .or(self.failed_at)
            .unwrap_or(self.started_at);
        ((end - self.started_at) * 1000.0).max(0.0)
    }

    fn body_bytes(&self) -> usize {
        self.request_post_data
            .as_ref()
            .map(BodyCapture::len)
            .unwrap_or(0)
            + self
                .response_body
                .as_ref()
                .map(BodyCapture::len)
                .unwrap_or(0)
    }

    fn log_json(&self) -> Value {
        json!({
            "id": self.id,
            "url": self.url,
            "method": self.method,
            "status": self.status,
            "resource_type": self.resource_type,
            "duration_ms": self.duration_ms(),
            "request_body": body_summary(self.request_post_data.as_ref()),
            "response_body": body_summary(self.response_body.as_ref()),
            "complete": self.complete,
            "failed": self.failure_text,
        })
    }

    fn full_json(&self) -> Value {
        json!({
            "id": self.id,
            "session_id": self.session_id,
            "target_id": self.target_id,
            "request_id": self.request_id,
            "hop": self.hop,
            "url": self.url,
            "method": self.method,
            "resource_type": self.resource_type,
            "started_at": self.wall_time,
            "duration_ms": self.duration_ms(),
            "request": {
                "headers": self.request_headers,
                "headers_text": self.request_headers_text,
                "post_data": body_json(self.request_post_data.as_ref()),
            },
            "response": {
                "status": self.status,
                "status_text": self.status_text,
                "protocol": self.protocol,
                "mime_type": self.mime_type,
                "headers": self.response_headers,
                "headers_text": self.response_headers_text,
                "remote_ip": self.remote_ip,
                "remote_port": self.remote_port,
                "encoded_data_length": self.encoded_data_length,
                "body": body_json(self.response_body.as_ref()),
            },
            "failure": self.failure_text,
            "canceled": self.canceled,
            "complete": self.complete,
        })
    }
}

#[derive(Default)]
pub struct NetworkStore {
    active: bool,
    session_id: Option<String>,
    target_id: Option<String>,
    config: NetworkConfig,
    entries: VecDeque<NetworkEntry>,
    active_by_key: HashMap<String, u64>,
    next_id: u64,
    body_bytes: usize,
    lagged_events: u64,
    dropped_events: u64,
    started_at: Option<u64>,
}

impl NetworkStore {
    fn clear(&mut self) {
        let active = self.active;
        let session_id = self.session_id.clone();
        let target_id = self.target_id.clone();
        let config = self.config.clone();
        self.entries.clear();
        self.active_by_key.clear();
        self.next_id = 0;
        self.body_bytes = 0;
        self.lagged_events = 0;
        self.dropped_events = 0;
        self.active = active;
        self.session_id = session_id;
        self.target_id = target_id;
        self.config = config;
    }

    fn push_entry(&mut self, mut entry: NetworkEntry) {
        self.next_id += 1;
        entry.id = self.next_id;
        self.active_by_key
            .insert(entry_key(&entry.session_id, &entry.request_id), entry.id);
        self.entries.push_back(entry);
        self.enforce_entry_limit();
    }

    fn entry_mut_by_id(&mut self, id: u64) -> Option<&mut NetworkEntry> {
        self.entries.iter_mut().find(|entry| entry.id == id)
    }

    fn entry_by_id(&self, id: u64) -> Option<&NetworkEntry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    fn current_entry_mut(
        &mut self,
        session_id: &str,
        request_id: &str,
    ) -> Option<&mut NetworkEntry> {
        let id = *self.active_by_key.get(&entry_key(session_id, request_id))?;
        self.entry_mut_by_id(id)
    }

    fn finalize_key(&mut self, session_id: &str, request_id: &str) {
        self.active_by_key
            .remove(&entry_key(session_id, request_id));
    }

    fn add_body_bytes(&mut self, bytes: usize) {
        self.body_bytes += bytes;
        self.enforce_body_limit();
    }

    fn enforce_entry_limit(&mut self) {
        while self.entries.len() > self.config.entry_limit {
            let Some(entry) = self.entries.pop_front() else {
                break;
            };
            self.body_bytes = self.body_bytes.saturating_sub(entry.body_bytes());
            self.active_by_key
                .remove(&entry_key(&entry.session_id, &entry.request_id));
        }
    }

    fn enforce_body_limit(&mut self) {
        while self.body_bytes > self.config.body_pool_bytes {
            let Some(entry) = self.entries.iter_mut().find(|entry| entry.body_bytes() > 0) else {
                break;
            };
            if let Some(body) = entry
                .request_post_data
                .as_mut()
                .filter(|body| body.len() > 0)
            {
                self.body_bytes = self.body_bytes.saturating_sub(body.len());
                *body = BodyCapture::omitted("evicted");
                continue;
            }
            if let Some(body) = entry.response_body.as_mut().filter(|body| body.len() > 0) {
                self.body_bytes = self.body_bytes.saturating_sub(body.len());
                *body = BodyCapture::omitted("evicted");
            }
        }
    }

    fn status_json(&self) -> Value {
        json!({
            "active": self.active,
            "session_id": self.session_id,
            "target_id": self.target_id,
            "patterns": self.config.patterns,
            "entry_count": self.entries.len(),
            "in_flight": self.active_by_key.len(),
            "captured_body_bytes": self.body_bytes,
            "body_limit_bytes": self.config.body_pool_bytes,
            "per_body_limit_bytes": self.config.max_body_bytes,
            "lagged_events": self.lagged_events,
            "dropped_events": self.dropped_events,
            "started_at": self.started_at,
        })
    }
}

#[derive(Clone)]
pub struct NetworkRecorder {
    store: Arc<Mutex<NetworkStore>>,
    control: mpsc::Sender<NetworkControl>,
}

impl NetworkRecorder {
    pub fn spawn(transport: Arc<Transport>) -> Self {
        let store = Arc::new(Mutex::new(NetworkStore {
            config: NetworkConfig::default(),
            ..NetworkStore::default()
        }));
        let (control, control_rx) = mpsc::channel(32);
        tokio::spawn(run_recorder(
            store.clone(),
            transport.clone(),
            transport.subscribe(),
            control_rx,
        ));
        Self { store, control }
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
        Ok(json!({ "cleared": true }))
    }

    pub async fn status(&self) -> Value {
        self.store.lock().await.status_json()
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
    ) -> Value {
        let store = self.store.lock().await;
        let mut rows: Vec<Value> = store
            .entries
            .iter()
            .filter(|entry| pattern.map(|p| url_matches(p, &entry.url)).unwrap_or(true))
            .filter(|entry| {
                method
                    .map(|m| entry.method.eq_ignore_ascii_case(m))
                    .unwrap_or(true)
            })
            .filter(|entry| status.map(|s| entry.status == Some(s)).unwrap_or(true))
            .rev()
            .take(limit)
            .map(NetworkEntry::log_json)
            .collect();
        rows.reverse();
        Value::Array(rows)
    }

    pub async fn show(&self, id: u64) -> Option<Value> {
        self.store
            .lock()
            .await
            .entry_by_id(id)
            .map(NetworkEntry::full_json)
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
                .iter()
                .rev()
                .find(|entry| entry.session_id == session_id && entry.request_id == request_id)
            {
                return Some(entry.id);
            }
        }
        store
            .entries
            .iter()
            .rev()
            .find(|entry| entry.url == url && entry.method.eq_ignore_ascii_case(method))
            .map(|entry| entry.id)
    }

    pub async fn save_har(&self, path: &Path) -> Result<Value, String> {
        let snapshot = {
            let store = self.store.lock().await;
            store.entries.iter().cloned().collect::<Vec<_>>()
        };
        let har = build_har(&snapshot);
        atomic_write_json(path, &har)?;
        let in_flight = snapshot.iter().filter(|entry| !entry.complete).count();
        let exported = snapshot.len().saturating_sub(in_flight);
        Ok(json!({
            "path": path.to_string_lossy(),
            "exported": exported,
            "skipped_in_flight": in_flight,
        }))
    }
}

enum NetworkControl {
    Start {
        session_id: String,
        target_id: String,
        config: NetworkConfig,
    },
    SetSession {
        session_id: String,
        target_id: String,
    },
    Stop,
    Clear,
}

async fn run_recorder(
    store: Arc<Mutex<NetworkStore>>,
    transport: Arc<Transport>,
    mut rx: broadcast::Receiver<CdpMessage>,
    mut control_rx: mpsc::Receiver<NetworkControl>,
) {
    loop {
        tokio::select! {
            command = control_rx.recv() => {
                let Some(command) = command else {
                    break;
                };
                apply_control(&store, command).await;
            }
            message = rx.recv() => {
                match message {
                    Ok(CdpMessage::Event { method, params, session_id }) => {
                        handle_network_event(&store, transport.clone(), method, params, session_id).await;
                    }
                    Ok(_) => {}
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        let mut store = store.lock().await;
                        store.lagged_events += n;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}

async fn apply_control(store: &Arc<Mutex<NetworkStore>>, command: NetworkControl) {
    let mut store = store.lock().await;
    match command {
        NetworkControl::Start {
            session_id,
            target_id,
            config,
        } => {
            store.active = true;
            store.session_id = Some(session_id);
            store.target_id = Some(target_id);
            store.config = config;
            if store.started_at.is_none() {
                store.started_at = Some(now_secs());
            }
        }
        NetworkControl::SetSession {
            session_id,
            target_id,
        } => {
            if store.active {
                store.session_id = Some(session_id);
                store.target_id = Some(target_id);
            }
        }
        NetworkControl::Stop => {
            store.active = false;
            store.session_id = None;
            store.target_id = None;
        }
        NetworkControl::Clear => store.clear(),
    }
}

async fn handle_network_event(
    store: &Arc<Mutex<NetworkStore>>,
    transport: Arc<Transport>,
    method: String,
    params: Value,
    session_id: Option<String>,
) {
    let Some(session_id) = session_id else {
        return;
    };
    match method.as_str() {
        "Network.requestWillBeSent" => request_will_be_sent(store, params, session_id).await,
        "Network.responseReceived" => response_received(store, params, session_id).await,
        "Network.requestWillBeSentExtraInfo" => request_extra_info(store, params, session_id).await,
        "Network.responseReceivedExtraInfo" => response_extra_info(store, params, session_id).await,
        "Network.loadingFinished" => loading_finished(store, transport, params, session_id).await,
        "Network.loadingFailed" => loading_failed(store, params, session_id).await,
        _ => {}
    }
}

async fn request_will_be_sent(store: &Arc<Mutex<NetworkStore>>, params: Value, session_id: String) {
    let request_id = match params.get("requestId").and_then(Value::as_str) {
        Some(id) => id.to_string(),
        None => return,
    };
    let request = params.get("request").cloned().unwrap_or_else(|| json!({}));
    let url = request
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let method = request
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("GET")
        .to_string();
    let mut store = store.lock().await;
    if !store.active || store.session_id.as_deref() != Some(session_id.as_str()) {
        return;
    }
    if !store
        .config
        .patterns
        .iter()
        .any(|pattern| url_matches(pattern, &url))
    {
        return;
    }
    if let Some(previous) = store.current_entry_mut(&session_id, &request_id) {
        previous.complete = true;
        previous.finished_at = params.get("timestamp").and_then(Value::as_f64);
    }
    let request_headers = value_object(request.get("headers"));
    let body = request.get("postData").and_then(Value::as_str).map(|data| {
        capture_text_body(
            data,
            store.config.max_body_bytes,
            store.config.capture_bodies,
        )
    });
    let body_bytes = body.as_ref().map(BodyCapture::len).unwrap_or(0);
    let hop = store
        .entries
        .iter()
        .filter(|entry| entry.session_id == session_id && entry.request_id == request_id)
        .count() as u32;
    let entry = NetworkEntry {
        id: 0,
        session_id: session_id.clone(),
        target_id: store.target_id.clone().unwrap_or_default(),
        request_id: request_id.clone(),
        hop,
        url,
        method,
        resource_type: params
            .get("type")
            .and_then(Value::as_str)
            .map(str::to_string),
        started_at: params
            .get("timestamp")
            .and_then(Value::as_f64)
            .unwrap_or_default(),
        wall_time: params.get("wallTime").and_then(Value::as_f64),
        request_headers,
        request_post_data: body,
        status: None,
        status_text: None,
        protocol: None,
        mime_type: None,
        response_headers: Map::new(),
        response_headers_text: None,
        request_headers_text: None,
        remote_ip: None,
        remote_port: None,
        encoded_data_length: None,
        response_body: None,
        finished_at: None,
        failed_at: None,
        failure_text: None,
        canceled: false,
        complete: false,
    };
    store.push_entry(entry);
    store.add_body_bytes(body_bytes);
}

async fn request_extra_info(store: &Arc<Mutex<NetworkStore>>, params: Value, session_id: String) {
    let Some(request_id) = params.get("requestId").and_then(Value::as_str) else {
        return;
    };
    let mut store = store.lock().await;
    if let Some(entry) = store.current_entry_mut(&session_id, request_id) {
        entry.request_headers = value_object(params.get("headers"));
        entry.request_headers_text = params
            .get("headersText")
            .and_then(Value::as_str)
            .map(str::to_string);
    }
}

async fn response_received(store: &Arc<Mutex<NetworkStore>>, params: Value, session_id: String) {
    let Some(request_id) = params.get("requestId").and_then(Value::as_str) else {
        return;
    };
    let response = params.get("response").cloned().unwrap_or_else(|| json!({}));
    let mut store = store.lock().await;
    if let Some(entry) = store.current_entry_mut(&session_id, request_id) {
        entry.status = response
            .get("status")
            .and_then(Value::as_u64)
            .and_then(|value| u16::try_from(value).ok());
        entry.status_text = response
            .get("statusText")
            .and_then(Value::as_str)
            .map(str::to_string);
        entry.protocol = response
            .get("protocol")
            .and_then(Value::as_str)
            .map(str::to_string);
        entry.mime_type = response
            .get("mimeType")
            .and_then(Value::as_str)
            .map(str::to_string);
        entry.response_headers = value_object(response.get("headers"));
        entry.remote_ip = response
            .get("remoteIPAddress")
            .and_then(Value::as_str)
            .map(str::to_string);
        entry.remote_port = response.get("remotePort").and_then(Value::as_u64);
    }
}

async fn response_extra_info(store: &Arc<Mutex<NetworkStore>>, params: Value, session_id: String) {
    let Some(request_id) = params.get("requestId").and_then(Value::as_str) else {
        return;
    };
    let mut store = store.lock().await;
    if let Some(entry) = store.current_entry_mut(&session_id, request_id) {
        if entry.status.is_none() {
            entry.status = params
                .get("statusCode")
                .and_then(Value::as_u64)
                .and_then(|value| u16::try_from(value).ok());
        }
        entry.response_headers = value_object(params.get("headers"));
        entry.response_headers_text = params
            .get("headersText")
            .and_then(Value::as_str)
            .map(str::to_string);
    }
}

async fn loading_finished(
    store: &Arc<Mutex<NetworkStore>>,
    transport: Arc<Transport>,
    params: Value,
    session_id: String,
) {
    let Some(request_id) = params
        .get("requestId")
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        return;
    };
    let entry_id = {
        let mut store = store.lock().await;
        let Some(entry) = store.current_entry_mut(&session_id, &request_id) else {
            return;
        };
        entry.finished_at = params.get("timestamp").and_then(Value::as_f64);
        entry.encoded_data_length = params.get("encodedDataLength").and_then(Value::as_f64);
        entry.complete = true;
        entry.id
    };
    {
        let mut store = store.lock().await;
        store.finalize_key(&session_id, &request_id);
    }
    let capture = { store.lock().await.config.clone() };
    if !capture.capture_bodies {
        return;
    }
    tokio::spawn(capture_response_body(
        store.clone(),
        transport,
        session_id,
        request_id,
        entry_id,
        capture.max_body_bytes,
    ));
}

async fn loading_failed(store: &Arc<Mutex<NetworkStore>>, params: Value, session_id: String) {
    let Some(request_id) = params.get("requestId").and_then(Value::as_str) else {
        return;
    };
    let mut store = store.lock().await;
    if let Some(entry) = store.current_entry_mut(&session_id, request_id) {
        entry.failed_at = params.get("timestamp").and_then(Value::as_f64);
        entry.failure_text = params
            .get("errorText")
            .and_then(Value::as_str)
            .map(str::to_string);
        entry.canceled = params
            .get("canceled")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        entry.complete = true;
        store.finalize_key(&session_id, request_id);
    }
}

async fn capture_response_body(
    store: Arc<Mutex<NetworkStore>>,
    transport: Arc<Transport>,
    session_id: String,
    request_id: String,
    entry_id: u64,
    max_body_bytes: usize,
) {
    let result = transport
        .send_to_session::<_, Value>(
            &session_id,
            "Network.getResponseBody",
            &json!({ "requestId": request_id }),
        )
        .await;
    let body = match result {
        Ok(value) => decode_response_body(value, max_body_bytes),
        Err(error) => BodyCapture::omitted(format!("body unavailable: {}", error)),
    };
    let size = body.len();
    let mut store = store.lock().await;
    if let Some(entry) = store.entry_mut_by_id(entry_id) {
        if body.mime_type.is_none() {
            let mut with_mime = body;
            with_mime.mime_type = entry.mime_type.clone();
            entry.response_body = Some(with_mime);
        } else {
            entry.response_body = Some(body);
        }
        store.add_body_bytes(size);
    }
}

async fn network_enable(session: &CdpSession, config: &NetworkConfig) -> Result<(), String> {
    session
        .send::<_, Value>(
            "Network.enable",
            &json!({
                "maxTotalBufferSize": DEFAULT_TOTAL_BUFFER_SIZE,
                "maxResourceBufferSize": config.max_body_bytes,
                "maxPostDataSize": config.max_body_bytes,
            }),
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
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

fn capture_text_body(data: &str, limit: usize, enabled: bool) -> BodyCapture {
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

fn decode_response_body(value: Value, limit: usize) -> BodyCapture {
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

fn value_object(value: Option<&Value>) -> Map<String, Value> {
    value
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

fn body_summary(body: Option<&BodyCapture>) -> Value {
    match body {
        Some(body) => json!({
            "bytes": body.len(),
            "base64": body.base64_encoded,
            "omitted": body.omitted,
        }),
        None => Value::Null,
    }
}

fn body_json(body: Option<&BodyCapture>) -> Value {
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

fn entry_key(session_id: &str, request_id: &str) -> String {
    format!("{}\n{}", session_id, request_id)
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn build_har(entries: &[NetworkEntry]) -> Value {
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

fn har_entry(entry: &NetworkEntry) -> Value {
    json!({
        "startedDateTime": har_time(entry),
        "time": entry.duration_ms(),
        "request": {
            "method": entry.method,
            "url": entry.url,
            "httpVersion": "HTTP/1.1",
            "cookies": [],
            "headers": har_headers(&entry.request_headers),
            "queryString": har_query(&entry.url),
            "headersSize": -1,
            "bodySize": entry.request_post_data.as_ref().map(|body| body.len() as i64).unwrap_or(0),
            "postData": har_post_data(entry),
        },
        "response": {
            "status": entry.status.unwrap_or(0),
            "statusText": entry.status_text.clone().unwrap_or_default(),
            "httpVersion": entry.protocol.clone().unwrap_or_else(|| "HTTP/1.1".to_string()),
            "cookies": [],
            "headers": har_headers(&entry.response_headers),
            "content": har_content(entry),
            "redirectURL": "",
            "headersSize": -1,
            "bodySize": entry.response_body.as_ref().map(|body| body.len() as i64).unwrap_or(-1),
        },
        "cache": {},
        "timings": {
            "send": 0,
            "wait": entry.duration_ms(),
            "receive": 0,
        },
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

fn har_query(url: &str) -> Vec<Value> {
    let Some(query) = url.split_once('?').map(|(_, query)| query) else {
        return Vec::new();
    };
    query
        .split('&')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let (name, value) = part.split_once('=').unwrap_or((part, ""));
            json!({ "name": name, "value": value })
        })
        .collect()
}

fn har_post_data(entry: &NetworkEntry) -> Value {
    match entry
        .request_post_data
        .as_ref()
        .and_then(BodyCapture::to_text)
    {
        Some(text) => json!({
            "mimeType": entry
                .request_headers
                .get("content-type")
                .or_else(|| entry.request_headers.get("Content-Type"))
                .and_then(Value::as_str)
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
    let text = body.and_then(BodyCapture::to_text).unwrap_or_default();
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

fn atomic_write_json(path: &Path, value: &Value) -> Result<(), String> {
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let entry = NetworkEntry {
            id: 1,
            session_id: "s".into(),
            target_id: "t".into(),
            request_id: "r".into(),
            hop: 0,
            url: "https://example.com/a?x=1".into(),
            method: "GET".into(),
            resource_type: Some("Document".into()),
            started_at: 1.0,
            wall_time: Some(1.0),
            request_headers: Map::new(),
            request_post_data: None,
            status: Some(200),
            status_text: Some("OK".into()),
            protocol: Some("h2".into()),
            mime_type: Some("text/plain".into()),
            response_headers: Map::new(),
            response_headers_text: None,
            request_headers_text: None,
            remote_ip: None,
            remote_port: None,
            encoded_data_length: None,
            response_body: None,
            finished_at: Some(1.1),
            failed_at: None,
            failure_text: None,
            canceled: false,
            complete: false,
        };

        assert_eq!(
            build_har(&[entry])["log"]["entries"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
    }
}
