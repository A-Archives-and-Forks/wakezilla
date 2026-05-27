use crate::models::{DiscoveredDevice, Machine, NetworkInterface, UpdateMachinePayload};
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
    Request::get(&format!("{}/machines/{}", API_BASE.as_str(), mac))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn update_machine(mac: &str, payload: &UpdateMachinePayload) -> Result<(), String> {
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
    let url = if device.is_empty() {
        format!("{}/scan", API_BASE.as_str())
    } else {
        format!("{}/scan?interface={}", API_BASE.as_str(), device)
    };
    Request::get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn turn_off_machine(mac: &str) -> Result<String, String> {
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
    let response = Request::get(&format!("{}/machines/{}/is-on", API_BASE.as_str(), mac))
        .send()
        .await
        .map_err(|e| e.to_string());

    if response.is_err() {
        console_log(&format!(
            "Error checking if machine is online: {}",
            response.err().unwrap()
        ));
        return false;
    }
    let response = response.unwrap();
    match response.status() {
        200 => true,
        _ => false,
    }
}
