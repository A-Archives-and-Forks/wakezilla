//! OS-native system service installation for the `setup` subcommand.

use anyhow::{anyhow, Context, Result};
use std::net::TcpStream;
#[cfg(target_os = "windows")]
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

/// Which Wakezilla server the service runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Proxy,
    Client,
}

impl Mode {
    /// Parse from a CLI string. Returns `None` for unknown values.
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "proxy" | "proxy-server" => Some(Mode::Proxy),
            "client" | "client-server" => Some(Mode::Client),
            _ => None,
        }
    }

    /// The wakezilla subcommand this mode launches.
    pub fn subcommand(self) -> &'static str {
        match self {
            Mode::Proxy => "proxy-server",
            Mode::Client => "client-server",
        }
    }

    /// systemd unit / Windows service name.
    // Platform-conditional: used by the systemd (Linux) / launchd (macOS) / Windows install paths; some are cfg'd out per-OS.
    #[allow(dead_code)]
    pub fn service_name(self) -> &'static str {
        match self {
            Mode::Proxy => "wakezilla-proxy",
            Mode::Client => "wakezilla-client",
        }
    }

    /// Human-readable service display name.
    #[allow(dead_code)]
    pub fn service_display_name(self) -> &'static str {
        match self {
            Mode::Proxy => "Wakezilla Proxy",
            Mode::Client => "Wakezilla Client",
        }
    }

    /// Stable CLI argument used by the hidden Windows service entrypoint.
    #[allow(dead_code)]
    pub fn service_arg(self) -> &'static str {
        match self {
            Mode::Proxy => "proxy",
            Mode::Client => "client",
        }
    }

    /// launchd label (reverse-DNS).
    // Platform-conditional: used by the systemd (Linux) / launchd (macOS) / Windows install paths; some are cfg'd out per-OS.
    #[allow(dead_code)]
    pub fn launchd_label(self) -> &'static str {
        match self {
            Mode::Proxy => "dev.wakezilla.proxy",
            Mode::Client => "dev.wakezilla.client",
        }
    }

    /// Default port for this mode.
    pub fn default_port(self) -> u16 {
        match self {
            Mode::Proxy => 3000,
            Mode::Client => 3001,
        }
    }
}

/// Arguments passed to a Wakezilla process managed by the OS service layer.
pub fn service_program_args(mode: Mode) -> [&'static str; 2] {
    ["--no-update-check", mode.subcommand()]
}

/// Arguments used when Windows Service Manager starts this binary.
#[allow(dead_code)]
pub fn windows_service_program_args(mode: Mode) -> [&'static str; 3] {
    ["--no-update-check", "windows-service", mode.service_arg()]
}

/// Render a systemd unit file. `exe` is the absolute path to the wakezilla binary.
// Platform-conditional: used by the systemd (Linux) / launchd (macOS) / Windows install paths; some are cfg'd out per-OS.
#[allow(dead_code)]
pub fn generate_systemd_unit(mode: Mode, exe: &str) -> String {
    let [no_update_check, sub] = service_program_args(mode);
    format!(
        "[Unit]\n\
         Description=Wakezilla {desc}\n\
         After=network-online.target\n\
         Wants=network-online.target\n\
         \n\
         [Service]\n\
         Type=simple\n\
         ExecStart={exe} {no_update_check} {sub}\n\
         Restart=on-failure\n\
         RestartSec=5\n\
         \n\
         [Install]\n\
         WantedBy=multi-user.target\n",
        desc = mode.subcommand(),
        exe = exe,
        no_update_check = no_update_check,
        sub = sub,
    )
}

/// Directory where the macOS LaunchDaemon writes its stdout/stderr logs.
pub const MACOS_LOG_DIR: &str = "/Library/Logs/wakezilla";

/// Path the daemon's stdout is redirected to (macOS).
fn macos_stdout_log(mode: Mode) -> String {
    format!("{MACOS_LOG_DIR}/{}.out.log", mode.launchd_label())
}

/// Path the daemon's stderr is redirected to (macOS). Tracing logs go here.
fn macos_stderr_log(mode: Mode) -> String {
    format!("{MACOS_LOG_DIR}/{}.err.log", mode.launchd_label())
}

/// Render a launchd LaunchDaemon plist.
// Platform-conditional: used by the systemd (Linux) / launchd (macOS) / Windows install paths; some are cfg'd out per-OS.
#[allow(dead_code)]
pub fn generate_launchd_plist(mode: Mode, exe: &str) -> String {
    let [no_update_check, sub] = service_program_args(mode);
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n\
         \t<key>Label</key>\n\
         \t<string>{label}</string>\n\
         \t<key>ProgramArguments</key>\n\
         \t<array>\n\
         \t\t<string>{exe}</string>\n\
         \t\t<string>{no_update_check}</string>\n\
         \t\t<string>{sub}</string>\n\
         \t</array>\n\
         \t<key>RunAtLoad</key>\n\
         \t<true/>\n\
         \t<key>KeepAlive</key>\n\
         \t<true/>\n\
         \t<key>StandardOutPath</key>\n\
         \t<string>{out}</string>\n\
         \t<key>StandardErrorPath</key>\n\
         \t<string>{err}</string>\n\
         </dict>\n\
         </plist>\n",
        label = mode.launchd_label(),
        exe = exe,
        no_update_check = no_update_check,
        sub = sub,
        out = macos_stdout_log(mode),
        err = macos_stderr_log(mode),
    )
}

/// Validate the service is up by TCP-connecting to `127.0.0.1:port`.
/// Retries up to `attempts` times with a 500ms pause and per-attempt 1s timeout.
pub fn validate(port: u16, attempts: u32) -> Result<()> {
    let addr = format!("127.0.0.1:{port}");
    let socket = addr
        .parse()
        .with_context(|| format!("invalid validation address {addr}"))?;

    for attempt in 1..=attempts {
        match TcpStream::connect_timeout(&socket, Duration::from_secs(1)) {
            Ok(_) => return Ok(()),
            Err(_) if attempt < attempts => {
                std::thread::sleep(Duration::from_millis(500));
            }
            Err(e) => {
                return Err(anyhow!(
                    "service did not accept connections on {addr} after {attempts} attempts: {e}"
                ));
            }
        }
    }
    Err(anyhow!("service not reachable on {addr}"))
}

/// Returns true if the current process can install a system service.
pub fn is_elevated() -> bool {
    #[cfg(unix)]
    {
        Command::new("id")
            .arg("-u")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim() == "0")
            .unwrap_or(false)
    }
    #[cfg(windows)]
    {
        // `net session` requires admin; non-zero exit if unprivileged.
        Command::new("net")
            .arg("session")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

/// Install, enable, and start the system service for `mode`.
///
/// Idempotent: re-running over an existing install overwrites the unit/plist and
/// reloads/recreates the service so a changed mode or port takes effect.
/// `exe` is the absolute path to the wakezilla binary.
pub fn install(mode: Mode, exe: &str) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let unit = generate_systemd_unit(mode, exe);
        let path = format!("/etc/systemd/system/{}.service", mode.service_name());
        std::fs::write(&path, unit).with_context(|| format!("writing {path}"))?;
        run("systemctl", &["daemon-reload"])?;
        run("systemctl", &["enable", mode.service_name()])?;
        // restart (not `enable --now`) so an already-running service picks up the
        // updated unit; restart also starts it if it was stopped.
        run("systemctl", &["restart", mode.service_name()])?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        // Ensure the log directory exists so launchd can redirect stdout/stderr.
        std::fs::create_dir_all(MACOS_LOG_DIR)
            .with_context(|| format!("creating log dir {MACOS_LOG_DIR}"))?;
        let plist = generate_launchd_plist(mode, exe);
        let path = format!("/Library/LaunchDaemons/{}.plist", mode.launchd_label());
        std::fs::write(&path, plist).with_context(|| format!("writing {path}"))?;
        // Unload any previous instance (ignored if not loaded) so the updated
        // plist is reloaded on a re-run instead of erroring "already loaded".
        run_ignore_err("launchctl", &["unload", &path]);
        run("launchctl", &["load", "-w", &path])?;
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        install_windows_service(mode, exe)?;
        start(mode)?;
        Ok(())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = (mode, exe);
        Err(anyhow!("service install not supported on this OS"))
    }
}

/// Run a command, returning an error if it exits non-zero.
#[allow(dead_code)]
fn run(cmd: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(cmd)
        .args(args)
        .status()
        .with_context(|| format!("failed to run {cmd}"))?;
    if !status.success() {
        return Err(anyhow!("{cmd} {args:?} exited with {status}"));
    }
    Ok(())
}

/// Run a command, ignoring its outcome. Used for best-effort teardown of a prior
/// install (e.g. unloading/deleting an existing service) before reinstalling.
#[allow(dead_code)]
fn run_ignore_err(cmd: &str, args: &[&str]) {
    let _ = Command::new(cmd).args(args).status();
}

/// Run a command inheriting the terminal's stdio (for streaming logs). Returns an
/// error only if the command could not be spawned; a non-zero exit (e.g. Ctrl-C
/// out of a `tail -f` / `journalctl -f`) is treated as a normal end of streaming.
#[allow(dead_code)]
fn run_inherit(cmd: &str, args: &[&str]) -> Result<()> {
    Command::new(cmd)
        .args(args)
        .status()
        .with_context(|| format!("failed to run {cmd}"))?;
    Ok(())
}

/// Path to the on-disk service descriptor (systemd unit / launchd plist).
/// Not defined on Windows, which has no descriptor file (services live in the SCM).
#[cfg(target_os = "macos")]
fn descriptor_path(mode: Mode) -> String {
    format!("/Library/LaunchDaemons/{}.plist", mode.launchd_label())
}
#[cfg(target_os = "linux")]
fn descriptor_path(mode: Mode) -> String {
    format!("/etc/systemd/system/{}.service", mode.service_name())
}

/// True if the service for `mode` appears installed on this host.
pub fn is_installed(mode: Mode) -> bool {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        std::path::Path::new(&descriptor_path(mode)).exists()
    }
    #[cfg(target_os = "windows")]
    {
        open_windows_service(mode, windows_service::service::ServiceAccess::QUERY_STATUS).is_ok()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = mode;
        false
    }
}

/// The set of modes currently installed as services on this host.
pub fn installed_modes() -> Vec<Mode> {
    [Mode::Proxy, Mode::Client]
        .into_iter()
        .filter(|m| is_installed(*m))
        .collect()
}

/// Start the installed service for `mode`.
pub fn start(mode: Mode) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        run("systemctl", &["start", mode.service_name()])
    }
    #[cfg(target_os = "macos")]
    {
        run("launchctl", &["load", "-w", &descriptor_path(mode)])
    }
    #[cfg(target_os = "windows")]
    {
        let service = open_windows_service(mode, windows_service::service::ServiceAccess::START)?;
        service.start::<std::ffi::OsString>(&[]).map_err(Into::into)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = mode;
        Err(anyhow!("service control not supported on this OS"))
    }
}

/// Stop the installed service for `mode`.
pub fn stop(mode: Mode) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        run("systemctl", &["stop", mode.service_name()])
    }
    #[cfg(target_os = "macos")]
    {
        run("launchctl", &["unload", &descriptor_path(mode)])
    }
    #[cfg(target_os = "windows")]
    {
        let service = open_windows_service(mode, windows_service::service::ServiceAccess::STOP)?;
        service.stop().map(|_| ()).map_err(Into::into)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = mode;
        Err(anyhow!("service control not supported on this OS"))
    }
}

/// Restart the installed service for `mode`.
pub fn restart(mode: Mode) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        run("systemctl", &["restart", mode.service_name()])
    }
    #[cfg(target_os = "macos")]
    {
        run_ignore_err("launchctl", &["unload", &descriptor_path(mode)]);
        run("launchctl", &["load", "-w", &descriptor_path(mode)])
    }
    #[cfg(target_os = "windows")]
    {
        if is_running(mode) {
            let _ = stop(mode);
            std::thread::sleep(Duration::from_secs(1));
        }
        start(mode)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = mode;
        Err(anyhow!("service control not supported on this OS"))
    }
}

/// Whether the service for `mode` is currently running.
pub fn is_running(mode: Mode) -> bool {
    #[cfg(target_os = "linux")]
    {
        Command::new("systemctl")
            .args(["is-active", "--quiet", mode.service_name()])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(target_os = "macos")]
    {
        let label = format!("system/{}", mode.launchd_label());
        Command::new("launchctl")
            .args(["print", &label])
            .output()
            .map(|o| {
                o.status.success() && String::from_utf8_lossy(&o.stdout).contains("state = running")
            })
            .unwrap_or(false)
    }
    #[cfg(target_os = "windows")]
    {
        open_windows_service(mode, windows_service::service::ServiceAccess::QUERY_STATUS)
            .and_then(|service| service.query_status().map_err(Into::into))
            .map(|status| status.current_state == windows_service::service::ServiceState::Running)
            .unwrap_or(false)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = mode;
        false
    }
}

#[cfg(target_os = "windows")]
fn open_windows_service(
    mode: Mode,
    access: windows_service::service::ServiceAccess,
) -> Result<windows_service::service::Service> {
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
    manager
        .open_service(mode.service_name(), access)
        .with_context(|| format!("failed to open Windows service {}", mode.service_name()))
}

#[cfg(target_os = "windows")]
fn install_windows_service(mode: Mode, exe: &str) -> Result<()> {
    use std::ffi::OsString;
    use windows_service::service::{
        ServiceAccess, ServiceErrorControl, ServiceInfo, ServiceStartType, ServiceType,
    };
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

    let manager_access = ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE;
    let service_manager = ServiceManager::local_computer(None::<&str>, manager_access)?;

    delete_windows_service_if_exists(&service_manager, mode)?;

    let service_info = ServiceInfo {
        name: OsString::from(mode.service_name()),
        display_name: OsString::from(mode.service_display_name()),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: PathBuf::from(exe),
        launch_arguments: windows_service_program_args(mode)
            .into_iter()
            .map(OsString::from)
            .collect(),
        dependencies: vec![],
        account_name: None,
        account_password: None,
    };

    let service = service_manager.create_service(
        &service_info,
        ServiceAccess::CHANGE_CONFIG | ServiceAccess::START,
    )?;
    service
        .set_description(format!(
            "Runs Wakezilla {} as a Windows service.",
            mode.subcommand()
        ))
        .ok();
    Ok(())
}

#[cfg(target_os = "windows")]
fn delete_windows_service_if_exists(
    service_manager: &windows_service::service_manager::ServiceManager,
    mode: Mode,
) -> Result<()> {
    use windows_service::service::{ServiceAccess, ServiceState};

    let access = ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE;
    let service = match service_manager.open_service(mode.service_name(), access) {
        Ok(service) => service,
        Err(_) => return Ok(()),
    };

    if service
        .query_status()
        .map(|status| status.current_state != ServiceState::Stopped)
        .unwrap_or(false)
    {
        let _ = service.stop();
    }
    service.delete()?;
    drop(service);

    for _ in 0..10 {
        if service_manager
            .open_service(mode.service_name(), ServiceAccess::QUERY_STATUS)
            .is_err()
        {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(500));
    }

    Ok(())
}

#[cfg(target_os = "windows")]
windows_service::define_windows_service!(ffi_service_main, windows_service_main);

#[cfg(target_os = "windows")]
static WINDOWS_SERVICE_MODE: std::sync::OnceLock<Mode> = std::sync::OnceLock::new();

#[cfg(target_os = "windows")]
pub fn run_windows_service(mode: Mode) -> Result<()> {
    let _ = WINDOWS_SERVICE_MODE.set(mode);
    windows_service::service_dispatcher::start(mode.service_name(), ffi_service_main)
        .with_context(|| format!("failed to start Windows service {}", mode.service_name()))?;
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn run_windows_service(mode: Mode) -> Result<()> {
    let _ = mode;
    Err(anyhow!(
        "Windows service entrypoint is only supported on Windows"
    ))
}

#[cfg(target_os = "windows")]
fn windows_service_main(_arguments: Vec<std::ffi::OsString>) {
    if let Err(err) = run_windows_service_inner() {
        tracing::error!("Windows service failed: {err:#}");
    }
}

#[cfg(target_os = "windows")]
fn run_windows_service_inner() -> Result<()> {
    use std::sync::{Arc, Mutex};
    use tokio::sync::oneshot;
    use windows_service::service::{ServiceControl, ServiceControlAccept, ServiceState};
    use windows_service::service_control_handler::{
        self, ServiceControlHandlerResult, ServiceStatusHandle,
    };

    let mode = WINDOWS_SERVICE_MODE
        .get()
        .copied()
        .ok_or_else(|| anyhow!("missing Windows service mode"))?;

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let shutdown_tx = Arc::new(Mutex::new(Some(shutdown_tx)));
    let status_handle_slot: Arc<Mutex<Option<ServiceStatusHandle>>> = Arc::new(Mutex::new(None));

    let handler_shutdown_tx = Arc::clone(&shutdown_tx);
    let handler_status = Arc::clone(&status_handle_slot);
    let event_handler = move |control_event| -> ServiceControlHandlerResult {
        match control_event {
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            ServiceControl::Stop => {
                if let Some(status_handle) =
                    *handler_status.lock().unwrap_or_else(|e| e.into_inner())
                {
                    let _ = status_handle.set_service_status(windows_status(
                        ServiceState::StopPending,
                        ServiceControlAccept::empty(),
                    ));
                }
                if let Some(tx) = handler_shutdown_tx
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .take()
                {
                    let _ = tx.send(());
                }
                ServiceControlHandlerResult::NoError
            }
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };

    let status_handle = service_control_handler::register(mode.service_name(), event_handler)?;
    *status_handle_slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(status_handle);

    status_handle.set_service_status(windows_status(
        ServiceState::Running,
        ServiceControlAccept::STOP,
    ))?;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("failed to start Tokio runtime for Windows service")?;

    let config = crate::config::Config::load();
    let result = runtime.block_on(async move {
        let shutdown = async {
            let _ = shutdown_rx.await;
        };
        match mode {
            Mode::Proxy => crate::proxy_server::start_with_shutdown(config, shutdown).await,
            Mode::Client => {
                crate::client_server::start_with_shutdown(config.server.client_port, shutdown).await
            }
        }
    });

    status_handle.set_service_status(windows_status(
        ServiceState::Stopped,
        ServiceControlAccept::empty(),
    ))?;

    result
}

#[cfg(target_os = "windows")]
fn windows_status(
    current_state: windows_service::service::ServiceState,
    controls_accepted: windows_service::service::ServiceControlAccept,
) -> windows_service::service::ServiceStatus {
    windows_service::service::ServiceStatus {
        service_type: windows_service::service::ServiceType::OWN_PROCESS,
        current_state,
        controls_accepted,
        exit_code: windows_service::service::ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    }
}

/// Stream the service's logs to the terminal. `lines` is the tail length;
/// `follow` keeps streaming new output until interrupted.
pub fn logs(mode: Mode, follow: bool, lines: u32) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let n = lines.to_string();
        let mut args = vec!["-u", mode.service_name(), "-n", &n];
        if follow {
            args.push("-f");
        }
        run_inherit("journalctl", &args)
    }
    #[cfg(target_os = "macos")]
    {
        let path = macos_stderr_log(mode);
        if !std::path::Path::new(&path).exists() {
            anyhow::bail!(
                "no log file at {path} yet. The service may not have started or \
                 produced any output."
            );
        }
        let n = lines.to_string();
        let mut args = vec!["-n", &n];
        if follow {
            args.push("-f");
        }
        args.push(&path);
        run_inherit("tail", &args)
    }
    #[cfg(target_os = "windows")]
    {
        let _ = (follow, lines);
        println!(
            "Log streaming is not captured for the Windows service ({}). \
             Check the Windows Event Viewer.",
            mode.subcommand()
        );
        Ok(())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = (mode, follow, lines);
        Err(anyhow!("log viewing not supported on this OS"))
    }
}
