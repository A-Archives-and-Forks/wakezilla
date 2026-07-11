use anyhow::{Context, Result};
use clap::Parser;
use std::io::Write;
use std::path::PathBuf;
use tracing::{error, info, instrument};
use tracing_subscriber::fmt::MakeWriter;

mod access_log;
mod api_models;
mod cli;
mod client_server;
mod config;
mod forward;
mod proxy_server;
mod scanner;
mod service;
mod setup;
mod system;
#[cfg(test)]
mod test_support;
mod tray;
mod tui;
mod update;
mod web;
mod wol;

pub use api_models::*;
use cli::{handle_send_command, should_check_for_updates, Cli, Commands};

#[tokio::main]
#[instrument(name = "wakezilla_main", skip_all)]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    init_tracing(tracing_log_path(&cli));

    if should_check_for_updates(&cli) {
        if let Err(e) = update::warn_if_update_available(env!("CARGO_PKG_VERSION")).await {
            tracing::warn!("Update check failed: {e}");
        }
    }

    match cli.command {
        Commands::Tui(args) => {
            tui::run(tui::TuiConfig {
                api_base_url: args.api_url,
            })
            .await?;
        }
        Commands::Tray(_) => {
            tray::run()?;
        }
        Commands::Send(args) => {
            let config = config::Config::load();
            log_config(&config);
            handle_send_command(args, &config).await?;
        }
        Commands::ProxyServer(_args) => {
            let config = config::Config::load();
            log_config(&config);
            if let Err(e) = proxy_server::start(config.clone()).await {
                error!("Proxy server error: {}", e);
                std::process::exit(1);
            }
        }
        Commands::ClientServer(_args) => {
            let config = config::Config::load();
            log_config(&config);
            if let Err(e) = client_server::start(config.server.client_port).await {
                error!("Client server error: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Setup(args) => {
            if let Err(e) = setup::run(args) {
                error!("Setup error: {e:#}");
                std::process::exit(1);
            }
        }
        Commands::Uninstall => {
            if let Err(e) = setup::run_uninstall() {
                error!("Uninstall error: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Service(args) => {
            if let Err(e) = setup::run_service(args) {
                error!("Service error: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Update(args) => {
            update::run_update(update::UpdateRequest {
                version: args.version,
            })
            .await?;
        }
        Commands::WindowsService(args) => {
            let mode = service::Mode::from_str_opt(&args.mode)
                .with_context(|| format!("invalid Windows service mode: {}", args.mode))?;
            service::run_windows_service(mode)?;
        }
    }

    Ok(())
}

fn tracing_log_path(cli: &Cli) -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        if let Commands::WindowsService(args) = &cli.command {
            return service::Mode::from_str_opt(&args.mode).map(service::service_log_path);
        }
    }
    let _ = cli;
    None
}

#[derive(Clone)]
struct TracingWriter {
    log_path: Option<PathBuf>,
}

enum TracingOutput {
    File(std::fs::File),
    Stderr(std::io::Stderr),
}

impl Write for TracingOutput {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            TracingOutput::File(file) => file.write(buf),
            TracingOutput::Stderr(stderr) => stderr.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            TracingOutput::File(file) => file.flush(),
            TracingOutput::Stderr(stderr) => stderr.flush(),
        }
    }
}

impl<'a> MakeWriter<'a> for TracingWriter {
    type Writer = TracingOutput;

    fn make_writer(&'a self) -> Self::Writer {
        let Some(path) = &self.log_path else {
            return TracingOutput::Stderr(std::io::stderr());
        };

        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map(TracingOutput::File)
            .unwrap_or_else(|_| TracingOutput::Stderr(std::io::stderr()))
    }
}

fn init_tracing(log_path: Option<PathBuf>) {
    let env_filter =
        tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());

    tracing_subscriber::fmt()
        .with_writer(TracingWriter { log_path })
        .with_env_filter(env_filter)
        .init();
}

fn log_config(config: &config::Config) {
    info!(
        "Using configuration: server_proxy_port={}, server_client_port={}, wol_default_port={}, machines_db_path={}",
        config.server.proxy_port, config.server.client_port, config.wol.default_port, config.storage.machines_db_path
    );
}
