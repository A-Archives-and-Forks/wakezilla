use std::sync::Mutex;
use std::time::Duration;

use wakezilla::config::Config;

/// Serializes tests that read or mutate the process environment, since env vars
/// are global to the process and tests run concurrently by default.
static ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvGuard {
    keys: Vec<&'static str>,
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl EnvGuard {
    fn set(vars: &[(&'static str, &str)]) -> Self {
        let lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        for (key, value) in vars {
            std::env::set_var(key, value);
        }
        Self {
            keys: vars.iter().map(|(key, _)| *key).collect(),
            _lock: lock,
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for key in &self.keys {
            std::env::remove_var(key);
        }
    }
}

#[test]
fn config_defaults_match_expected_values() {
    let cfg = Config::default();

    assert_eq!(cfg.server.proxy_port, 3000);
    assert_eq!(cfg.server.client_port, 3001);
    assert_eq!(cfg.server.health_timeout_secs, 5);
    assert_eq!(cfg.wol.default_port, 9);
    assert_eq!(cfg.network.scan_duration_secs, 5);
    assert_eq!(cfg.health.check_interval_ms, 30_000);
}

#[test]
fn config_from_env_applies_overrides() {
    let _guard = EnvGuard::set(&[
        ("WAKEZILLA__SERVER__PROXY_PORT", "4444"),
        ("WAKEZILLA__WOL__DEFAULT_BROADCAST_IP", "192.168.1.255"),
        ("WAKEZILLA__HEALTH__CHECK_INTERVAL_MS", "5000"),
    ]);

    let cfg = Config::from_env().expect("config should load from env");

    assert_eq!(cfg.server.proxy_port, 4444);
    assert_eq!(cfg.wol.default_broadcast_ip, "192.168.1.255");
    assert_eq!(cfg.health.check_interval_ms, 5000);
}

#[test]
fn helper_methods_return_expected_durations() {
    let cfg = Config::default();

    assert_eq!(
        cfg.get_default_broadcast_addr(),
        std::net::Ipv4Addr::new(255, 255, 255, 255)
    );
    assert_eq!(cfg.proxy_connect_timeout(), Duration::from_millis(1000));
    assert_eq!(cfg.wol_packet_sleeptime(), Duration::from_millis(50));
    assert_eq!(cfg.network_scan_duration(), Duration::from_secs(5));
    assert_eq!(cfg.network_read_timeout(), Duration::from_secs(2));
    assert_eq!(cfg.health_check_interval(), Duration::from_millis(30_000));
    assert_eq!(cfg.system_shutdown_sleep_duration(), Duration::from_secs(5));
}

#[test]
fn config_save_load_round_trip_preserves_ports() {
    // `load_from` merges `WAKEZILLA__*` env vars, so hold the env lock to avoid
    // interference from tests that set those vars concurrently.
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("config.toml");

    let mut cfg = Config::default();
    cfg.server.proxy_port = 4567;
    cfg.server.client_port = 7654;

    cfg.save_to(&path).expect("save_to writes toml");
    assert!(path.exists(), "config file should be written");

    let loaded = Config::load_from(&path).expect("load_from reads toml");
    assert_eq!(loaded.server.proxy_port, 4567);
    assert_eq!(loaded.server.client_port, 7654);
}

#[test]
fn config_load_from_missing_file_returns_defaults() {
    // `load_from` merges `WAKEZILLA__*` env vars, so hold the env lock to avoid
    // interference from tests that set those vars concurrently.
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("does-not-exist.toml");

    let loaded = Config::load_from(&path).expect("missing file falls back to defaults");
    assert_eq!(loaded.server.proxy_port, 3000);
}
