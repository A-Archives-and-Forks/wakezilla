//! Configuration management for Wakezilla
//!
//! This module provides a comprehensive configuration system that:
//! - Centralizes all configurable values
//! - Supports environment variables with `serde`
//! - Provides sensible defaults
//! - Validates configuration at runtime

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Default configuration file path for machines database
pub const DEFAULT_MACHINES_DB_PATH: &str = "machines.json";
pub const DEFAULT_ACCESS_HISTORY_PATH: &str = "access_history.json";

/// Main configuration structure for the entire application
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    /// Server configuration
    #[serde(default)]
    pub server: ServerConfig,

    /// Wake-on-LAN configuration
    #[serde(default)]
    pub wol: WolConfig,

    /// Network scanning configuration
    #[serde(default)]
    pub network: NetworkConfig,

    /// File system configuration
    #[serde(default)]
    pub storage: StorageConfig,

    /// Health check configuration
    #[serde(default)]
    pub health: HealthConfig,

    /// Authentication settings for destructive client operations.
    #[serde(default)]
    pub security: SecurityConfig,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct SecurityConfig {
    /// Shared HMAC key used by a client server to authenticate its proxy.
    #[serde(default)]
    pub client_shutdown_key: Option<String>,
}

impl std::fmt::Debug for SecurityConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SecurityConfig")
            .field(
                "client_shutdown_key",
                &self.client_shutdown_key.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
}

impl Config {
    /// Load configuration from environment variables
    // Retained as public env-based loader; used by tests and as library API.
    #[allow(dead_code)]
    pub fn from_env() -> Result<Self, config::ConfigError> {
        config::Config::builder()
            .add_source(
                config::Environment::with_prefix("WAKEZILLA")
                    .separator("__")
                    .try_parsing(true),
            )
            .build()?
            .try_deserialize()
    }
}

/// OS-standard directory for the Wakezilla system config file.
pub fn config_dir() -> PathBuf {
    #[cfg(target_os = "linux")]
    {
        PathBuf::from("/etc/wakezilla")
    }
    #[cfg(target_os = "macos")]
    {
        PathBuf::from("/Library/Application Support/wakezilla")
    }
    #[cfg(target_os = "windows")]
    {
        let base = std::env::var("ProgramData").unwrap_or_else(|_| "C:\\ProgramData".to_string());
        PathBuf::from(base).join("wakezilla")
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        PathBuf::from("/etc/wakezilla")
    }
}

/// OS-standard directory for Wakezilla service data files.
pub fn data_dir() -> PathBuf {
    #[cfg(target_os = "linux")]
    {
        PathBuf::from("/var/lib/wakezilla")
    }
    #[cfg(target_os = "macos")]
    {
        PathBuf::from("/Library/Application Support/wakezilla")
    }
    #[cfg(target_os = "windows")]
    {
        let base = std::env::var("ProgramData").unwrap_or_else(|_| "C:\\ProgramData".to_string());
        PathBuf::from(base).join("wakezilla")
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        PathBuf::from("/var/lib/wakezilla")
    }
}

pub fn data_path(file_name: &str) -> PathBuf {
    data_dir().join(file_name)
}

/// Full path to the system config file.
pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

impl Config {
    /// Serialize this config to a TOML file, creating parent directories.
    pub fn save_to(&self, path: &Path) -> Result<(), anyhow::Error> {
        let toml_str = toml::to_string_pretty(self)?;
        write_secret_file(path, toml_str.as_bytes())
    }

    /// Load config from a TOML file (optional) merged with `WAKEZILLA__*` env vars.
    pub fn load_from(path: &Path) -> Result<Self, config::ConfigError> {
        config::Config::builder()
            .add_source(config::File::from(path).required(false))
            .add_source(
                config::Environment::with_prefix("WAKEZILLA")
                    .separator("__")
                    .try_parsing(true),
            )
            .build()?
            .try_deserialize()
    }

    /// Load config from the OS-standard path, falling back to defaults on error.
    pub fn load() -> Self {
        let path = config_path();
        Self::load_from(&path).unwrap_or_else(|e| {
            tracing::warn!(
                "Failed to load configuration from {}: {} - using defaults",
                path.display(),
                e
            );
            Self::default()
        })
    }
}

pub(crate) fn write_secret_file(path: &Path, contents: &[u8]) -> Result<(), anyhow::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)?;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        file.write_all(contents)?;
        file.sync_all()?;
        Ok(())
    }

    #[cfg(target_os = "windows")]
    {
        write_file_after_security(path, contents, apply_windows_secret_file_security)
    }

    #[cfg(not(any(unix, target_os = "windows")))]
    {
        std::fs::write(path, contents)?;
        Ok(())
    }
}

#[cfg(any(target_os = "windows", test))]
fn write_file_after_security(
    path: &Path,
    contents: &[u8],
    secure: impl FnOnce(&Path) -> Result<(), anyhow::Error>,
) -> Result<(), anyhow::Error> {
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)?;
    secure(path)?;
    file.write_all(contents)?;
    file.sync_all()?;
    Ok(())
}

#[cfg(any(target_os = "windows", test))]
fn windows_secret_file_sddl() -> &'static str {
    "D:P(A;;FA;;;OW)(A;;FA;;;SY)(A;;FA;;;BA)"
}

#[cfg(target_os = "windows")]
fn apply_windows_secret_file_security(path: &Path) -> Result<(), anyhow::Error> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{LocalFree, ERROR_SUCCESS};
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SetNamedSecurityInfoW,
        SDDL_REVISION_1, SE_FILE_OBJECT,
    };
    use windows_sys::Win32::Security::{
        GetSecurityDescriptorDacl, DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
    };

    let mut wide_sddl: Vec<u16> = std::ffi::OsStr::new(windows_secret_file_sddl())
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut descriptor = std::ptr::null_mut();
    let converted = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            wide_sddl.as_mut_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    };
    if converted == 0 || descriptor.is_null() {
        return Err(std::io::Error::last_os_error().into());
    }

    let mut present = 0;
    let mut dacl = std::ptr::null_mut();
    let mut defaulted = 0;
    let dacl_ok = unsafe {
        GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted) != 0
            && present != 0
            && !dacl.is_null()
    };
    if !dacl_ok {
        unsafe { LocalFree(descriptor.cast()) };
        return Err(std::io::Error::last_os_error().into());
    }

    let mut wide_path: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let status = unsafe {
        SetNamedSecurityInfoW(
            wide_path.as_mut_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            dacl,
            std::ptr::null(),
        )
    };
    unsafe { LocalFree(descriptor.cast()) };
    if status != ERROR_SUCCESS {
        anyhow::bail!(
            "failed to protect secret file {} with error {}",
            path.display(),
            status
        );
    }
    Ok(())
}

#[cfg(test)]
mod secret_file_tests {
    #[test]
    fn windows_secret_acl_allows_owner_system_and_administrators_only() {
        assert_eq!(
            super::windows_secret_file_sddl(),
            "D:P(A;;FA;;;OW)(A;;FA;;;SY)(A;;FA;;;BA)"
        );
    }

    #[test]
    fn security_is_applied_before_secret_contents_are_written() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("secret");
        let mut security_applied = false;

        super::write_file_after_security(&path, b"secret", |secured_path| {
            security_applied = true;
            assert_eq!(std::fs::read(secured_path).expect("empty file"), b"");
            Ok(())
        })
        .expect("secret write should succeed");

        assert!(security_applied);
        assert_eq!(std::fs::read(path).expect("secret file"), b"secret");
    }
}

/// Server-related configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Port for the proxy server (default: 3000)
    #[serde(default = "default_proxy_port")]
    pub proxy_port: u16,

    /// Port for the client server (default: 3001)
    #[serde(default = "default_client_port")]
    pub client_port: u16,

    /// HTTP health check timeout in seconds (default: 5)
    #[serde(default = "default_health_timeout_secs")]
    pub health_timeout_secs: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            proxy_port: default_proxy_port(),
            client_port: default_client_port(),
            health_timeout_secs: default_health_timeout_secs(),
        }
    }
}

/// Wake-on-LAN specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WolConfig {
    /// Default WOL port (default: 9)
    #[serde(default = "default_wol_port")]
    pub default_port: u16,

    /// Default broadcast IP (default: 255.255.255.255)
    #[serde(default = "default_broadcast_ip")]
    pub default_broadcast_ip: String,

    /// Default number of WOL packets to send (default: 3)
    #[serde(default = "default_wol_packet_count")]
    pub default_packet_count: u32,

    /// Sleep interval between WOL packets in milliseconds (default: 50)
    #[serde(default = "default_wol_packet_sleeptime_ms")]
    pub packet_sleeptime_ms: u64,

    /// Default wait time for WOL in seconds (default: 90)
    #[serde(default = "default_wol_wait_secs")]
    pub default_wait_secs: u64,

    /// Default poll interval between checks in milliseconds (default: 1000)
    #[serde(default = "default_wol_poll_interval_ms")]
    pub default_poll_interval_ms: u64,

    /// Default TCP connect timeout in milliseconds (default: 700)
    #[serde(default = "default_wol_connect_timeout_ms")]
    pub default_connect_timeout_ms: u64,
}

impl Default for WolConfig {
    fn default() -> Self {
        Self {
            default_port: default_wol_port(),
            default_broadcast_ip: default_broadcast_ip(),
            default_packet_count: default_wol_packet_count(),
            packet_sleeptime_ms: default_wol_packet_sleeptime_ms(),
            default_wait_secs: default_wol_wait_secs(),
            default_poll_interval_ms: default_wol_poll_interval_ms(),
            default_connect_timeout_ms: default_wol_connect_timeout_ms(),
        }
    }
}

/// Network scanning configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Network scanning duration in seconds (default: 5)
    #[serde(default = "default_network_scan_duration_secs")]
    pub scan_duration_secs: u64,

    /// Network read timeout in seconds (default: 2)
    #[serde(default = "default_network_read_timeout_secs")]
    pub read_timeout_secs: u64,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            scan_duration_secs: default_network_scan_duration_secs(),
            read_timeout_secs: default_network_read_timeout_secs(),
        }
    }
}

/// File system and storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Path to the machines database file (default: "machines.json")
    #[serde(default = "default_machines_db_path")]
    pub machines_db_path: String,

    /// Path to the access-history database file (default: "access_history.json")
    #[serde(default = "default_access_history_path")]
    pub access_history_path: String,

    /// Maximum access-history records kept per service (default: 2000)
    #[serde(default = "default_max_access_records")]
    pub max_access_records: usize,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            machines_db_path: default_machines_db_path(),
            access_history_path: default_access_history_path(),
            max_access_records: default_max_access_records(),
        }
    }
}

/// Health check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthConfig {
    /// Health check interval in milliseconds (default: 30000)
    #[serde(default = "default_health_check_interval_ms")]
    pub check_interval_ms: u64,

    /// Proxy connect timeout in milliseconds (default: 1000)
    #[serde(default = "default_proxy_connect_timeout_ms")]
    pub proxy_connect_timeout_ms: u64,

    /// Proxy WOL wait time in seconds (default: 60)
    #[serde(default = "default_proxy_wol_wait_secs")]
    pub proxy_wol_wait_secs: u64,

    /// System shutdown sleep time in seconds (default: 5)
    #[serde(default = "default_system_shutdown_sleep_secs")]
    pub system_shutdown_sleep_secs: u64,

    /// Rate limiting sampling interval in seconds (default: 1)
    #[serde(default = "default_rate_limit_sample_interval_secs")]
    pub rate_limit_sample_interval_secs: u64,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            check_interval_ms: default_health_check_interval_ms(),
            proxy_connect_timeout_ms: default_proxy_connect_timeout_ms(),
            proxy_wol_wait_secs: default_proxy_wol_wait_secs(),
            system_shutdown_sleep_secs: default_system_shutdown_sleep_secs(),
            rate_limit_sample_interval_secs: default_rate_limit_sample_interval_secs(),
        }
    }
}

// Default value functions for serde

fn default_proxy_port() -> u16 {
    3000
}
fn default_client_port() -> u16 {
    3001
}
fn default_health_timeout_secs() -> u64 {
    5
}
fn default_wol_port() -> u16 {
    9
}
fn default_broadcast_ip() -> String {
    "255.255.255.255".into()
}
fn default_wol_packet_count() -> u32 {
    3
}
fn default_wol_packet_sleeptime_ms() -> u64 {
    50
}
fn default_wol_wait_secs() -> u64 {
    90
}
fn default_wol_poll_interval_ms() -> u64 {
    1000
}
fn default_wol_connect_timeout_ms() -> u64 {
    700
}
fn default_network_scan_duration_secs() -> u64 {
    5
}
fn default_network_read_timeout_secs() -> u64 {
    2
}
fn default_machines_db_path() -> String {
    DEFAULT_MACHINES_DB_PATH.into()
}
fn default_access_history_path() -> String {
    DEFAULT_ACCESS_HISTORY_PATH.into()
}
fn default_max_access_records() -> usize {
    2000
}
fn default_health_check_interval_ms() -> u64 {
    30000
}
fn default_proxy_connect_timeout_ms() -> u64 {
    1000
}
fn default_proxy_wol_wait_secs() -> u64 {
    60
}
fn default_system_shutdown_sleep_secs() -> u64 {
    5
}
fn default_rate_limit_sample_interval_secs() -> u64 {
    1
}

/// Convenience functions to get commonly used values
#[allow(dead_code)]
impl Config {
    /// Get the default WOL broadcast address as Ipv4Addr
    pub fn get_default_broadcast_addr(&self) -> std::net::Ipv4Addr {
        self.wol
            .default_broadcast_ip
            .parse()
            .unwrap_or_else(|_| std::net::Ipv4Addr::new(255, 255, 255, 255))
    }

    /// Get proxy connect timeout as Duration
    pub fn proxy_connect_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.health.proxy_connect_timeout_ms)
    }

    /// Get WOL packet sleep duration as Duration
    pub fn wol_packet_sleeptime(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.wol.packet_sleeptime_ms)
    }

    /// Get network scan duration as Duration
    pub fn network_scan_duration(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.network.scan_duration_secs)
    }

    /// Get network read timeout as Duration
    pub fn network_read_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.network.read_timeout_secs)
    }

    /// Get health check interval as Duration
    pub fn health_check_interval(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.health.check_interval_ms)
    }

    /// Get system shutdown sleep duration as Duration
    pub fn system_shutdown_sleep_duration(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.health.system_shutdown_sleep_secs)
    }
}
