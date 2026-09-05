use crate::models::{
    AccessHistory, DiscoveredDevice, Machine, NetworkInterface, ShutdownSetup, UpdateMachinePayload,
};
use gloo_net::http::{Request, Response};
use serde::de::DeserializeOwned;
use serde_json::Value;

const API_BASE: &str = "/api";

fn encode_path_segment(segment: &str) -> String {
    let mut encoded = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(byte as char)
            }
            _ => {
                use std::fmt::Write as _;
                let _ = write!(&mut encoded, "%{byte:02X}");
            }
        }
    }
    encoded
}

fn machine_url(mac: &str, action: &str) -> String {
    format!("{API_BASE}/machines/{}{action}", encode_path_segment(mac))
}

fn response_error(status: u16, body: &Value) -> String {
    if let Some(errors) = body.get("errors").and_then(Value::as_object) {
        return errors
            .iter()
            .map(|(field, messages)| {
                let label = match field.as_str() {
                    "name" => "Name",
                    "mac" => "MAC address",
                    "ip" => "IP address",
                    "turn_off_port" => "Client port",
                    "port_forwards" => "Services",
                    _ => field,
                };
                let messages = messages
                    .as_array()
                    .map(|values| {
                        values
                            .iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                format!("{label}: {messages}")
            })
            .collect::<Vec<_>>()
            .join(". ");
    }
    body.get("error")
        .or_else(|| body.get("message"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("The operation could not be completed (HTTP {status})."))
}

async fn decode_response<T: DeserializeOwned>(response: Response) -> Result<T, String> {
    let status = response.status();
    let success = response.ok();
    let body = response
        .json::<Value>()
        .await
        .map_err(|_| format!("Invalid server response (HTTP {status})."))?;
    if !success {
        return Err(response_error(status, &body));
    }
    serde_json::from_value(body).map_err(|_| "The server returned unexpected data.".into())
}

pub async fn create_machine(machine: Machine) -> Result<(), String> {
    let response = Request::post(&format!("{API_BASE}/machines"))
        .json(&machine)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    decode_response::<Value>(response).await.map(|_| ())
}

pub async fn get_details_machine(mac: &str) -> Result<Machine, String> {
    let response = Request::get(&machine_url(mac, ""))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    decode_response(response).await
}

pub async fn get_access_history(mac: &str) -> Result<AccessHistory, String> {
    let response = Request::get(&machine_url(mac, "/access-history"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    decode_response(response).await
}

pub async fn update_machine(mac: &str, payload: &UpdateMachinePayload) -> Result<(), String> {
    let response = Request::put(&machine_url(mac, ""))
        .json(payload)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    decode_response::<Value>(response).await.map(|_| ())
}

pub async fn delete_machine(mac: &str) -> Result<(), String> {
    let response = Request::delete(&format!("{API_BASE}/machines/delete"))
        .json(&serde_json::json!({"mac":mac}))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    decode_response::<Value>(response).await.map(|_| ())
}

pub async fn fetch_machines() -> Result<Vec<Machine>, String> {
    let response = Request::get(&format!("{API_BASE}/machines"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    decode_response(response).await
}

pub async fn fetch_interfaces() -> Result<Vec<NetworkInterface>, String> {
    let response = Request::get(&format!("{API_BASE}/interfaces"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    decode_response(response).await
}

pub async fn fetch_scan_network(device: String) -> Result<Vec<DiscoveredDevice>, String> {
    let request = Request::get(&format!("{API_BASE}/scan"));
    let request = if device.is_empty() {
        request
    } else {
        request.query([("interface", device.as_str())])
    };
    decode_response(request.send().await.map_err(|e| e.to_string())?).await
}

async fn machine_action(mac: &str, action: &str) -> Result<String, String> {
    let response = Request::post(&machine_url(mac, action))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let body: Value = decode_response(response).await?;
    Ok(body
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("Command sent.")
        .to_owned())
}

pub async fn turn_off_machine(mac: &str) -> Result<String, String> {
    machine_action(mac, "/remote-turn-off").await
}
pub async fn wake_machine(mac: &str) -> Result<String, String> {
    machine_action(mac, "/wake").await
}

#[derive(serde::Deserialize)]
struct MachineStatus {
    is_on: bool,
}

pub async fn get_machine_status(mac: &str) -> Result<bool, String> {
    let response = Request::get(&machine_url(mac, "/is-on"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    Ok(decode_response::<MachineStatus>(response).await?.is_on)
}

pub async fn get_shutdown_setup(mac: &str) -> Result<ShutdownSetup, String> {
    let response = Request::get(&machine_url(mac, "/shutdown-setup"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    decode_response(response).await
}

pub async fn verify_shutdown_setup(mac: &str) -> Result<ShutdownSetup, String> {
    let response = Request::post(&machine_url(mac, "/shutdown-setup/verify"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    decode_response(response).await
}

pub async fn rotate_shutdown_key(mac: &str) -> Result<ShutdownSetup, String> {
    let response = Request::post(&machine_url(mac, "/shutdown-setup/rotate"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    decode_response(response).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn machine_urls_use_current_origin_and_escape_identifiers() {
        assert_eq!(
            machine_url("AA:BB/CC", "/wake"),
            "/api/machines/AA%3ABB%2FCC/wake"
        );
    }
    #[test]
    fn offline_json_is_not_confused_with_http_success() {
        let status: MachineStatus =
            serde_json::from_value(serde_json::json!({"is_on":false})).unwrap();
        assert!(!status.is_on);
    }
    #[test]
    fn api_errors_preserve_actionable_server_details() {
        assert_eq!(
            response_error(500, &serde_json::json!({"error":"Failed to save machines"})),
            "Failed to save machines"
        );
        assert!(
            response_error(
                400,
                &serde_json::json!({"errors":{"mac":["Invalid MAC address"]}})
            )
            .contains("MAC address")
        );
    }
}
