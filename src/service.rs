//! OS-native system service installation for the `setup` subcommand.

use anyhow::{anyhow, Context, Result};
#[cfg(target_os = "windows")]
use std::collections::VecDeque;
#[cfg(target_os = "windows")]
use std::io::{Read, Seek, SeekFrom, Write};
#[cfg(unix)]
use std::io::{Read, Seek, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
#[cfg(target_os = "windows")]
use std::time::Instant;

#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};

#[cfg(any(unix, target_os = "windows"))]
use sha2::{Digest, Sha256};

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

/// Fixed root-owned executable path used by systemd for `mode`.
pub fn linux_service_binary_path(mode: Mode) -> PathBuf {
    Path::new("/usr/local/libexec/wakezilla").join(mode.service_name())
}

/// Fixed root-owned executable path used by launchd for `mode`.
pub fn macos_service_binary_path(mode: Mode) -> PathBuf {
    Path::new("/Library/PrivilegedHelperTools").join(mode.launchd_label())
}

/// Fixed LocalSystem executable path below the Program Files known folder.
#[allow(dead_code)]
pub fn windows_service_binary_path_in(program_files: &Path, mode: Mode) -> PathBuf {
    program_files
        .join("Wakezilla")
        .join("Service")
        .join(format!("{}.exe", mode.service_name()))
}

/// Return whether a Windows SCM executable path is the protected per-mode path.
#[allow(dead_code)]
pub fn windows_image_path_uses_protected_binary(
    program_files: &Path,
    mode: Mode,
    image_path: &Path,
) -> bool {
    fn normalized(path: &Path) -> String {
        path.to_string_lossy()
            .trim_start_matches(r"\\?\")
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_ascii_lowercase()
    }

    let protected = windows_service_binary_path_in(program_files, mode);
    if normalized(image_path) == normalized(&protected) {
        return true;
    }

    let image_path = image_path.to_string_lossy();
    let Some(command) = image_path.strip_prefix('"') else {
        return false;
    };
    let Some((executable, arguments)) = command.split_once('"') else {
        return false;
    };
    let expected_arguments = format!(" {}", windows_service_program_args(mode).join(" "));
    normalized(Path::new(executable)) == normalized(&protected) && arguments == expected_arguments
}

/// Protected DACL for the Windows service directory: full control for only
/// LocalSystem and built-in Administrators, inherited by children.
#[allow(dead_code)]
pub fn windows_service_directory_sddl() -> &'static str {
    "D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)"
}

/// Protected DACL for each Windows service executable.
#[allow(dead_code)]
pub fn windows_service_file_sddl() -> &'static str {
    "D:P(A;;FA;;;SY)(A;;FA;;;BA)"
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

/// Windows Firewall rule name used for the inbound service port.
#[allow(dead_code)]
pub fn firewall_rule_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Proxy => "Wakezilla Proxy Server",
        Mode::Client => "Wakezilla Client Server",
    }
}

/// Log file name used by OS service processes when stdout/stderr are not
/// captured by the platform service manager.
#[allow(dead_code)]
pub fn service_log_file_name(mode: Mode) -> String {
    format!("{}.log", mode.service_name())
}

/// OS-standard path for Wakezilla service log files.
#[allow(dead_code)]
pub fn service_log_path(mode: Mode) -> PathBuf {
    crate::config::data_path(&service_log_file_name(mode))
}

/// Create or update the OS firewall rule needed for remote access to the service.
///
/// On Windows this installs an inbound TCP allow rule for the configured port
/// and executable. Other platforms currently do not need setup-managed firewall
/// configuration, so this is a no-op.
pub fn configure_firewall(mode: Mode, port: u16) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        let exe = validate_windows_protected_binary(mode)?;
        let exe = exe.to_string_lossy();
        let rule_name = firewall_rule_name(mode);
        let name_arg = format!("name={rule_name}");
        let program_arg = format!("program={exe}");
        let port_arg = format!("localport={port}");

        run_ignore_err(
            "netsh",
            &["advfirewall", "firewall", "delete", "rule", &name_arg],
        );
        run(
            "netsh",
            &[
                "advfirewall",
                "firewall",
                "add",
                "rule",
                &name_arg,
                "dir=in",
                "action=allow",
                &program_arg,
                "enable=yes",
                "profile=any",
                "protocol=TCP",
                &port_arg,
            ],
        )
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (mode, port);
        Ok(())
    }
}

/// Remove the OS firewall rule managed by Wakezilla setup.
pub fn remove_firewall(mode: Mode) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        let rule_name = firewall_rule_name(mode);
        let name_arg = format!("name={rule_name}");

        let output = run_output(
            "netsh",
            &["advfirewall", "firewall", "delete", "rule", &name_arg],
        )?;
        if output.status.success() || netsh_rule_not_found(&output) {
            return Ok(());
        }
        anyhow::bail!(
            "netsh failed to remove firewall rule '{}': {}",
            rule_name,
            command_output_text(&output)
        );
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = mode;
        Ok(())
    }
}

/// Render a systemd unit file using the fixed root-owned executable path.
// Platform-conditional: used by the systemd (Linux) / launchd (macOS) / Windows install paths; some are cfg'd out per-OS.
#[allow(dead_code)]
pub fn generate_systemd_unit(mode: Mode) -> String {
    let [no_update_check, sub] = service_program_args(mode);
    let exe = linux_service_binary_path(mode);
    let exe = exe.to_string_lossy();
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

/// Return whether `unit` is exactly Wakezilla's canonical protected systemd unit.
#[allow(dead_code)]
pub fn systemd_unit_uses_protected_binary(mode: Mode, unit: &str) -> bool {
    unit == generate_systemd_unit(mode)
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
pub fn generate_launchd_plist(mode: Mode) -> String {
    let [no_update_check, sub] = service_program_args(mode);
    let exe = macos_service_binary_path(mode);
    let exe = exe.to_string_lossy();
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

/// Return whether `plist` is exactly Wakezilla's canonical protected LaunchDaemon.
#[allow(dead_code)]
pub fn launchd_plist_uses_protected_binary(mode: Mode, plist: &str) -> bool {
    plist == generate_launchd_plist(mode)
}

#[cfg(unix)]
fn ensure_unix_service_directory_at(path: &Path, owner_uid: u32, owner_gid: u32) -> Result<bool> {
    let created = match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.file_type().is_dir()
                || metadata.uid() != owner_uid
                || metadata.gid() != owner_gid
                || metadata.permissions().mode() & 0o777 != 0o755
            {
                anyhow::bail!(
                    "protected service directory {} must be a real directory with protected ownership and mode 0755",
                    path.display()
                );
            }
            false
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::DirBuilder::new()
                .mode(0o755)
                .create(path)
                .with_context(|| {
                    format!("creating protected service directory {}", path.display())
                })?;
            true
        }
        Err(error) => {
            return Err(error)
                .with_context(|| format!("inspecting service directory {}", path.display()));
        }
    };

    let directory = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .with_context(|| format!("opening protected service directory {}", path.display()))?;
    if created {
        let chown_status = unsafe {
            libc::fchown(
                std::os::fd::AsRawFd::as_raw_fd(&directory),
                owner_uid,
                owner_gid,
            )
        };
        if chown_status != 0 {
            return Err(std::io::Error::last_os_error()).with_context(|| {
                format!(
                    "setting protected service directory ownership on {}",
                    path.display()
                )
            });
        }
        directory
            .set_permissions(std::fs::Permissions::from_mode(0o755))
            .with_context(|| {
                format!(
                    "setting protected service directory mode on {}",
                    path.display()
                )
            })?;
    }
    directory
        .sync_all()
        .with_context(|| format!("syncing protected service directory {}", path.display()))?;
    let metadata = directory.metadata()?;
    if !metadata.file_type().is_dir()
        || metadata.uid() != owner_uid
        || metadata.gid() != owner_gid
        || metadata.permissions().mode() & 0o777 != 0o755
    {
        anyhow::bail!(
            "protected service directory {} did not retain secure ownership and mode",
            path.display()
        );
    }
    Ok(created)
}

#[cfg(unix)]
fn remove_unix_protected_binary_at(
    binary: &Path,
    remove_parent_if_empty: bool,
    owner_uid: u32,
    owner_gid: u32,
) -> Result<()> {
    match std::fs::symlink_metadata(binary) {
        Ok(metadata) => {
            if !metadata.file_type().is_file()
                || metadata.uid() != owner_uid
                || metadata.gid() != owner_gid
                || metadata.permissions().mode() & 0o777 != 0o755
            {
                anyhow::bail!(
                    "refusing to remove unsafe or foreign protected service binary {}",
                    binary.display()
                );
            }
            std::fs::remove_file(binary).with_context(|| {
                format!("removing protected service binary {}", binary.display())
            })?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error)
                .with_context(|| format!("inspecting service binary {}", binary.display()));
        }
    }

    if remove_parent_if_empty {
        if let Some(parent) = binary.parent() {
            match std::fs::remove_dir(parent) {
                Ok(()) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
                    ) => {}
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!("removing empty service directory {}", parent.display())
                    });
                }
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
#[derive(Debug)]
struct UnixProtectedBinaryUpdate {
    destination: PathBuf,
    backup: Option<PathBuf>,
    created_parent: bool,
    finished: bool,
}

#[cfg(unix)]
impl UnixProtectedBinaryUpdate {
    fn commit(mut self) -> Result<()> {
        // Persist the published rename before deleting the only rollback copy.
        sync_parent_directory(&self.destination)?;
        if let Some(backup) = self.backup.as_ref() {
            std::fs::remove_file(backup).with_context(|| {
                format!("removing protected binary backup {}", backup.display())
            })?;
            self.backup = None;
            // The new binary is now the only valid copy. Never let Drop remove it,
            // even if the final directory sync reports an I/O error.
            self.finished = true;
            sync_parent_directory(&self.destination)?;
            return Ok(());
        }
        self.finished = true;
        Ok(())
    }

    fn rollback(mut self) -> Result<()> {
        let result = self.rollback_inner();
        if result.is_ok() {
            self.finished = true;
        }
        result
    }

    fn rollback_inner(&mut self) -> Result<()> {
        match std::fs::remove_file(&self.destination) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "removing failed protected binary {} during rollback",
                        self.destination.display()
                    )
                });
            }
        }

        if let Some(backup) = self.backup.as_ref() {
            std::fs::rename(backup, &self.destination).with_context(|| {
                format!(
                    "restoring protected binary backup {} to {}",
                    backup.display(),
                    self.destination.display()
                )
            })?;
        }
        sync_parent_directory(&self.destination)?;
        if self.created_parent {
            if let Some(parent) = self.destination.parent() {
                match std::fs::remove_dir(parent) {
                    Ok(()) => {}
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
                        ) => {}
                    Err(error) => {
                        return Err(error).with_context(|| {
                            format!(
                                "removing rolled-back service directory {}",
                                parent.display()
                            )
                        });
                    }
                }
                sync_parent_directory(parent)?;
            }
        }
        Ok(())
    }
}

#[cfg(unix)]
impl Drop for UnixProtectedBinaryUpdate {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.rollback_inner();
        }
    }
}

#[cfg(unix)]
fn begin_unix_protected_binary_update_at(
    source: &Path,
    destination: &Path,
    owner_uid: u32,
    owner_gid: u32,
) -> Result<UnixProtectedBinaryUpdate> {
    let source_metadata = std::fs::symlink_metadata(source).with_context(|| {
        format!(
            "inspecting service binary source {} (expected a regular non-symlink file)",
            source.display()
        )
    })?;
    if !source_metadata.file_type().is_file() {
        anyhow::bail!(
            "service binary source {} must be a regular non-symlink file",
            source.display()
        );
    }

    let parent = destination.parent().ok_or_else(|| {
        anyhow!(
            "protected service binary path {} has no parent",
            destination.display()
        )
    })?;
    let parent_metadata = std::fs::symlink_metadata(parent)
        .with_context(|| format!("inspecting protected directory {}", parent.display()))?;
    if !parent_metadata.file_type().is_dir() {
        anyhow::bail!(
            "protected service directory {} must be a real directory",
            parent.display()
        );
    }

    match std::fs::symlink_metadata(destination) {
        Ok(existing)
            if existing.file_type().is_file()
                && existing.uid() == owner_uid
                && existing.gid() == owner_gid
                && existing.permissions().mode() & 0o777 == 0o755 => {}
        Ok(_) => anyhow::bail!(
            "existing protected service binary {} has unsafe type, ownership, or mode; remove it manually and rerun setup",
            destination.display()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| {
                format!("inspecting protected service binary {}", destination.display())
            });
        }
    }

    let mut source_file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(source)
        .with_context(|| {
            format!(
                "opening service binary source {} as a regular non-symlink file",
                source.display()
            )
        })?;
    if !source_file.metadata()?.file_type().is_file() {
        anyhow::bail!(
            "service binary source {} must be a regular non-symlink file",
            source.display()
        );
    }

    let mut staged = tempfile::Builder::new()
        .prefix(".wakezilla-service-stage-")
        .tempfile_in(parent)
        .with_context(|| format!("creating private service stage in {}", parent.display()))?;
    std::io::copy(&mut source_file, staged.as_file_mut()).with_context(|| {
        format!(
            "copying {} into the protected service stage",
            source.display()
        )
    })?;
    staged
        .as_file_mut()
        .sync_all()
        .context("syncing protected service stage")?;

    source_file.rewind()?;
    staged.as_file_mut().rewind()?;
    let source_hash = sha256_reader(&mut source_file)?;
    let staged_hash = sha256_reader(staged.as_file_mut())?;
    if source_hash != staged_hash {
        anyhow::bail!(
            "protected service stage verification failed for {}",
            source.display()
        );
    }

    staged
        .as_file()
        .set_permissions(std::fs::Permissions::from_mode(0o755))
        .context("setting protected service binary mode")?;
    let chown_status = unsafe {
        libc::fchown(
            std::os::fd::AsRawFd::as_raw_fd(staged.as_file()),
            owner_uid,
            owner_gid,
        )
    };
    if chown_status != 0 {
        return Err(std::io::Error::last_os_error())
            .context("setting protected service binary ownership");
    }
    staged
        .as_file()
        .sync_all()
        .context("syncing protected service binary metadata")?;

    let backup = if destination.exists() {
        let temporary = tempfile::Builder::new()
            .prefix(".wakezilla-service-backup-")
            .tempfile_in(parent)
            .with_context(|| format!("allocating protected backup in {}", parent.display()))?;
        let backup_path = temporary.path().to_path_buf();
        drop(temporary);
        std::fs::rename(destination, &backup_path).with_context(|| {
            format!(
                "moving prior protected binary {} to backup {}",
                destination.display(),
                backup_path.display()
            )
        })?;
        Some(backup_path)
    } else {
        None
    };

    let persist_result = staged.persist_noclobber(destination);
    if let Err(error) = persist_result {
        if let Some(backup_path) = backup.as_ref() {
            let _ = std::fs::rename(backup_path, destination);
        }
        return Err(error.error).with_context(|| {
            format!(
                "publishing protected service binary {}",
                destination.display()
            )
        });
    }
    let update = UnixProtectedBinaryUpdate {
        destination: destination.to_path_buf(),
        backup,
        created_parent: false,
        finished: false,
    };
    if let Err(error) = sync_parent_directory(destination) {
        return match update.rollback() {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(anyhow!(
                "{error:#}; additionally failed to roll back the unsynced protected binary: {rollback_error:#}"
            )),
        };
    }
    Ok(update)
}

#[cfg(any(unix, target_os = "windows"))]
fn sha256_reader(reader: &mut std::fs::File) -> Result<[u8; 32]> {
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().into())
}

#[cfg(unix)]
fn sync_parent_directory(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("{} has no parent", path.display()))?;
    std::fs::File::open(parent)
        .with_context(|| format!("opening directory {} for sync", parent.display()))?
        .sync_all()
        .with_context(|| format!("syncing directory {}", parent.display()))
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn protected_service_binary_path(mode: Mode) -> Result<PathBuf> {
    #[cfg(target_os = "linux")]
    let path = linux_service_binary_path(mode);
    #[cfg(target_os = "macos")]
    let path = macos_service_binary_path(mode);
    Ok(path)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn prepare_unix_service_directory(path: &Path) -> Result<bool> {
    if !path.is_absolute() {
        anyhow::bail!(
            "protected service directory {} must be absolute",
            path.display()
        );
    }

    let mut current = PathBuf::new();
    let mut final_created = false;
    for component in path.components() {
        current.push(component.as_os_str());
        if current == Path::new("/") {
            continue;
        }
        match std::fs::symlink_metadata(&current) {
            Ok(_) => {
                ensure_unix_service_directory_at(&current, 0, 0)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                ensure_unix_service_directory_at(&current, 0, 0)?;
                if current == path {
                    final_created = true;
                }
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("inspecting protected path component {}", current.display())
                });
            }
        }
    }
    if !final_created {
        ensure_unix_service_directory_at(path, 0, 0)?;
    }
    Ok(final_created)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn begin_protected_binary_update(mode: Mode, source: &Path) -> Result<UnixProtectedBinaryUpdate> {
    if !source.is_absolute() {
        anyhow::bail!(
            "service binary source {} must be an absolute regular non-symlink file",
            source.display()
        );
    }
    let destination = protected_service_binary_path(mode)?;
    let parent = destination
        .parent()
        .ok_or_else(|| anyhow!("{} has no parent", destination.display()))?;
    let created_parent = prepare_unix_service_directory(parent)?;
    let mut update = match begin_unix_protected_binary_update_at(source, &destination, 0, 0) {
        Ok(update) => update,
        Err(error) => {
            if created_parent {
                let _ = std::fs::remove_dir(parent);
            }
            return Err(error);
        }
    };
    update.created_parent = created_parent;
    Ok(update)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn write_unix_service_descriptor(path: &Path, contents: &str) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("service descriptor {} has no parent", path.display()))?;
    let parent_metadata = std::fs::symlink_metadata(parent).with_context(|| {
        format!(
            "inspecting service descriptor directory {}",
            parent.display()
        )
    })?;
    if !parent_metadata.file_type().is_dir() {
        anyhow::bail!(
            "service descriptor directory {} must be a real directory",
            parent.display()
        );
    }
    if let Ok(metadata) = std::fs::symlink_metadata(path) {
        if !metadata.file_type().is_file() {
            anyhow::bail!(
                "service descriptor {} must be a regular non-symlink file",
                path.display()
            );
        }
    }

    let mut staged = tempfile::Builder::new()
        .prefix(".wakezilla-descriptor-")
        .tempfile_in(parent)
        .with_context(|| format!("staging service descriptor in {}", parent.display()))?;
    staged.as_file_mut().write_all(contents.as_bytes())?;
    staged
        .as_file()
        .set_permissions(std::fs::Permissions::from_mode(0o644))?;
    let chown_status =
        unsafe { libc::fchown(std::os::fd::AsRawFd::as_raw_fd(staged.as_file()), 0, 0) };
    if chown_status != 0 {
        return Err(std::io::Error::last_os_error())
            .context("setting service descriptor ownership");
    }
    staged.as_file().sync_all()?;
    staged
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("publishing service descriptor {}", path.display()))?;
    sync_parent_directory(path)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn ensure_unix_descriptor_uses_protected_binary(mode: Mode) -> Result<()> {
    let binary = protected_service_binary_path(mode)?;
    let path = PathBuf::from(descriptor_path(mode));
    #[cfg(target_os = "linux")]
    let canonical = generate_systemd_unit(mode);
    #[cfg(target_os = "macos")]
    let canonical = generate_launchd_plist(mode);
    validate_unix_service_files_at(&binary, &path, &canonical, 0, 0).with_context(|| {
        format!(
            "run `wakezilla setup --mode {} --port <port> --yes` to migrate {} to the protected binary",
            mode.service_arg(),
            mode.subcommand()
        )
    })
}

#[cfg(unix)]
fn validate_unix_service_files_at(
    binary: &Path,
    descriptor: &Path,
    canonical_descriptor: &str,
    owner_uid: u32,
    owner_gid: u32,
) -> Result<()> {
    let binary_metadata = std::fs::symlink_metadata(binary)
        .with_context(|| format!("inspecting protected service binary {}", binary.display()))?;
    if !binary_metadata.file_type().is_file()
        || binary_metadata.uid() != owner_uid
        || binary_metadata.gid() != owner_gid
        || binary_metadata.permissions().mode() & 0o777 != 0o755
    {
        anyhow::bail!(
            "unsafe legacy protected service binary {}; expected a regular non-symlink file with protected ownership and mode 0755",
            binary.display()
        );
    }

    let descriptor_metadata = std::fs::symlink_metadata(descriptor)
        .with_context(|| format!("inspecting service descriptor {}", descriptor.display()))?;
    if !descriptor_metadata.file_type().is_file()
        || descriptor_metadata.uid() != owner_uid
        || descriptor_metadata.gid() != owner_gid
        || descriptor_metadata.permissions().mode() & 0o022 != 0
    {
        anyhow::bail!(
            "unsafe legacy service descriptor {}; expected a protected regular non-symlink file",
            descriptor.display()
        );
    }
    let contents = std::fs::read_to_string(descriptor)
        .with_context(|| format!("reading service descriptor {}", descriptor.display()))?;
    if contents != canonical_descriptor {
        anyhow::bail!(
            "unsafe legacy service descriptor {} references a non-protected executable",
            descriptor.display()
        );
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn windows_program_files_known_folder() -> Result<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{FOLDERID_ProgramFiles, SHGetKnownFolderPath};

    let mut raw_path = std::ptr::null_mut();
    let result = unsafe {
        SHGetKnownFolderPath(
            &FOLDERID_ProgramFiles,
            0,
            std::ptr::null_mut(),
            &mut raw_path,
        )
    };
    if result < 0 || raw_path.is_null() {
        anyhow::bail!(
            "failed to resolve Program Files with SHGetKnownFolderPath (HRESULT 0x{:08x})",
            result as u32
        );
    }

    let mut length = 0usize;
    while unsafe { *raw_path.add(length) } != 0 {
        length += 1;
    }
    let path = PathBuf::from(std::ffi::OsString::from_wide(unsafe {
        std::slice::from_raw_parts(raw_path, length)
    }));
    unsafe { CoTaskMemFree(raw_path.cast()) };
    if !path.is_absolute() {
        anyhow::bail!(
            "Program Files known folder {} is not absolute",
            path.display()
        );
    }
    Ok(path)
}

#[cfg(target_os = "windows")]
fn protected_service_binary_path(mode: Mode) -> Result<PathBuf> {
    Ok(windows_service_binary_path_in(
        &windows_program_files_known_folder()?,
        mode,
    ))
}

#[cfg(target_os = "windows")]
fn windows_wide(value: &std::ffi::OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    value.encode_wide().chain(std::iter::once(0)).collect()
}

#[cfg(target_os = "windows")]
fn windows_path_has_no_reparse_components(path: &Path) -> Result<()> {
    use std::path::Component;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileAttributesW, FILE_ATTRIBUTE_REPARSE_POINT, INVALID_FILE_ATTRIBUTES,
    };

    if !path.is_absolute() {
        anyhow::bail!("protected Windows path {} must be absolute", path.display());
    }
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        if matches!(component, Component::Prefix(_)) {
            continue;
        }
        let wide = windows_wide(current.as_os_str());
        let attributes = unsafe { GetFileAttributesW(wide.as_ptr()) };
        if attributes == INVALID_FILE_ATTRIBUTES {
            return Err(std::io::Error::last_os_error()).with_context(|| {
                format!("inspecting Windows path component {}", current.display())
            });
        }
        if attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            anyhow::bail!(
                "protected Windows path component {} must not be a reparse point",
                current.display()
            );
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn windows_full_security_sddl(directory: bool) -> String {
    let dacl = if directory {
        windows_service_directory_sddl()
    } else {
        windows_service_file_sddl()
    };
    format!("O:BAG:BA{dacl}")
}

#[cfg(target_os = "windows")]
fn windows_security_sddl(path: &Path) -> Result<String> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::Foundation::{LocalFree, ERROR_SUCCESS};
    use windows_sys::Win32::Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW, GetNamedSecurityInfoW,
        SDDL_REVISION_1, SE_FILE_OBJECT,
    };
    use windows_sys::Win32::Security::{
        DACL_SECURITY_INFORMATION, GROUP_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION,
    };

    let wide_path = windows_wide(path.as_os_str());
    let information =
        OWNER_SECURITY_INFORMATION | GROUP_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;
    let mut descriptor = std::ptr::null_mut();
    let status = unsafe {
        GetNamedSecurityInfoW(
            wide_path.as_ptr(),
            SE_FILE_OBJECT,
            information,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    if status != ERROR_SUCCESS || descriptor.is_null() {
        anyhow::bail!(
            "GetNamedSecurityInfoW failed for {} with error {}",
            path.display(),
            status
        );
    }

    let mut raw_sddl = std::ptr::null_mut();
    let convert_ok = unsafe {
        ConvertSecurityDescriptorToStringSecurityDescriptorW(
            descriptor,
            SDDL_REVISION_1,
            information,
            &mut raw_sddl,
            std::ptr::null_mut(),
        )
    };
    if convert_ok == 0 || raw_sddl.is_null() {
        unsafe { LocalFree(descriptor.cast()) };
        return Err(std::io::Error::last_os_error())
            .with_context(|| format!("reading protected ACL from {}", path.display()));
    }

    let mut length = 0usize;
    while unsafe { *raw_sddl.add(length) } != 0 {
        length += 1;
    }
    let sddl =
        std::ffi::OsString::from_wide(unsafe { std::slice::from_raw_parts(raw_sddl, length) })
            .to_string_lossy()
            .into_owned();
    unsafe {
        LocalFree(raw_sddl.cast());
        LocalFree(descriptor.cast());
    }
    Ok(sddl)
}

#[cfg(target_os = "windows")]
fn windows_security_matches_expected(path: &Path, directory: bool) -> Result<bool> {
    let expected = windows_full_security_sddl(directory);
    let actual = windows_security_sddl(path)?;
    let auto_inherited = expected.replacen("D:P", "D:PAI", 1);
    Ok(actual == expected || actual == auto_inherited)
}

#[cfg(target_os = "windows")]
fn apply_windows_protected_security(path: &Path, directory: bool) -> Result<()> {
    use windows_sys::Win32::Foundation::{LocalFree, ERROR_SUCCESS};
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SetNamedSecurityInfoW,
        SDDL_REVISION_1, SE_FILE_OBJECT,
    };
    use windows_sys::Win32::Security::{
        GetSecurityDescriptorDacl, GetSecurityDescriptorGroup, GetSecurityDescriptorOwner,
        DACL_SECURITY_INFORMATION, GROUP_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION,
        PROTECTED_DACL_SECURITY_INFORMATION,
    };

    let expected = windows_full_security_sddl(directory);
    let wide_sddl = windows_wide(std::ffi::OsStr::new(&expected));
    let mut descriptor = std::ptr::null_mut();
    let converted = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            wide_sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    };
    if converted == 0 || descriptor.is_null() {
        return Err(std::io::Error::last_os_error())
            .with_context(|| format!("building protected ACL for {}", path.display()));
    }

    let mut owner = std::ptr::null_mut();
    let mut group = std::ptr::null_mut();
    let mut dacl = std::ptr::null_mut();
    let mut defaulted = 0;
    let mut present = 0;
    let fields_ok = unsafe {
        GetSecurityDescriptorOwner(descriptor, &mut owner, &mut defaulted) != 0
            && GetSecurityDescriptorGroup(descriptor, &mut group, &mut defaulted) != 0
            && GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted) != 0
            && present != 0
            && !dacl.is_null()
    };
    if !fields_ok {
        unsafe { LocalFree(descriptor.cast()) };
        return Err(std::io::Error::last_os_error())
            .with_context(|| format!("reading protected ACL fields for {}", path.display()));
    }

    let mut wide_path = windows_wide(path.as_os_str());
    let status = unsafe {
        SetNamedSecurityInfoW(
            wide_path.as_mut_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION
                | GROUP_SECURITY_INFORMATION
                | DACL_SECURITY_INFORMATION
                | PROTECTED_DACL_SECURITY_INFORMATION,
            owner,
            group,
            dacl,
            std::ptr::null(),
        )
    };
    unsafe { LocalFree(descriptor.cast()) };
    if status != ERROR_SUCCESS {
        anyhow::bail!(
            "SetNamedSecurityInfoW failed for {} with error {}",
            path.display(),
            status
        );
    }
    if !windows_security_matches_expected(path, directory)? {
        anyhow::bail!(
            "protected Windows ACL verification failed for {}",
            path.display()
        );
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn ensure_windows_service_directories() -> Result<(PathBuf, Vec<PathBuf>)> {
    let program_files = windows_program_files_known_folder()?;
    windows_path_has_no_reparse_components(&program_files)?;
    let wakezilla = program_files.join("Wakezilla");
    let service = wakezilla.join("Service");
    let mut created = Vec::new();
    for directory in [&wakezilla, &service] {
        match std::fs::symlink_metadata(directory) {
            Ok(metadata) if metadata.file_type().is_dir() => {}
            Ok(_) => anyhow::bail!(
                "protected Windows service path {} must be a real directory",
                directory.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir(directory).with_context(|| {
                    format!(
                        "creating protected Windows service directory {}",
                        directory.display()
                    )
                })?;
                created.push(directory.to_path_buf());
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "inspecting Windows service directory {}",
                        directory.display()
                    )
                });
            }
        }
        windows_path_has_no_reparse_components(directory)?;
        apply_windows_protected_security(directory, true)?;
    }
    Ok((service, created))
}

#[cfg(target_os = "windows")]
#[derive(Debug)]
struct WindowsProtectedBinaryUpdate {
    destination: PathBuf,
    backup: Option<PathBuf>,
    created_directories: Vec<PathBuf>,
    finished: bool,
}

#[cfg(target_os = "windows")]
fn windows_rename_with_retry(source: &Path, destination: &Path) -> Result<()> {
    use windows_sys::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_SHARING_VIOLATION};

    for attempt in 0..50 {
        match std::fs::rename(source, destination) {
            Ok(()) => return Ok(()),
            Err(error)
                if attempt < 49
                    && matches!(
                        error.raw_os_error(),
                        Some(code)
                            if code == ERROR_ACCESS_DENIED as i32
                                || code == ERROR_SHARING_VIOLATION as i32
                    ) =>
            {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("renaming {} to {}", source.display(), destination.display())
                });
            }
        }
    }
    unreachable!("bounded Windows rename loop always returns")
}

#[cfg(target_os = "windows")]
fn windows_remove_file_with_retry(path: &Path) -> Result<()> {
    use windows_sys::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_SHARING_VIOLATION};

    for attempt in 0..50 {
        match std::fs::remove_file(path) {
            Ok(()) => return Ok(()),
            Err(error)
                if attempt < 49
                    && matches!(
                        error.raw_os_error(),
                        Some(code)
                            if code == ERROR_ACCESS_DENIED as i32
                                || code == ERROR_SHARING_VIOLATION as i32
                    ) =>
            {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(error) => {
                return Err(error).with_context(|| format!("removing {}", path.display()));
            }
        }
    }
    unreachable!("bounded Windows removal loop always returns")
}

#[cfg(target_os = "windows")]
impl WindowsProtectedBinaryUpdate {
    fn commit(mut self) -> Result<()> {
        if let Some(backup) = self.backup.as_ref() {
            std::fs::remove_file(backup).with_context(|| {
                format!("removing protected binary backup {}", backup.display())
            })?;
            self.backup = None;
        }
        self.finished = true;
        Ok(())
    }

    fn rollback(mut self) -> Result<()> {
        let result = self.rollback_inner();
        if result.is_ok() {
            self.finished = true;
        }
        result
    }

    fn rollback_inner(&mut self) -> Result<()> {
        match windows_remove_file_with_retry(&self.destination) {
            Ok(()) => {}
            Err(error)
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|io_error| io_error.kind() == std::io::ErrorKind::NotFound) => {}
            Err(error) => {
                return Err(error).context(format!(
                    "removing {} during rollback",
                    self.destination.display()
                ));
            }
        }
        if let Some(backup) = self.backup.as_ref() {
            windows_rename_with_retry(backup, &self.destination).with_context(|| {
                format!(
                    "restoring protected binary backup {} to {}",
                    backup.display(),
                    self.destination.display()
                )
            })?;
        }
        for directory in self.created_directories.iter().rev() {
            match std::fs::remove_dir(directory) {
                Ok(()) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
                    ) => {}
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!("removing rolled-back directory {}", directory.display())
                    });
                }
            }
        }
        Ok(())
    }
}

#[cfg(target_os = "windows")]
impl Drop for WindowsProtectedBinaryUpdate {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.rollback_inner();
        }
    }
}

#[cfg(target_os = "windows")]
fn begin_protected_binary_update(
    mode: Mode,
    source: &Path,
) -> Result<WindowsProtectedBinaryUpdate> {
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileAttributesW, FILE_ATTRIBUTE_REPARSE_POINT, INVALID_FILE_ATTRIBUTES,
    };

    if !source.is_absolute() {
        anyhow::bail!(
            "service binary source {} must be an absolute regular non-symlink file",
            source.display()
        );
    }
    let source_metadata = std::fs::symlink_metadata(source)
        .with_context(|| format!("inspecting service binary source {}", source.display()))?;
    let source_wide = windows_wide(source.as_os_str());
    let source_attributes = unsafe { GetFileAttributesW(source_wide.as_ptr()) };
    if !source_metadata.file_type().is_file()
        || source_attributes == INVALID_FILE_ATTRIBUTES
        || source_attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
    {
        anyhow::bail!(
            "service binary source {} must be a regular non-symlink file",
            source.display()
        );
    }

    let (directory, created_directories) = ensure_windows_service_directories()?;
    let destination = windows_service_binary_path_in(
        directory.parent().and_then(Path::parent).ok_or_else(|| {
            anyhow!(
                "invalid protected service directory {}",
                directory.display()
            )
        })?,
        mode,
    );

    if destination.exists() {
        windows_path_has_no_reparse_components(&destination)?;
        if !std::fs::symlink_metadata(&destination)?
            .file_type()
            .is_file()
        {
            anyhow::bail!(
                "existing protected service binary {} must be a regular non-reparse file",
                destination.display()
            );
        }
        if !windows_security_matches_expected(&destination, false)? {
            anyhow::bail!(
                "existing protected service binary {} has an unsafe ACL; remove it manually and rerun setup",
                destination.display()
            );
        }
    }

    let mut source_file = std::fs::File::open(source)
        .with_context(|| format!("opening service binary source {}", source.display()))?;
    let mut staged = tempfile::Builder::new()
        .prefix(".wakezilla-service-stage-")
        .suffix(".exe")
        .tempfile_in(&directory)
        .with_context(|| format!("creating private service stage in {}", directory.display()))?;
    std::io::copy(&mut source_file, staged.as_file_mut())?;
    staged.as_file().sync_all()?;
    source_file.rewind()?;
    staged.as_file_mut().rewind()?;
    if sha256_reader(&mut source_file)? != sha256_reader(staged.as_file_mut())? {
        anyhow::bail!(
            "protected service stage verification failed for {}",
            source.display()
        );
    }
    apply_windows_protected_security(staged.path(), false)?;
    staged.as_file().sync_all()?;

    let backup = if destination.exists() {
        let temporary = tempfile::Builder::new()
            .prefix(".wakezilla-service-backup-")
            .suffix(".exe")
            .tempfile_in(&directory)?;
        let backup_path = temporary.path().to_path_buf();
        drop(temporary);
        windows_rename_with_retry(&destination, &backup_path)
            .with_context(|| format!("backing up protected binary {}", destination.display()))?;
        Some(backup_path)
    } else {
        None
    };

    if let Err(error) = staged.persist_noclobber(&destination) {
        if let Some(backup_path) = backup.as_ref() {
            let _ = windows_rename_with_retry(backup_path, &destination);
        }
        return Err(error.error)
            .with_context(|| format!("publishing protected binary {}", destination.display()));
    }
    let update = WindowsProtectedBinaryUpdate {
        destination,
        backup,
        created_directories,
        finished: false,
    };
    let validation = (|| -> Result<()> {
        windows_path_has_no_reparse_components(&update.destination)?;
        if !windows_security_matches_expected(&update.destination, false)? {
            anyhow::bail!(
                "published protected service binary {} has an unsafe ACL",
                update.destination.display()
            );
        }
        Ok(())
    })();
    if let Err(error) = validation {
        return match update.rollback() {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(anyhow!(
                "{error:#}; additionally failed to roll back the invalid protected binary: {rollback_error:#}"
            )),
        };
    }
    Ok(update)
}

#[cfg(target_os = "windows")]
fn validate_windows_protected_binary(mode: Mode) -> Result<PathBuf> {
    let path = protected_service_binary_path(mode)?;
    windows_path_has_no_reparse_components(&path)?;
    if !std::fs::symlink_metadata(&path)?.file_type().is_file()
        || !windows_security_matches_expected(&path, false)?
    {
        anyhow::bail!(
            "protected Windows service binary {} is missing or has unsafe metadata",
            path.display()
        );
    }
    Ok(path)
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, Copy, Default)]
struct WindowsServiceUpdateState {
    was_installed: bool,
    was_running: bool,
    used_protected_binary: bool,
}

#[cfg(target_os = "windows")]
fn windows_service_error_is_absent(error: &windows_service::Error) -> bool {
    use windows_sys::Win32::Foundation::ERROR_SERVICE_DOES_NOT_EXIST;

    matches!(
        error,
        windows_service::Error::Winapi(io_error)
            if io_error.raw_os_error() == Some(ERROR_SERVICE_DOES_NOT_EXIST as i32)
    )
}

#[cfg(target_os = "windows")]
fn prepare_windows_service_for_binary_update(mode: Mode) -> Result<WindowsServiceUpdateState> {
    use windows_service::service::{ServiceAccess, ServiceState};

    let service = match open_windows_service(
        mode,
        ServiceAccess::QUERY_CONFIG | ServiceAccess::QUERY_STATUS | ServiceAccess::STOP,
    ) {
        Ok(service) => service,
        Err(error) => {
            if error
                .downcast_ref::<windows_service::Error>()
                .is_some_and(windows_service_error_is_absent)
            {
                return Ok(WindowsServiceUpdateState::default());
            }
            return Err(error).context("inspecting existing Windows service before update");
        }
    };
    let config = service.query_config()?;
    let status = service.query_status()?;
    let program_files = windows_program_files_known_folder()?;
    let used_protected_binary =
        windows_image_path_uses_protected_binary(&program_files, mode, &config.executable_path)
            && validate_windows_protected_binary(mode).is_ok();
    let was_running = status.current_state != ServiceState::Stopped;
    drop(service);

    if was_running {
        // Stopping by SCM name does not execute the configured ImagePath. A
        // legacy service is deliberately left stopped unless migration succeeds.
        stop_windows_service_for_restart(mode, Duration::from_secs(30))?;
    }

    Ok(WindowsServiceUpdateState {
        was_installed: true,
        was_running,
        used_protected_binary,
    })
}

#[cfg(target_os = "windows")]
fn restore_windows_service_after_failed_update(
    mode: Mode,
    state: WindowsServiceUpdateState,
    service_configuration_changed: bool,
    port: u16,
) -> Result<()> {
    if !state.was_installed || !state.used_protected_binary {
        if service_configuration_changed {
            uninstall_windows_service(mode).context(
                "removing partial protected Windows service after failed legacy migration",
            )?;
        }
        return Ok(());
    }

    if service_configuration_changed {
        install_windows_service(mode)
            .context("restoring the prior protected Windows service definition")?;
    }
    configure_firewall(mode, port).context("restoring the protected Windows firewall rule")?;
    if state.was_running {
        start(mode).context("restarting the restored protected Windows service")?;
    }
    Ok(())
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
/// `source` is copied into the platform's fixed protected location before any
/// service manager is configured. The protected copy is restored on failure.
pub fn install(mode: Mode, source: &Path, port: u16) -> Result<()> {
    #[cfg(target_os = "windows")]
    let windows_state = prepare_windows_service_for_binary_update(mode)?;
    #[cfg(target_os = "windows")]
    let mut windows_service_configuration_changed = false;

    let update = match begin_protected_binary_update(mode, source) {
        Ok(update) => update,
        Err(error) => {
            #[cfg(target_os = "windows")]
            {
                if let Err(restore_error) =
                    restore_windows_service_after_failed_update(mode, windows_state, false, port)
                {
                    return Err(anyhow!(
                        "{error:#}; additionally failed to restore the stopped Windows service: {restore_error:#}"
                    ));
                }
            }
            return Err(error);
        }
    };
    let install_result = (|| -> Result<()> {
        configure_firewall(mode, port).context("configuring protected service firewall rule")?;

        #[cfg(target_os = "linux")]
        {
            let unit = generate_systemd_unit(mode);
            let path = format!("/etc/systemd/system/{}.service", mode.service_name());
            write_unix_service_descriptor(Path::new(&path), &unit)
                .with_context(|| format!("writing {path}"))?;
            run("systemctl", &["daemon-reload"])?;
            run("systemctl", &["enable", mode.service_name()])?;
            restart(mode)?;
        }
        #[cfg(target_os = "macos")]
        {
            // Ensure the log directory exists so launchd can redirect stdout/stderr.
            prepare_unix_service_directory(Path::new(MACOS_LOG_DIR))
                .with_context(|| format!("creating log dir {MACOS_LOG_DIR}"))?;
            let plist = generate_launchd_plist(mode);
            let path = format!("/Library/LaunchDaemons/{}.plist", mode.launchd_label());
            write_unix_service_descriptor(Path::new(&path), &plist)
                .with_context(|| format!("writing {path}"))?;
            // Unload any previous instance (ignored if not loaded) so the updated
            // plist is reloaded on a re-run instead of erroring "already loaded".
            run_ignore_err("launchctl", &["unload", &path]);
            start(mode)?;
        }
        #[cfg(target_os = "windows")]
        {
            windows_service_configuration_changed = true;
            install_windows_service(mode)?;
            start(mode)?;
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            let _ = mode;
            return Err(anyhow!("service install not supported on this OS"));
        }

        validate(port, 10).context("service installed but did not become reachable")
    })();

    match install_result {
        Ok(()) => update
            .commit()
            .context("committing protected service binary"),
        Err(install_error) => {
            let _ = remove_firewall(mode);
            #[cfg(target_os = "windows")]
            if windows_service_configuration_changed {
                if let Err(cleanup_error) = uninstall_windows_service(mode) {
                    return Err(anyhow!(
                        "{install_error:#}; additionally failed to stop/remove the partial Windows service before protected-binary rollback: {cleanup_error:#}"
                    ));
                }
            }
            let rollback_result = update.rollback();
            #[cfg(target_os = "windows")]
            let service_restore_result = if rollback_result.is_ok() {
                restore_windows_service_after_failed_update(
                    mode,
                    windows_state,
                    windows_service_configuration_changed,
                    port,
                )
            } else {
                Ok(())
            };

            match rollback_result {
                Err(rollback_error) => Err(anyhow!(
                    "{install_error:#}; additionally failed to restore the prior protected service binary: {rollback_error:#}"
                )),
                Ok(()) => {
                    #[cfg(target_os = "windows")]
                    if let Err(restore_error) = service_restore_result {
                        return Err(anyhow!(
                            "{install_error:#}; the binary was restored, but the prior protected Windows service state could not be restored: {restore_error:#}"
                        ));
                    }
                    Err(install_error)
                }
            }
        }
    }
}

/// Modes that can be installed and removed by the setup/uninstall workflow.
pub fn managed_modes() -> [Mode; 2] {
    [Mode::Proxy, Mode::Client]
}

/// Remove the system service/autostart configuration for one Wakezilla mode.
///
/// This intentionally leaves Wakezilla config, data, and logs in place.
pub fn uninstall(mode: Mode) -> Result<()> {
    #[cfg(target_os = "linux")]
    let service_result: Result<()> = {
        run_ignore_err("systemctl", &["stop", mode.service_name()]);
        run_ignore_err("systemctl", &["disable", mode.service_name()]);
        remove_file_if_exists(&descriptor_path(mode))?;
        run("systemctl", &["daemon-reload"])?;
        run_ignore_err("systemctl", &["reset-failed", mode.service_name()]);
        Ok(())
    };
    #[cfg(target_os = "macos")]
    let service_result = {
        let path = descriptor_path(mode);
        run_ignore_err("launchctl", &["unload", &path]);
        remove_file_if_exists(&path)
    };
    #[cfg(target_os = "windows")]
    let service_result = uninstall_windows_service(mode);
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    let service_result = Err(anyhow!("service uninstall not supported on this OS"));

    let firewall_result = remove_firewall(mode);
    service_result?;
    firewall_result?;
    remove_protected_service_binary(mode)?;
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn remove_protected_service_binary(mode: Mode) -> Result<()> {
    let binary = protected_service_binary_path(mode)?;
    if let Some(parent) = binary.parent() {
        match std::fs::symlink_metadata(parent) {
            Ok(metadata) if metadata.file_type().is_dir() => {}
            Ok(_) => anyhow::bail!(
                "protected service directory {} must be a real directory",
                parent.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "inspecting protected service directory {}",
                        parent.display()
                    )
                });
            }
        }
    }
    #[cfg(target_os = "linux")]
    let remove_parent = true;
    #[cfg(target_os = "macos")]
    let remove_parent = false;
    remove_unix_protected_binary_at(&binary, remove_parent, 0, 0)
}

#[cfg(target_os = "windows")]
fn remove_protected_service_binary(mode: Mode) -> Result<()> {
    let program_files = windows_program_files_known_folder()?;
    let binary = windows_service_binary_path_in(&program_files, mode);
    match std::fs::symlink_metadata(&binary) {
        Ok(metadata) => {
            windows_path_has_no_reparse_components(&binary)?;
            if !metadata.file_type().is_file()
                || !windows_security_matches_expected(&binary, false)?
            {
                anyhow::bail!(
                    "refusing to remove unsafe or foreign protected Windows binary {}",
                    binary.display()
                );
            }
            windows_remove_file_with_retry(&binary).with_context(|| {
                format!("removing protected service binary {}", binary.display())
            })?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error)
                .with_context(|| format!("inspecting protected binary {}", binary.display()));
        }
    }

    let service = program_files.join("Wakezilla").join("Service");
    let wakezilla = program_files.join("Wakezilla");
    for directory in [&service, &wakezilla] {
        match std::fs::symlink_metadata(directory) {
            Ok(metadata) if metadata.file_type().is_dir() => {
                windows_path_has_no_reparse_components(directory)?;
                match std::fs::remove_dir(directory) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::DirectoryNotEmpty => {}
                    Err(error) => {
                        return Err(error).with_context(|| {
                            format!("removing empty service directory {}", directory.display())
                        });
                    }
                }
            }
            Ok(_) => anyhow::bail!(
                "protected Windows service directory {} must not be a reparse point",
                directory.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("inspecting service directory {}", directory.display())
                });
            }
        }
    }
    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn remove_protected_service_binary(mode: Mode) -> Result<()> {
    let _ = mode;
    Ok(())
}

/// Remove all Wakezilla services installed by setup.
pub fn uninstall_all() -> Result<Vec<Mode>> {
    let mut removed = Vec::new();
    for mode in managed_modes() {
        let was_installed = is_installed(mode);
        uninstall(mode).with_context(|| format!("failed to uninstall {}", mode.subcommand()))?;
        if was_installed {
            removed.push(mode);
        }
    }
    Ok(removed)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn remove_file_if_exists(path: &str) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).with_context(|| format!("removing {path}")),
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

/// Run a command and capture stdout/stderr for callers that need to distinguish
/// expected non-zero exits from real failures.
#[cfg(target_os = "windows")]
fn run_output(cmd: &str, args: &[&str]) -> Result<std::process::Output> {
    Command::new(cmd)
        .args(args)
        .output()
        .with_context(|| format!("failed to run {cmd}"))
}

#[cfg(target_os = "windows")]
fn command_output_text(output: &std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let text = format!("{stdout}{stderr}");
    let text = text.trim();
    if text.is_empty() {
        output.status.to_string()
    } else {
        text.to_string()
    }
}

#[cfg(target_os = "windows")]
fn netsh_rule_not_found(output: &std::process::Output) -> bool {
    let text = command_output_text(output).to_ascii_lowercase();
    text.contains("no rules match") || text.contains("nenhuma regra")
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
    managed_modes()
        .into_iter()
        .filter(|m| is_installed(*m))
        .collect()
}

/// Start the installed service for `mode`.
pub fn start(mode: Mode) -> Result<()> {
    ensure_service_uses_protected_binary(mode)?;
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
    ensure_service_uses_protected_binary(mode)?;
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
        stop_windows_service_for_restart(mode, Duration::from_secs(15))?;
        start(mode)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = mode;
        Err(anyhow!("service control not supported on this OS"))
    }
}

fn ensure_service_uses_protected_binary(mode: Mode) -> Result<()> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        ensure_unix_descriptor_uses_protected_binary(mode)
    }
    #[cfg(target_os = "windows")]
    {
        use windows_service::service::ServiceAccess;

        let protected = validate_windows_protected_binary(mode)?;
        let service = open_windows_service(mode, ServiceAccess::QUERY_CONFIG)?;
        let config = service.query_config()?;
        let program_files = windows_program_files_known_folder()?;
        if !windows_image_path_uses_protected_binary(&program_files, mode, &config.executable_path)
            || !windows_image_path_uses_protected_binary(&program_files, mode, &protected)
        {
            anyhow::bail!(
                "unsafe legacy {} Windows service ImagePath {}; run `wakezilla setup --mode {} --port <port> --yes` from an elevated shell to migrate it to the protected binary",
                mode.subcommand(),
                config.executable_path.display(),
                mode.service_arg()
            );
        }
        Ok(())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = mode;
        Err(anyhow!("service validation not supported on this OS"))
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
fn install_windows_service(mode: Mode) -> Result<()> {
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
        executable_path: validate_windows_protected_binary(mode)?,
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
fn stop_windows_service_for_restart(mode: Mode, timeout: Duration) -> Result<()> {
    use windows_service::service::{ServiceAccess, ServiceState};

    let service = open_windows_service(mode, ServiceAccess::QUERY_STATUS | ServiceAccess::STOP)?;
    let status = service.query_status()?;
    if status.current_state == ServiceState::Stopped {
        return Ok(());
    }
    if status.current_state != ServiceState::StopPending {
        service.stop()?;
    }

    let deadline = Instant::now() + timeout;
    loop {
        let status = service.query_status()?;
        if status.current_state == ServiceState::Stopped {
            return Ok(());
        }
        if Instant::now() >= deadline {
            anyhow::bail!(
                "{} service did not stop within {:?}; current state: {:?}",
                mode.subcommand(),
                timeout,
                status.current_state
            );
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

#[cfg(target_os = "windows")]
fn uninstall_windows_service(mode: Mode) -> Result<()> {
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

    let service_manager =
        ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
    delete_windows_service_if_exists(&service_manager, mode)
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
        Err(error) if windows_service_error_is_absent(&error) => return Ok(()),
        Err(error) => return Err(error.into()),
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
        match service_manager.open_service(mode.service_name(), ServiceAccess::QUERY_STATUS) {
            Ok(_) => {}
            Err(error) if windows_service_error_is_absent(&error) => return Ok(()),
            Err(error) => return Err(error.into()),
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    anyhow::bail!(
        "Windows service {} remained pending deletion; wait for it to stop and rerun uninstall",
        mode.service_name()
    )
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
                crate::client_server::start_with_shutdown(
                    config.server.client_port,
                    config.security.client_shutdown_key.clone(),
                    shutdown,
                )
                .await
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
        let path = service_log_path(mode);
        if !path.exists() {
            anyhow::bail!(
                "no log file at {} yet. The service may not have started or \
                 produced any output.",
                path.display()
            );
        }
        print_last_log_lines(&path, lines)?;
        if follow {
            follow_log_file(&path)?;
        }
        Ok(())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = (mode, follow, lines);
        Err(anyhow!("log viewing not supported on this OS"))
    }
}

#[cfg(target_os = "windows")]
fn print_last_log_lines(path: &Path, line_count: u32) -> Result<()> {
    if line_count == 0 {
        return Ok(());
    }

    let tail = read_tail_for_lines(path, line_count)?;
    let contents = String::from_utf8_lossy(&tail);
    let lines: Vec<&str> = contents.lines().collect();
    let start = lines.len().saturating_sub(line_count as usize);
    for line in &lines[start..] {
        println!("{line}");
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn read_tail_for_lines(path: &Path, line_count: u32) -> Result<Vec<u8>> {
    const CHUNK_SIZE: u64 = 8192;

    let mut file =
        std::fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut remaining = file
        .seek(SeekFrom::End(0))
        .with_context(|| format!("failed to seek {}", path.display()))?;
    let mut newline_count = 0usize;
    let mut chunks = VecDeque::new();

    while remaining > 0 && newline_count <= line_count as usize {
        let read_len = remaining.min(CHUNK_SIZE);
        remaining -= read_len;
        file.seek(SeekFrom::Start(remaining))
            .with_context(|| format!("failed to seek {}", path.display()))?;

        let mut chunk = vec![0; read_len as usize];
        file.read_exact(&mut chunk)
            .with_context(|| format!("failed to read {}", path.display()))?;
        newline_count += chunk.iter().filter(|byte| **byte == b'\n').count();
        chunks.push_front(chunk);
    }

    let total_len = chunks.iter().map(Vec::len).sum();
    let mut tail = Vec::with_capacity(total_len);
    for chunk in chunks {
        tail.extend(chunk);
    }
    Ok(tail)
}

#[cfg(target_os = "windows")]
fn follow_log_file(path: &Path) -> Result<()> {
    let mut file =
        std::fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    file.seek(SeekFrom::End(0))
        .with_context(|| format!("failed to seek {}", path.display()))?;

    loop {
        let mut chunk = String::new();
        let bytes = file
            .read_to_string(&mut chunk)
            .with_context(|| format!("failed to read {}", path.display()))?;
        if bytes > 0 {
            print!("{chunk}");
            std::io::stdout().flush().ok();
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};

    #[cfg(unix)]
    #[test]
    fn protected_binary_update_publishes_verified_bytes_with_secure_metadata() {
        let temp = tempfile::tempdir().expect("tempdir");
        let source = temp.path().join("wakezilla-source");
        let destination = temp.path().join("service").join("wakezilla-proxy");
        std::fs::create_dir(destination.parent().unwrap()).expect("service directory");
        std::fs::write(&source, b"new protected executable").expect("source");
        std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o700))
            .expect("source mode");

        let update = begin_unix_protected_binary_update_at(
            &source,
            &destination,
            unsafe { libc::geteuid() },
            unsafe { libc::getegid() },
        )
        .expect("begin protected update");

        assert_eq!(
            std::fs::read(&destination).expect("published bytes"),
            b"new protected executable"
        );
        let metadata = std::fs::symlink_metadata(&destination).expect("published metadata");
        assert!(metadata.file_type().is_file());
        assert_eq!(metadata.permissions().mode() & 0o777, 0o755);
        assert_eq!(metadata.uid(), unsafe { libc::geteuid() });
        assert_eq!(metadata.gid(), unsafe { libc::getegid() });

        update.commit().expect("commit protected update");
        assert_eq!(
            std::fs::read_dir(destination.parent().unwrap())
                .expect("read service directory")
                .count(),
            1,
            "private staging and backup files must not remain"
        );
    }

    #[cfg(unix)]
    #[test]
    fn protected_binary_update_rejects_symlink_source_without_publication() {
        let temp = tempfile::tempdir().expect("tempdir");
        let real_source = temp.path().join("real-wakezilla");
        let source = temp.path().join("wakezilla-source");
        let destination = temp.path().join("service").join("wakezilla-client");
        std::fs::create_dir(destination.parent().unwrap()).expect("service directory");
        std::fs::write(&real_source, b"outside bytes").expect("real source");
        symlink(&real_source, &source).expect("source symlink");

        let error = begin_unix_protected_binary_update_at(
            &source,
            &destination,
            unsafe { libc::geteuid() },
            unsafe { libc::getegid() },
        )
        .expect_err("symlink source must fail closed");

        assert!(error.to_string().contains("regular non-symlink"));
        assert!(!destination.exists());
        assert_eq!(std::fs::read(&real_source).unwrap(), b"outside bytes");
    }

    #[cfg(unix)]
    #[test]
    fn protected_binary_update_restores_prior_binary_after_late_failure() {
        let temp = tempfile::tempdir().expect("tempdir");
        let source = temp.path().join("wakezilla-source");
        let destination = temp.path().join("service").join("wakezilla-proxy");
        std::fs::create_dir(destination.parent().unwrap()).expect("service directory");
        std::fs::write(&source, b"new executable").expect("source");
        std::fs::write(&destination, b"prior protected executable").expect("prior binary");
        std::fs::set_permissions(&destination, std::fs::Permissions::from_mode(0o755))
            .expect("prior mode");

        let update = begin_unix_protected_binary_update_at(
            &source,
            &destination,
            unsafe { libc::geteuid() },
            unsafe { libc::getegid() },
        )
        .expect("begin protected update");
        assert_eq!(std::fs::read(&destination).unwrap(), b"new executable");

        update.rollback().expect("rollback protected update");

        assert_eq!(
            std::fs::read(&destination).expect("restored prior binary"),
            b"prior protected executable"
        );
        assert_eq!(
            std::fs::symlink_metadata(&destination)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
        assert_eq!(
            std::fs::read_dir(destination.parent().unwrap())
                .expect("read service directory")
                .count(),
            1
        );
    }

    #[cfg(unix)]
    #[test]
    fn secure_service_directory_rejects_symlink_destination() {
        let temp = tempfile::tempdir().expect("tempdir");
        let outside = temp.path().join("outside");
        let protected = temp.path().join("protected");
        std::fs::create_dir(&outside).expect("outside");
        symlink(&outside, &protected).expect("protected symlink");

        let error =
            ensure_unix_service_directory_at(&protected, unsafe { libc::geteuid() }, unsafe {
                libc::getegid()
            })
            .expect_err("symlinked protected directory must fail closed");

        assert!(error.to_string().contains("real directory"));
        assert!(outside.exists());
        assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn protected_binary_removal_is_exact_and_only_removes_empty_owned_parent() {
        let temp = tempfile::tempdir().expect("tempdir");
        let service_dir = temp.path().join("wakezilla");
        let binary = service_dir.join("wakezilla-proxy");
        let sibling = service_dir.join("wakezilla-client");
        let preserved_data = temp.path().join("config-and-history");
        std::fs::create_dir(&service_dir).expect("service dir");
        std::fs::write(&binary, b"proxy").expect("proxy binary");
        std::fs::write(&sibling, b"client").expect("client binary");
        std::fs::write(&preserved_data, b"do not delete").expect("preserved data");
        for path in [&binary, &sibling] {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
                .expect("binary mode");
        }

        remove_unix_protected_binary_at(&binary, true, unsafe { libc::geteuid() }, unsafe {
            libc::getegid()
        })
        .expect("remove first mode");
        assert!(!binary.exists());
        assert_eq!(std::fs::read(&sibling).unwrap(), b"client");
        assert!(service_dir.exists(), "non-empty parent must remain");
        assert_eq!(std::fs::read(&preserved_data).unwrap(), b"do not delete");

        remove_unix_protected_binary_at(&sibling, true, unsafe { libc::geteuid() }, unsafe {
            libc::getegid()
        })
        .expect("remove second mode");
        assert!(
            !service_dir.exists(),
            "empty owned parent should be removed"
        );
        assert_eq!(std::fs::read(&preserved_data).unwrap(), b"do not delete");
    }

    #[cfg(unix)]
    #[test]
    fn service_control_validation_rejects_legacy_user_writable_descriptor() {
        let temp = tempfile::tempdir().expect("tempdir");
        let binary = temp.path().join("wakezilla-proxy");
        let descriptor = temp.path().join("wakezilla-proxy.service");
        std::fs::write(&binary, b"protected executable").expect("binary");
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755))
            .expect("binary mode");
        let canonical = generate_systemd_unit(Mode::Proxy);
        std::fs::write(&descriptor, &canonical).expect("descriptor");
        std::fs::set_permissions(&descriptor, std::fs::Permissions::from_mode(0o644))
            .expect("descriptor mode");

        validate_unix_service_files_at(
            &binary,
            &descriptor,
            &canonical,
            unsafe { libc::geteuid() },
            unsafe { libc::getegid() },
        )
        .expect("canonical protected service files");

        let legacy = canonical.replace(
            "/usr/local/libexec/wakezilla/wakezilla-proxy",
            "/home/alice/.local/bin/wakezilla",
        );
        std::fs::write(&descriptor, legacy).expect("legacy descriptor");
        let error = validate_unix_service_files_at(
            &binary,
            &descriptor,
            &canonical,
            unsafe { libc::geteuid() },
            unsafe { libc::getegid() },
        )
        .expect_err("legacy descriptor must fail closed");
        assert!(error.to_string().contains("unsafe legacy"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_program_files_is_resolved_by_known_folder_and_has_no_reparse_components() {
        let program_files =
            windows_program_files_known_folder().expect("Program Files known folder");
        assert!(program_files.is_absolute());
        assert!(windows_path_has_no_reparse_components(&program_files).is_ok());
        assert_eq!(
            protected_service_binary_path(Mode::Proxy).expect("protected proxy path"),
            windows_service_binary_path_in(&program_files, Mode::Proxy)
        );
        assert_eq!(
            protected_service_binary_path(Mode::Client).expect("protected client path"),
            windows_service_binary_path_in(&program_files, Mode::Client)
        );
    }
}
