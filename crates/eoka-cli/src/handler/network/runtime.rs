use std::sync::Arc;

use eoka::cdp::{transport::CdpMessage, Session as CdpSession, Transport};
use serde_json::{json, Map, Value};
use tokio::sync::{broadcast, mpsc, Mutex, Notify, Semaphore};

use super::body::{capture_text_body, decode_response_body, BodyCaptureExt};
use super::{
    now_secs, url_matches, BodyCapture, NetworkConfig, NetworkEntry, NetworkStore,
    DEFAULT_TOTAL_BUFFER_SIZE,
};

pub(super) enum NetworkControl {
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

pub(super) async fn run_recorder(
    store: Arc<Mutex<NetworkStore>>,
    transport: Arc<Transport>,
    mut rx: broadcast::Receiver<CdpMessage>,
    mut control_rx: mpsc::Receiver<NetworkControl>,
    body_permits: Arc<Semaphore>,
    notify: Arc<Notify>,
) {
    loop {
        tokio::select! {
            command = control_rx.recv() => {
                let Some(command) = command else {
                    break;
                };
                apply_control(&store, command, notify.as_ref()).await;
            }
            message = rx.recv() => {
                match message {
                    Ok(CdpMessage::Event { method, params, session_id }) => {
                        handle_network_event(&store, transport.clone(), body_permits.clone(), notify.clone(), method, params, session_id).await;
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

async fn apply_control(store: &Arc<Mutex<NetworkStore>>, command: NetworkControl, notify: &Notify) {
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
    notify.notify_waiters();
}

async fn handle_network_event(
    store: &Arc<Mutex<NetworkStore>>,
    transport: Arc<Transport>,
    body_permits: Arc<Semaphore>,
    notify: Arc<Notify>,
    method: String,
    params: Value,
    session_id: Option<String>,
) {
    let Some(session_id) = session_id else {
        return;
    };
    match method.as_str() {
        "Network.requestWillBeSent" => {
            request_will_be_sent(store, params, session_id, notify.as_ref()).await
        }
        "Network.responseReceived" => {
            response_received(store, params, session_id, notify.as_ref()).await
        }
        "Network.requestWillBeSentExtraInfo" => {
            request_extra_info(store, params, session_id, notify.as_ref()).await
        }
        "Network.responseReceivedExtraInfo" => {
            response_extra_info(store, params, session_id, notify.as_ref()).await
        }
        "Network.loadingFinished" => {
            loading_finished(store, transport, body_permits, notify, params, session_id).await
        }
        "Network.loadingFailed" => loading_failed(store, params, session_id, notify.as_ref()).await,
        _ => {}
    }
}

async fn request_will_be_sent(
    store: &Arc<Mutex<NetworkStore>>,
    params: Value,
    session_id: String,
    notify: &Notify,
) {
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
    let body_bytes = body.as_ref().map(BodyCaptureExt::len).unwrap_or(0);
    let hop = store
        .entries
        .values()
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
        response_timing: None,
        redirect_url: None,
        response_body: None,
        finished_at: None,
        failed_at: None,
        failure_text: None,
        canceled: false,
        complete: false,
    };
    store.push_entry(entry);
    store.add_body_bytes(body_bytes);
    notify.notify_waiters();
}

async fn request_extra_info(
    store: &Arc<Mutex<NetworkStore>>,
    params: Value,
    session_id: String,
    notify: &Notify,
) {
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
        notify.notify_waiters();
    }
}

async fn response_received(
    store: &Arc<Mutex<NetworkStore>>,
    params: Value,
    session_id: String,
    notify: &Notify,
) {
    let Some(request_id) = params.get("requestId").and_then(Value::as_str) else {
        return;
    };
    let response = params.get("response").cloned().unwrap_or_else(|| json!({}));
    let mut store = store.lock().await;
    let entry_id = store
        .current_entry_mut(&session_id, request_id)
        .map(|entry| entry.id);
    if let Some(entry_id) = entry_id {
        let status = response
            .get("status")
            .and_then(Value::as_u64)
            .and_then(|value| u16::try_from(value).ok());
        store.set_status(entry_id, status);
        if let Some(entry) = store.entry_mut_by_id(entry_id) {
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
            entry.response_timing = response.get("timing").cloned();
            entry.redirect_url = response
                .get("headers")
                .and_then(|headers| headers.get("location").or_else(|| headers.get("Location")))
                .and_then(Value::as_str)
                .map(str::to_string);
        }
        notify.notify_waiters();
    }
}

async fn response_extra_info(
    store: &Arc<Mutex<NetworkStore>>,
    params: Value,
    session_id: String,
    notify: &Notify,
) {
    let Some(request_id) = params.get("requestId").and_then(Value::as_str) else {
        return;
    };
    let mut store = store.lock().await;
    let entry_id = store
        .current_entry_mut(&session_id, request_id)
        .map(|entry| entry.id);
    if let Some(entry_id) = entry_id {
        if store
            .entry_by_id(entry_id)
            .and_then(|entry| entry.status)
            .is_none()
        {
            let status = params
                .get("statusCode")
                .and_then(Value::as_u64)
                .and_then(|value| u16::try_from(value).ok());
            store.set_status(entry_id, status);
        }
        if let Some(entry) = store.entry_mut_by_id(entry_id) {
            entry.response_headers = value_object(params.get("headers"));
            entry.response_headers_text = params
                .get("headersText")
                .and_then(Value::as_str)
                .map(str::to_string);
            entry.redirect_url = params
                .get("headers")
                .and_then(|headers| headers.get("location").or_else(|| headers.get("Location")))
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| entry.redirect_url.clone());
        }
        notify.notify_waiters();
    }
}

async fn loading_finished(
    store: &Arc<Mutex<NetworkStore>>,
    transport: Arc<Transport>,
    body_permits: Arc<Semaphore>,
    notify: Arc<Notify>,
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
        let entry_id = entry.id;
        store.finalize_key(&session_id, &request_id);
        entry_id
    };
    notify.notify_waiters();
    let capture = { store.lock().await.config.clone() };
    if !capture.capture_bodies {
        return;
    }
    {
        let mut store = store.lock().await;
        store.body_pending += 1;
    }
    let store = store.clone();
    tokio::spawn(async move {
        let Ok(_permit) = body_permits.acquire_owned().await else {
            let mut store = store.lock().await;
            store.body_pending = store.body_pending.saturating_sub(1);
            store.body_failed += 1;
            notify.notify_waiters();
            return;
        };
        capture_response_body(
            store.clone(),
            transport,
            notify,
            session_id,
            request_id,
            entry_id,
            capture.max_body_bytes,
        )
        .await;
    });
}

async fn loading_failed(
    store: &Arc<Mutex<NetworkStore>>,
    params: Value,
    session_id: String,
    notify: &Notify,
) {
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
        notify.notify_waiters();
    }
}

async fn capture_response_body(
    store: Arc<Mutex<NetworkStore>>,
    transport: Arc<Transport>,
    notify: Arc<Notify>,
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
    store.body_pending = store.body_pending.saturating_sub(1);
    if body.omitted.is_some() {
        store.body_failed += 1;
    }
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
    notify.notify_waiters();
}

pub(super) async fn network_enable(
    session: &CdpSession,
    config: &NetworkConfig,
) -> Result<(), String> {
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

fn value_object(value: Option<&Value>) -> Map<String, Value> {
    value
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}
