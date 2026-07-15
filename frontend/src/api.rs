use crate::models::{
    AccessHistory, DiscoveredDevice, Machine, NetworkInterface, ShutdownSetup, UpdateMachinePayload,
};
use std::sync::LazyLock;

use leptos::leptos_dom::logging::console_log;

use gloo_net::http::Request;
use web_sys::window;

const DEFAULT_API_PORT: u16 = 3000;

static API_BASE: LazyLock<String> = LazyLock::new(compute_api_base);

// Function to get the API base URL dynamically from the current window location
fn compute_api_base() -> String {
    if let Some(window) = window() {
        let location = window.location();
        if let (Ok(protocol), Ok(hostname), Ok(port)) =
            (location.protocol(), location.hostname(), location.port())
        {
            // If the client window location does not include a port, do not include one in the API base.
            if port.is_empty() {
                format!("{}//{}{}", protocol, hostname, "/api")
            } else {
                format!("{}//{}:{}{}", protocol, hostname, DEFAULT_API_PORT, "/api")
            }
        } else {
            // Fallback to default if location properties are not available
            String::from("http://localhost:3000/api")
        }
    } else {
        String::from("http://localhost:3000/api")
    }
}

fn encode_path_segment(segment: &str) -> String {
    let mut encoded = String::with_capacity(segment.len());

    for byte in segment.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                use std::fmt::Write as _;
                let _ = write!(&mut encoded, "%{byte:02X}");
            }
        }
    }

    encoded
}

pub async fn create_machine(machine: Machine) -> Result<(), String> {
    Request::post(&format!("{}/machines", API_BASE.as_str()))
        .json(&machine)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

pub async fn get_details_machine(mac: &str) -> Result<Machine, String> {
    let mac = encode_path_segment(mac);
    Request::get(&format!("{}/machines/{}", API_BASE.as_str(), mac))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn get_access_history(mac: &str) -> Result<AccessHistory, String> {
    let mac = encode_path_segment(mac);
    Request::get(&format!(
        "{}/machines/{}/access-history",
        API_BASE.as_str(),
        mac
    ))
    .send()
    .await
    .map_err(|e| e.to_string())?
    .json()
    .await
    .map_err(|e| e.to_string())
}

pub async fn update_machine(mac: &str, payload: &UpdateMachinePayload) -> Result<(), String> {
    let mac = encode_path_segment(mac);
    Request::put(&format!("{}/machines/{}", API_BASE.as_str(), mac))
        .json(payload)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

pub async fn delete_machine(mac: &str) -> Result<(), String> {
    let payload = serde_json::json!({ "mac": mac });
    Request::delete(&format!("{}/machines/delete", API_BASE.as_str()))
        .json(&payload)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn fetch_machines() -> Result<Vec<Machine>, String> {
    Request::get(&format!("{}/machines", API_BASE.as_str()))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn fetch_interfaces() -> Result<Vec<NetworkInterface>, String> {
    Request::get(&format!("{}/interfaces", API_BASE.as_str()))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn fetch_scan_network(device: String) -> Result<Vec<DiscoveredDevice>, String> {
    let request = Request::get(&format!("{}/scan", API_BASE.as_str()));
    let request = if device.is_empty() {
        request
    } else {
        request.query([("interface", device.as_str())])
    };
    request
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn turn_off_machine(mac: &str) -> Result<String, String> {
    let mac = encode_path_segment(mac);
    let response = Request::post(&format!(
        "{}/machines/{}/remote-turn-off",
        API_BASE.as_str(),
        mac
    ))
    .send()
    .await
    .map_err(|e| e.to_string())?;

    let is_success = response.ok();
    let body: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    let message = body
        .get("message")
        .and_then(|value| value.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| body.to_string());

    if is_success {
        Ok(message)
    } else {
        Err(message)
    }
}

pub async fn wake_machine(mac: &str) -> Result<String, String> {
    let mac = encode_path_segment(mac);
    let response = Request::post(&format!("{}/machines/{}/wake", API_BASE.as_str(), mac))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let is_success = response.ok();
    let body: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    let message = body
        .get("message")
        .and_then(|value| value.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| body.to_string());

    if is_success {
        Ok(message)
    } else {
        Err(message)
    }
}

pub async fn is_machine_online(mac: &str) -> bool {
    let mac = encode_path_segment(mac);
    let response = Request::get(&format!("{}/machines/{}/is-on", API_BASE.as_str(), mac))
        .send()
        .await;

    let response = match response {
        Ok(response) => response,
        Err(err) => {
            console_log(&format!("Error checking if machine is online: {err}"));
            return false;
        }
    };

    response.status() == 200
}

pub async fn get_shutdown_setup(mac: &str) -> Result<ShutdownSetup, String> {
    let mac = encode_path_segment(mac);
    let response = Request::get(&format!(
        "{}/machines/{}/shutdown-setup",
        API_BASE.as_str(),
        mac
    ))
    .send()
    .await
    .map_err(|error| error.to_string())?;
    decode_shutdown_setup_http_response(response).await
}

pub async fn verify_shutdown_setup(mac: &str) -> Result<ShutdownSetup, String> {
    let mac = encode_path_segment(mac);
    let response = Request::post(&format!(
        "{}/machines/{}/shutdown-setup/verify",
        API_BASE.as_str(),
        mac
    ))
    .send()
    .await
    .map_err(|error| error.to_string())?;
    decode_shutdown_setup_http_response(response).await
}

pub async fn rotate_shutdown_key(mac: &str) -> Result<ShutdownSetup, String> {
    let mac = encode_path_segment(mac);
    let response = Request::post(&format!(
        "{}/machines/{}/shutdown-setup/rotate",
        API_BASE.as_str(),
        mac
    ))
    .send()
    .await
    .map_err(|error| error.to_string())?;
    decode_shutdown_setup_http_response(response).await
}

async fn decode_shutdown_setup_http_response(
    response: gloo_net::http::Response,
) -> Result<ShutdownSetup, String> {
    let is_success = response.ok();
    let body = response
        .json::<serde_json::Value>()
        .await
        .map_err(|error| error.to_string())?;
    decode_shutdown_setup_response(is_success, body)
}

fn decode_shutdown_setup_response(
    is_success: bool,
    body: serde_json::Value,
) -> Result<ShutdownSetup, String> {
    if !is_success {
        return Err(body
            .get("error")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| body.to_string()));
    }

    serde_json::from_value(body).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_setup_error_preserves_backend_message() {
        let result = decode_shutdown_setup_response(
            false,
            serde_json::json!({ "error": "Failed to persist shutdown setup" }),
        );

        assert_eq!(result, Err("Failed to persist shutdown setup".to_string()));
    }

    #[test]
    fn shutdown_setup_success_deserializes_the_setup() {
        let result = decode_shutdown_setup_response(
            true,
            serde_json::json!({
                "status": "verified",
                "unix_command": null,
                "windows_command": null
            }),
        )
        .expect("successful response should deserialize");

        assert_eq!(result.status, crate::models::ShutdownSetupStatus::Verified);
    }
}
