use crate::setup::{ServiceArgs, SetupArgs};
use crate::{config, wol};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::net::{IpAddr, Ipv4Addr};
use tracing::{info, instrument};

#[derive(Parser, Debug)]
#[command()]
pub struct SendArgs {
    /// Target MAC address (formats: 00:11:22:33:44:55 or 001122334455, etc.)
    pub mac: String,

    /// Broadcast IP to use (default 255.255.255.255)
    #[arg(short, long)]
    pub broadcast: Option<Ipv4Addr>,

    /// UDP port (common: 9 or 7). Default: 9
    #[arg(short, long, default_value_t = 9)]
    pub port: u16,

    /// Number of times to send the packet (helps with flaky networks)
    #[arg(short = 'n', long, default_value_t = 3)]
    pub count: u32,

    /// Optional: IP/host to check after WOL (e.g., 192.168.0.200)
    #[arg(long, value_name = "IP")]
    pub check_ip: Option<IpAddr>,

    /// Optional: TCP port to check on the target host (default 22)
    #[arg(long, default_value_t = 22)]
    pub check_tcp_port: u16,

    /// Max time to wait (seconds) for the host to come up
    #[arg(long, default_value_t = 90)]
    pub wait_secs: u64,

    /// Poll interval (milliseconds) between checks
    #[arg(long, default_value_t = 1000)]
    pub interval_ms: u64,

    /// Per-attempt TCP connect timeout (milliseconds)
    #[arg(long, default_value_t = 700)]
    pub connect_timeout_ms: u64,
}

#[derive(Parser, Debug)]
#[command()]
pub struct ServeArgs {
    /// Port to listen on for the web server
    #[arg(
        short,
        long,
        default_value_t = 3000,
        help_heading = "Proxy Server Options"
    )]
    pub port: u16,
}

#[derive(Parser, Debug)]
#[command()]
pub struct UpdateArgs {
    /// Version to install, without leading `v`. Defaults to the latest release.
    #[arg(long, help_heading = "Update Options")]
    pub version: Option<String>,
}

#[derive(Parser, Debug)]
#[command()]
pub struct TuiArgs {
    /// Base URL for the Wakezilla proxy server API
    #[arg(
        long,
        default_value = "http://127.0.0.1:3000",
        help_heading = "TUI Options"
    )]
    pub api_url: String,
}

#[derive(Parser, Debug)]
#[command()]
pub struct ClientServerArgs {
    /// Port to listen on for the client server
    #[arg(
        short,
        long,
        default_value_t = 3001,
        help_heading = "Client Server Options"
    )]
    pub port: u16,
}

#[derive(Parser, Debug)]
#[command(hide = true)]
pub struct WindowsServiceArgs {
    /// Service mode to run: proxy or client.
    pub mode: String,
}

#[derive(Parser, Debug)]
#[command()]
pub struct TrayArgs {}

/// Simple Wake-on-LAN sender + post-WOL reachability check.
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Cli {
    /// Skip the automatic startup check for a newer Wakezilla release.
    #[arg(long, global = true, help_heading = "Global Options")]
    pub no_update_check: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Send WOL packet via CLI
    Send(SendArgs),
    /// Start proxy server
    ProxyServer(ServeArgs),
    /// Start a client server
    ClientServer(ClientServerArgs),
    /// Start the terminal UI against a running proxy server
    Tui(TuiArgs),
    /// Start the desktop system tray menu
    Tray(TrayArgs),
    /// Configure this host to auto-start a Wakezilla server as a system service
    Setup(SetupArgs),
    /// Remove Wakezilla services installed by setup
    Uninstall,
    /// Control an installed Wakezilla service (start/stop/restart/status/logs)
    Service(ServiceArgs),
    /// Download and install a Wakezilla release
    Update(UpdateArgs),
    /// Internal Windows Service Manager entrypoint.
    #[command(name = "windows-service", hide = true)]
    WindowsService(WindowsServiceArgs),
}

pub fn should_check_for_updates(cli: &Cli) -> bool {
    if cli.no_update_check {
        return false;
    }

    !matches!(
        cli.command,
        Commands::Setup(_)
            | Commands::Uninstall
            | Commands::Update(_)
            | Commands::Tray(_)
            | Commands::WindowsService(_)
    )
}

fn send_broadcast_addr(args: &SendArgs, config: &config::Config) -> Ipv4Addr {
    args.broadcast
        .unwrap_or_else(|| config.get_default_broadcast_addr())
}

#[instrument(name = "handle_send_command", skip(args, config))]
pub async fn handle_send_command(args: SendArgs, config: &config::Config) -> Result<()> {
    info!("Processing WOL send command");

    let mac = wol::parse_mac(&args.mac).context("Failed to parse MAC address")?;
    let bcast = send_broadcast_addr(&args, config);

    wol::send_packets(&mac, bcast, args.port, args.count, config)
        .await
        .context("Failed to send WOL packets")?;

    info!(
        "Sent WOL magic packet to {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} via {}:{}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5], bcast, args.port
    );

    // ---- Optional post-WOL reachability check ----
    if let Some(ip) = args.check_ip {
        info!("Performing post-WOL reachability check for {}", ip);
        if !wol::check_host(
            ip,
            args.check_tcp_port,
            args.wait_secs,
            args.interval_ms,
            args.connect_timeout_ms,
            config,
        )
        .await
        {
            anyhow::bail!(
                "Host {}:{} did not become reachable within {} seconds",
                ip,
                args.check_tcp_port,
                args.wait_secs
            );
        }
        info!("Host {}:{} is now reachable", ip, args.check_tcp_port);
    }

    Ok(())
}

#[cfg(test)]
mod cli_tests {
    use super::*;

    #[test]
    fn cli_accepts_tui_subcommand_with_default_api_url() {
        let cli = Cli::try_parse_from(["wakezilla", "tui"]).expect("tui subcommand parses");

        match cli.command {
            Commands::Tui(args) => assert_eq!(args.api_url, "http://127.0.0.1:3000"),
            other => panic!("expected Tui command, got {other:?}"),
        }
    }

    #[test]
    fn cli_accepts_tray_subcommand() {
        let cli = Cli::try_parse_from(["wakezilla", "tray"]).expect("tray subcommand parses");

        match cli.command {
            Commands::Tray(_) => {}
            other => panic!("expected Tray command, got {other:?}"),
        }
        assert!(!should_check_for_updates(&cli));
    }

    #[test]
    fn cli_accepts_global_no_update_check_before_subcommand() {
        let cli = Cli::try_parse_from(["wakezilla", "--no-update-check", "proxy-server"])
            .expect("global no-update-check parses");

        assert!(cli.no_update_check);
    }

    #[test]
    fn cli_accepts_update_without_version() {
        let cli = Cli::try_parse_from(["wakezilla", "update"]).expect("update parses");

        match cli.command {
            Commands::Update(args) => assert!(args.version.is_none()),
            other => panic!("expected Update command, got {other:?}"),
        }
    }

    #[test]
    fn cli_accepts_update_with_version() {
        let cli = Cli::try_parse_from(["wakezilla", "update", "--version", "0.2.3"])
            .expect("update version parses");

        match cli.command {
            Commands::Update(args) => assert_eq!(args.version.as_deref(), Some("0.2.3")),
            other => panic!("expected Update command, got {other:?}"),
        }
    }

    #[test]
    fn cli_accepts_hidden_windows_service_entrypoint() {
        let cli =
            Cli::try_parse_from(["wakezilla", "--no-update-check", "windows-service", "proxy"])
                .expect("windows service entrypoint parses");

        assert!(cli.no_update_check);
        assert!(!should_check_for_updates(&cli));
        match cli.command {
            Commands::WindowsService(args) => assert_eq!(args.mode, "proxy"),
            other => panic!("expected WindowsService command, got {other:?}"),
        }
    }

    #[test]
    fn startup_update_check_is_skipped_for_setup_update_and_flag() {
        let setup_cli = Cli::try_parse_from(["wakezilla", "setup"]).expect("setup parses");
        assert!(!should_check_for_updates(&setup_cli));

        let uninstall_cli =
            Cli::try_parse_from(["wakezilla", "uninstall"]).expect("uninstall parses");
        assert!(!should_check_for_updates(&uninstall_cli));

        let update_cli = Cli::try_parse_from(["wakezilla", "update"]).expect("update parses");
        assert!(!should_check_for_updates(&update_cli));

        let no_check_cli = Cli::try_parse_from(["wakezilla", "--no-update-check", "proxy-server"])
            .expect("proxy parses");
        assert!(!should_check_for_updates(&no_check_cli));

        let proxy_cli = Cli::try_parse_from(["wakezilla", "proxy-server"]).expect("proxy parses");
        assert!(should_check_for_updates(&proxy_cli));
    }

    #[test]
    fn cli_accepts_tui_api_url_override() {
        let cli =
            Cli::try_parse_from(["wakezilla", "tui", "--api-url", "http://192.168.1.200:3000"])
                .expect("tui subcommand parses with api override");

        match cli.command {
            Commands::Tui(args) => assert_eq!(args.api_url, "http://192.168.1.200:3000"),
            other => panic!("expected Tui command, got {other:?}"),
        }
    }

    #[test]
    fn cli_accepts_setup_subcommand_with_flags() {
        let cli = Cli::try_parse_from(["wakezilla", "setup", "--mode", "proxy", "--port", "3000"])
            .expect("setup subcommand parses");

        match cli.command {
            Commands::Setup(args) => {
                assert_eq!(args.mode.as_deref(), Some("proxy"));
                assert_eq!(args.port, Some(3000));
            }
            other => panic!("expected Setup command, got {other:?}"),
        }
    }

    #[test]
    fn cli_accepts_setup_subcommand_without_flags() {
        let cli = Cli::try_parse_from(["wakezilla", "setup"]).expect("bare setup parses");
        match cli.command {
            Commands::Setup(args) => {
                assert!(args.mode.is_none());
                assert!(args.port.is_none());
            }
            other => panic!("expected Setup command, got {other:?}"),
        }
    }

    #[test]
    fn cli_accepts_uninstall_subcommand() {
        let cli = Cli::try_parse_from(["wakezilla", "uninstall"]).expect("uninstall parses");

        match cli.command {
            Commands::Uninstall => {}
            other => panic!("expected Uninstall command, got {other:?}"),
        }
    }

    #[test]
    fn cli_accepts_service_subcommand_with_action_and_mode() {
        use crate::setup::ServiceAction;
        let cli = Cli::try_parse_from(["wakezilla", "service", "stop", "--mode", "client"])
            .expect("service subcommand parses");

        match cli.command {
            Commands::Service(args) => {
                assert_eq!(args.action, ServiceAction::Stop);
                assert_eq!(args.mode.as_deref(), Some("client"));
            }
            other => panic!("expected Service command, got {other:?}"),
        }
    }

    #[test]
    fn cli_accepts_service_subcommand_without_mode() {
        use crate::setup::ServiceAction;
        let cli = Cli::try_parse_from(["wakezilla", "service", "restart"])
            .expect("bare service action parses");
        match cli.command {
            Commands::Service(args) => {
                assert_eq!(args.action, ServiceAction::Restart);
                assert!(args.mode.is_none());
            }
            other => panic!("expected Service command, got {other:?}"),
        }
    }

    #[test]
    fn cli_rejects_service_subcommand_without_action() {
        let result = Cli::try_parse_from(["wakezilla", "service"]);
        assert!(result.is_err(), "service requires an action argument");
    }

    #[test]
    fn cli_accepts_service_logs_with_follow_and_lines() {
        use crate::setup::ServiceAction;
        let cli = Cli::try_parse_from([
            "wakezilla",
            "service",
            "logs",
            "--follow",
            "--lines",
            "100",
            "--mode",
            "proxy",
        ])
        .expect("service logs parses");

        match cli.command {
            Commands::Service(args) => {
                assert_eq!(args.action, ServiceAction::Logs);
                assert!(args.follow);
                assert_eq!(args.lines, Some(100));
                assert_eq!(args.mode.as_deref(), Some("proxy"));
            }
            other => panic!("expected Service command, got {other:?}"),
        }
    }

    #[test]
    fn cli_accepts_service_status() {
        use crate::setup::ServiceAction;
        let cli =
            Cli::try_parse_from(["wakezilla", "service", "status"]).expect("service status parses");
        match cli.command {
            Commands::Service(args) => {
                assert_eq!(args.action, ServiceAction::Status);
                assert!(!args.follow);
                assert!(args.lines.is_none());
            }
            other => panic!("expected Service command, got {other:?}"),
        }
    }

    #[test]
    fn send_broadcast_prefers_cli_override() {
        let cli = Cli::try_parse_from([
            "wakezilla",
            "send",
            "AA:BB:CC:DD:EE:FF",
            "--broadcast",
            "192.168.1.255",
        ])
        .expect("send subcommand parses with broadcast override");

        match cli.command {
            Commands::Send(args) => {
                let config = config::Config::default();
                assert_eq!(
                    send_broadcast_addr(&args, &config),
                    Ipv4Addr::new(192, 168, 1, 255)
                );
            }
            other => panic!("expected Send command, got {other:?}"),
        }
    }
}
