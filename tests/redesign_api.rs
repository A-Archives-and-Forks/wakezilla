use std::{collections::HashMap, sync::Arc};

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::RwLock,
};
use tower::ServiceExt;
use wakezilla::{
    config::Config,
    forward::TurnOffLimiter,
    proxy_server::api_routes,
    web::{AppState, Machine},
};
use wakezilla_common::MachineType;

fn state(dir: &TempDir) -> AppState {
    let mut config = Config::default();
    config.storage.machines_db_path = dir
        .path()
        .join("machines.json")
        .to_string_lossy()
        .into_owned();
    AppState {
        machines: Arc::new(RwLock::new(vec![])),
        proxies: Arc::new(RwLock::new(HashMap::new())),
        config: Arc::new(config),
        turn_off_limiter: Arc::new(TurnOffLimiter::new()),
        monitor_handle: Arc::new(std::sync::Mutex::new(None)),
        access_log: Arc::new(RwLock::new(wakezilla::access_log::AccessLog::new(10))),
    }
}

fn machine() -> Machine {
    serde_json::from_value(payload()).expect("valid machine")
}

fn payload() -> Value {
    json!({"name":"NAS", "mac":"02:00:00:00:00:01", "ip":"127.0.0.1",
        "description":null, "turn_off_port":3001, "can_be_turned_off":true,
        "inactivity_period":0, "port_forwards":[]})
}

async fn request(state: AppState, method: &str, path: &str, body: Value) -> (StatusCode, Value) {
    let response = api_routes(state)
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

#[test]
fn records_without_a_machine_type_still_load() {
    assert_eq!(machine().machine_type, MachineType::Server);
}

#[tokio::test]
async fn omitted_inactivity_uses_sixty_minute_default() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(&dir);
    let mut body = payload();
    body.as_object_mut().unwrap().remove("inactivity_period");

    assert_eq!(
        request(state.clone(), "POST", "/api/machines", body)
            .await
            .0,
        StatusCode::CREATED
    );
    assert_eq!(state.machines.read().await[0].inactivity_period, 60);
}

#[tokio::test]
async fn machine_type_survives_persistence_and_updates_from_older_clients() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(&dir);
    let mut body = payload();
    body["machine_type"] = json!("nas");
    assert_eq!(
        request(state.clone(), "POST", "/api/machines", body)
            .await
            .0,
        StatusCode::CREATED
    );
    assert_eq!(
        request(
            state.clone(),
            "PUT",
            "/api/machines/02:00:00:00:00:01",
            payload()
        )
        .await
        .0,
        StatusCode::OK
    );
    let stored = wakezilla::web::load_machines_from_path(dir.path().join("machines.json")).unwrap();
    assert_eq!(stored[0].machine_type, MachineType::Nas);
    let list = request(state.clone(), "GET", "/api/machines", Value::Null)
        .await
        .1;
    assert_eq!(list[0]["machine_type"], "nas");
    assert!(list[0].get("shutdown_auth_key").is_none());
    if let Some(handle) = state.monitor_handle.lock().unwrap().take() {
        handle.abort();
    };
}

#[tokio::test]
async fn invalid_edit_preserves_the_registered_machine() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(&dir);
    state.machines.write().await.push(machine());
    let mut body = payload();
    body["ip"] = json!("invalid");
    assert_eq!(
        request(
            state.clone(),
            "PUT",
            "/api/machines/02:00:00:00:00:01",
            body
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(state.machines.read().await[0].ip.to_string(), "127.0.0.1");
}

#[tokio::test]
async fn failed_storage_write_preserves_machine_on_update_and_delete() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(&dir);
    state.machines.write().await.push(machine());
    std::fs::create_dir(dir.path().join("machines.json")).unwrap();
    let mut body = payload();
    body["name"] = json!("Changed");
    assert_eq!(
        request(
            state.clone(),
            "PUT",
            "/api/machines/02:00:00:00:00:01",
            body
        )
        .await
        .0,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        request(
            state.clone(),
            "DELETE",
            "/api/machines/delete",
            json!({"mac":"02:00:00:00:00:01"})
        )
        .await
        .0,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(state.machines.read().await[0].name, "NAS");
}

#[tokio::test]
async fn zero_inactivity_preserves_client_pairing() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(&dir);
    let mut configured = machine();
    configured.inactivity_period = 30;
    configured.shutdown_auth_key = Some("test-only-key".into());
    configured.shutdown_auth_verified = true;
    state.machines.write().await.push(configured);
    assert_eq!(
        request(
            state.clone(),
            "PUT",
            "/api/machines/02:00:00:00:00:01",
            payload()
        )
        .await
        .0,
        StatusCode::OK
    );
    let setup = request(
        state.clone(),
        "GET",
        "/api/machines/02:00:00:00:00:01/shutdown-setup",
        Value::Null,
    )
    .await
    .1;
    assert_eq!(setup["status"], "verified");
    assert_eq!(state.machines.read().await[0].inactivity_period, 0);
    if let Some(handle) = state.monitor_handle.lock().unwrap().take() {
        handle.abort();
    };
}

#[tokio::test]
async fn unhealthy_client_returns_false_in_a_successful_status_response() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(&dir);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut unhealthy = machine();
    unhealthy.turn_off_port = Some(listener.local_addr().unwrap().port());
    state.machines.write().await.push(unhealthy);
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buffer = [0; 1024];
        let _ = stream.read(&mut buffer).await.unwrap();
        stream.write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
    });
    let (status, body) = request(
        state,
        "GET",
        "/api/machines/02:00:00:00:00:01/is-on",
        Value::Null,
    )
    .await;
    server.await.unwrap();
    assert_eq!((status, body), (StatusCode::OK, json!({"is_on":false})));
}

#[tokio::test]
async fn debug_root_redirects_to_the_request_host_on_the_frontend_port() {
    let dir = tempfile::tempdir().unwrap();
    let response = wakezilla::proxy_server::build_router(state(&dir))
        .oneshot(
            Request::builder()
                .uri("/")
                .header("host", "192.168.1.17:5006")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        response.headers().get("location").unwrap(),
        "http://192.168.1.17:8080"
    );
}
