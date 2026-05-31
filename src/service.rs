//! OS-native system service installation for the `setup` subcommand.

use anyhow::{anyhow, Context, Result};
use std::net::TcpStream;
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

/// Render a systemd unit file. `exe` is the absolute path to the wakezilla binary.
// Platform-conditional: used by the systemd (Linux) / launchd (macOS) / Windows install paths; some are cfg'd out per-OS.
#[allow(dead_code)]
pub fn generate_systemd_unit(mode: Mode, exe: &str) -> String {
    format!(
        "[Unit]\n\
         Description=Wakezilla {desc}\n\
         After=network-online.target\n\
         Wants=network-online.target\n\
         \n\
         [Service]\n\
         Type=simple\n\
         ExecStart={exe} {sub}\n\
         Restart=on-failure\n\
         RestartSec=5\n\
         \n\
         [Install]\n\
         WantedBy=multi-user.target\n",
        desc = mode.subcommand(),
        exe = exe,
        sub = mode.subcommand(),
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
        sub = mode.subcommand(),
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
        // Best-effort teardown of any previous instance so `sc create` does not
        // error "service already exists" on a re-run.
        run_ignore_err("sc", &["stop", mode.service_name()]);
        run_ignore_err("sc", &["delete", mode.service_name()]);
        let bin_path = format!("\"{exe}\" {}", mode.subcommand());
        run(
            "sc",
            &[
                "create",
                mode.service_name(),
                "binPath=",
                &bin_path,
                "start=",
                "auto",
            ],
        )?;
        run("sc", &["start", mode.service_name()])?;
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
        Command::new("sc")
            .args(["query", mode.service_name()])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
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
        run("sc", &["start", mode.service_name()])
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
        run("sc", &["stop", mode.service_name()])
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
        run_ignore_err("sc", &["stop", mode.service_name()]);
        run("sc", &["start", mode.service_name()])
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
        Command::new("sc")
            .args(["query", mode.service_name()])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains("RUNNING"))
            .unwrap_or(false)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = mode;
        false
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
                "no log file at {path}. Re-run `sudo wakezilla setup` to enable log capture \
                 (existing installs predate stdout/stderr redirection)."
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
