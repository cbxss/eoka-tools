use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::Hash;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Map, Value};

use super::body::{body_json, body_summary, limit_body_json, BodyCaptureExt};

const DEFAULT_BODY_LIMIT: usize = 10 * 1024 * 1024;
const DEFAULT_BODY_POOL_LIMIT: usize = 512 * 1024 * 1024;
const DEFAULT_ENTRY_LIMIT: usize = 10_000;
pub(super) const DEFAULT_TOTAL_BUFFER_SIZE: usize = 128 * 1024 * 1024;

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
    pub response_timing: Option<Value>,
    pub redirect_url: Option<String>,
    pub response_body: Option<BodyCapture>,
    pub finished_at: Option<f64>,
    pub failed_at: Option<f64>,
    pub failure_text: Option<String>,
    pub canceled: bool,
    pub complete: bool,
}

impl NetworkEntry {
    pub(super) fn duration_ms(&self) -> f64 {
        let end = self
            .finished_at
            .or(self.failed_at)
            .unwrap_or(self.started_at);
        ((end - self.started_at) * 1000.0).max(0.0)
    }

    pub(super) fn body_bytes(&self) -> usize {
        self.request_post_data
            .as_ref()
            .map(BodyCaptureExt::len)
            .unwrap_or(0)
            + self
                .response_body
                .as_ref()
                .map(BodyCaptureExt::len)
                .unwrap_or(0)
    }

    pub(super) fn log_json(&self) -> Value {
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

    pub(super) fn full_json(&self) -> Value {
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
                "timing": self.response_timing,
                "redirect_url": self.redirect_url,
                "body": body_json(self.response_body.as_ref()),
            },
            "failure": self.failure_text,
            "canceled": self.canceled,
            "complete": self.complete,
        })
    }

    pub(super) fn metadata_json(&self) -> Value {
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
                "post_data": body_summary(self.request_post_data.as_ref()),
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
                "timing": self.response_timing,
                "redirect_url": self.redirect_url,
                "body": body_summary(self.response_body.as_ref()),
            },
            "failure": self.failure_text,
            "canceled": self.canceled,
            "complete": self.complete,
        })
    }

    pub(super) fn full_json_with_body_limit(&self, max_body: Option<usize>) -> Value {
        let mut value = self.full_json();
        if let Some(max_body) = max_body {
            limit_body_json(&mut value["request"]["post_data"], max_body);
            limit_body_json(&mut value["response"]["body"], max_body);
        }
        value
    }
}

#[derive(Default)]
pub struct NetworkStore {
    pub(super) active: bool,
    pub(super) session_id: Option<String>,
    pub(super) target_id: Option<String>,
    pub(super) config: NetworkConfig,
    pub(super) entries: BTreeMap<u64, NetworkEntry>,
    pub(super) active_by_key: HashMap<String, u64>,
    pub(super) by_method: HashMap<String, BTreeSet<u64>>,
    pub(super) by_status: HashMap<u16, BTreeSet<u64>>,
    pub(super) by_url: HashMap<String, BTreeSet<u64>>,
    pub(super) by_host: HashMap<String, BTreeSet<u64>>,
    pub(super) next_id: u64,
    pub(super) body_bytes: usize,
    pub(super) body_pending: usize,
    pub(super) body_failed: usize,
    pub(super) lagged_events: u64,
    pub(super) dropped_events: u64,
    pub(super) started_at: Option<u64>,
}

impl NetworkStore {
    pub(super) fn clear(&mut self) {
        let active = self.active;
        let session_id = self.session_id.clone();
        let target_id = self.target_id.clone();
        let config = self.config.clone();
        self.entries.clear();
        self.active_by_key.clear();
        self.by_method.clear();
        self.by_status.clear();
        self.by_url.clear();
        self.by_host.clear();
        self.next_id = 0;
        self.body_bytes = 0;
        self.body_pending = 0;
        self.body_failed = 0;
        self.lagged_events = 0;
        self.dropped_events = 0;
        self.active = active;
        self.session_id = session_id;
        self.target_id = target_id;
        self.config = config;
    }

    pub(super) fn push_entry(&mut self, mut entry: NetworkEntry) {
        self.next_id += 1;
        entry.id = self.next_id;
        self.active_by_key
            .insert(entry_key(&entry.session_id, &entry.request_id), entry.id);
        self.index_entry(&entry);
        self.entries.insert(entry.id, entry);
        self.enforce_entry_limit();
    }

    pub(super) fn entry_mut_by_id(&mut self, id: u64) -> Option<&mut NetworkEntry> {
        self.entries.get_mut(&id)
    }

    pub(super) fn entry_by_id(&self, id: u64) -> Option<&NetworkEntry> {
        self.entries.get(&id)
    }

    pub(super) fn current_entry_mut(
        &mut self,
        session_id: &str,
        request_id: &str,
    ) -> Option<&mut NetworkEntry> {
        let id = *self.active_by_key.get(&entry_key(session_id, request_id))?;
        self.entry_mut_by_id(id)
    }

    pub(super) fn finalize_key(&mut self, session_id: &str, request_id: &str) {
        self.active_by_key
            .remove(&entry_key(session_id, request_id));
    }

    pub(super) fn set_status(&mut self, id: u64, status: Option<u16>) {
        let previous = self.entries.get(&id).and_then(|entry| entry.status);
        if previous == status {
            return;
        }
        if let Some(previous) = previous {
            remove_index_id(&mut self.by_status, &previous, id);
        }
        if let Some(status) = status {
            self.by_status.entry(status).or_default().insert(id);
        }
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.status = status;
        }
    }

    pub(super) fn candidate_ids(
        &self,
        method: Option<&str>,
        status: Option<u16>,
        exact_url: Option<&str>,
        host: Option<&str>,
    ) -> Vec<u64> {
        let mut sets: Vec<&BTreeSet<u64>> = Vec::new();
        if let Some(method) = method {
            let method = method.to_ascii_uppercase();
            let Some(ids) = self.by_method.get(&method) else {
                return Vec::new();
            };
            sets.push(ids);
        }
        if let Some(status) = status {
            let Some(ids) = self.by_status.get(&status) else {
                return Vec::new();
            };
            sets.push(ids);
        }
        if let Some(url) = exact_url {
            let Some(ids) = self.by_url.get(url) else {
                return Vec::new();
            };
            sets.push(ids);
        }
        if let Some(host) = host {
            let Some(ids) = self.by_host.get(host) else {
                return Vec::new();
            };
            sets.push(ids);
        }
        if sets.is_empty() {
            return self.entries.keys().copied().collect();
        }
        sets.sort_by_key(|ids| ids.len());
        let (smallest, rest) = sets.split_first().expect("sets is not empty");
        smallest
            .iter()
            .copied()
            .filter(|id| rest.iter().all(|ids| ids.contains(id)))
            .collect()
    }

    pub(super) fn add_body_bytes(&mut self, bytes: usize) {
        self.body_bytes += bytes;
        self.enforce_body_limit();
    }

    fn enforce_entry_limit(&mut self) {
        while self.entries.len() > self.config.entry_limit {
            let Some((_id, entry)) = self.entries.pop_first() else {
                break;
            };
            self.body_bytes = self.body_bytes.saturating_sub(entry.body_bytes());
            self.active_by_key
                .remove(&entry_key(&entry.session_id, &entry.request_id));
            self.remove_indexes(&entry);
        }
    }

    fn enforce_body_limit(&mut self) {
        while self.body_bytes > self.config.body_pool_bytes {
            let Some(entry) = self
                .entries
                .values_mut()
                .find(|entry| entry.body_bytes() > 0)
            else {
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

    pub(super) fn status_json(&self) -> Value {
        json!({
            "active": self.active,
            "session_id": self.session_id,
            "target_id": self.target_id,
            "patterns": self.config.patterns,
            "entry_count": self.entries.len(),
            "in_flight": self.active_by_key.len(),
            "captured_body_bytes": self.body_bytes,
            "body_pending": self.body_pending,
            "body_failed": self.body_failed,
            "body_limit_bytes": self.config.body_pool_bytes,
            "per_body_limit_bytes": self.config.max_body_bytes,
            "lagged_events": self.lagged_events,
            "dropped_events": self.dropped_events,
            "started_at": self.started_at,
            "last_id": self.next_id,
            "next_since": self.next_id,
        })
    }

    fn index_entry(&mut self, entry: &NetworkEntry) {
        self.by_method
            .entry(entry.method.to_ascii_uppercase())
            .or_default()
            .insert(entry.id);
        if let Some(status) = entry.status {
            self.by_status.entry(status).or_default().insert(entry.id);
        }
        self.by_url
            .entry(entry.url.clone())
            .or_default()
            .insert(entry.id);
        if let Some(host) = url_host(&entry.url) {
            self.by_host.entry(host).or_default().insert(entry.id);
        }
    }

    fn remove_indexes(&mut self, entry: &NetworkEntry) {
        remove_index_id(
            &mut self.by_method,
            &entry.method.to_ascii_uppercase(),
            entry.id,
        );
        if let Some(status) = entry.status {
            remove_index_id(&mut self.by_status, &status, entry.id);
        }
        remove_index_id(&mut self.by_url, &entry.url, entry.id);
        if let Some(host) = url_host(&entry.url) {
            remove_index_id(&mut self.by_host, &host, entry.id);
        }
    }
}

pub(super) fn entry_key(session_id: &str, request_id: &str) -> String {
    format!("{}\n{}", session_id, request_id)
}

pub(super) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn remove_index_id<K>(index: &mut HashMap<K, BTreeSet<u64>>, key: &K, id: u64)
where
    K: Eq + Hash,
{
    if let Some(ids) = index.get_mut(key) {
        ids.remove(&id);
        if ids.is_empty() {
            index.remove(key);
        }
    }
}

fn url_host(url: &str) -> Option<String> {
    url::Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
}
