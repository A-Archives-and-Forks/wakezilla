//! Interactive `setup` wizard: configure and install a Wakezilla system service.

use anyhow::{Context, Result};
use clap::Parser;

use crate::config::{self, Config};
use crate::service::{self, Mode};

/// CLI arguments for the `setup` subcommand.
#[derive(Parser, Debug, Default)]
#[command()]
pub struct SetupArgs {
    /// Pre-select the mode ("proxy" or "client"); skips the TUI prompt if combined with --port.
    #[arg(long, help_heading = "Setup Options")]
    pub mode: Option<String>,

    /// Pre-select the port; skips the TUI prompt if combined with --mode.
    #[arg(long, help_heading = "Setup Options")]
    pub port: Option<u16>,
}

/// Build a Config with the chosen port placed in the correct field for `mode`.
pub fn build_config(mode: Mode, port: u16) -> Config {
    let mut cfg = Config::default();
    match mode {
        Mode::Proxy => cfg.server.proxy_port = port,
        Mode::Client => cfg.server.client_port = port,
    }
    cfg
}

/// Write the config file, install the service, and validate it.
/// Returns the config path on success.
pub fn apply(mode: Mode, port: u16) -> Result<std::path::PathBuf> {
    let exe = std::env::current_exe().context("failed to resolve current executable path")?;
    let exe = exe.to_string_lossy().to_string();

    let cfg = build_config(mode, port);
    let path = config::config_path();
    cfg.save_to(&path)
        .with_context(|| format!("failed to write config to {}", path.display()))?;

    service::install(mode, &exe).context("failed to install system service")?;
    service::validate(port, 10).context("service installed but did not become reachable")?;

    Ok(path)
}
