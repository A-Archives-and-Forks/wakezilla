#[cfg(not(windows))]
use pnet::datalink;
use std::process::Command;

#[allow(dead_code)]
pub fn get_local_mac_addresses() -> Vec<String> {
    #[cfg(windows)]
    {
        Vec::new()
    }
    #[cfg(not(windows))]
    {
        datalink::interfaces()
            .into_iter()
            .filter_map(|iface| iface.mac.map(|mac| mac.to_string()))
            .collect()
    }
}

#[allow(dead_code)]
pub fn shutdown_machine() {
    tracing::warn!("SHUTTING DOWN THE MACHINE IN 5 SECONDS!");
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(5));
        // Try to execute suspend instead of shutdown for linux systems with systemd
        // and fall back to shutdown if suspend is not supported.
        let status = if cfg!(target_os = "linux") {
            let suspend_command_status = Command::new("systemctl").args(["suspend"]).status();
            match suspend_command_status {
                Ok(s) if s.success() => return,
                _ => Command::new("shutdown").args(["-h", "now"]).status(),
            }
        } else if cfg!(target_os = "macos") {
            Command::new("osascript")
                .args(["-e", "tell app \"System Events\" to shut down"])
                .status()
        } else if cfg!(target_os = "windows") {
            Command::new("shutdown").args(["/h"]).status()
        } else {
            let os_name = std::env::consts::OS;
            tracing::warn!("Unsupported OS for hibernate: {}", os_name);
            Command::new("shutdown").args(["-h", "now"]).status()
        };

        match status {
            Ok(s) if s.success() => (),
            Ok(s) => tracing::warn!("Shutdown command exited with status: {}", s),
            Err(e) => tracing::warn!("Failed to execute shutdown command: {}", e),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_local_mac_addresses_returns_vector() {
        let addrs = get_local_mac_addresses();
        // Ensure any discovered MAC addresses are non-empty strings.
        assert!(addrs.iter().all(|addr| !addr.is_empty()));
    }
}
