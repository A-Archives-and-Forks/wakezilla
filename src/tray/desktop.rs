use crate::{config, service, update};
use anyhow::{anyhow, Context, Result};
#[cfg(target_os = "windows")]
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use std::io::Cursor;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::os::fd::AsRawFd;
#[cfg(target_os = "linux")]
use std::os::fd::{FromRawFd, OwnedFd};
#[cfg(target_os = "macos")]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu},
    Icon, TrayIcon, TrayIconBuilder,
};
#[cfg(target_os = "macos")]
use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
use winit::{
    application::ApplicationHandler,
    event::{StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    window::WindowId,
};

const TRAY_INSTANCE_NAME: &str = "dev.wakezilla.tray";
const OPEN_DASHBOARD_ID: &str = "open_dashboard";
const COPY_DASHBOARD_URL_ID: &str = "copy_dashboard_url";
const SETUP_ID: &str = "setup_services";
const CHECK_UPDATES_ID: &str = "check_updates";
const QUIT_ID: &str = "quit_tray";
const PROXY_START_ID: &str = "proxy_start";
const PROXY_STOP_ID: &str = "proxy_stop";
const PROXY_RESTART_ID: &str = "proxy_restart";
const PROXY_LOGS_ID: &str = "proxy_logs";
const CLIENT_START_ID: &str = "client_start";
const CLIENT_STOP_ID: &str = "client_stop";
const CLIENT_RESTART_ID: &str = "client_restart";
const CLIENT_LOGS_ID: &str = "client_logs";

#[derive(Debug)]
enum UserEvent {
    Menu(String),
    Refresh,
    Status(ServiceStatuses),
    Message(String),
}

#[derive(Debug, Clone, Copy)]
enum ServiceControl {
    Start,
    Stop,
    Restart,
}

struct ModeMenu {
    status: MenuItem,
    start: MenuItem,
    stop: MenuItem,
    restart: MenuItem,
    logs: MenuItem,
}

struct TrayMenu {
    message: MenuItem,
    proxy: ModeMenu,
    client: ModeMenu,
}

struct TrayApp {
    dashboard_url: String,
    proxy: EventLoopProxy<UserEvent>,
    menu: Option<TrayMenu>,
    tray_icon: Option<TrayIcon>,
    startup_error: Option<String>,
    status_refresh_in_flight: bool,
}

struct TrayInstanceGuard {
    #[cfg(target_os = "linux")]
    _socket: OwnedFd,
    #[cfg(target_os = "windows")]
    _instance: single_instance::SingleInstance,
    #[cfg(target_os = "macos")]
    _lock_file: std::fs::File,
}

impl TrayInstanceGuard {
    fn acquire_named(name: &str) -> Result<Option<Self>> {
        #[cfg(target_os = "linux")]
        {
            Self::acquire_linux(name)
        }

        #[cfg(target_os = "windows")]
        {
            let backend_name = name.to_owned();
            let instance =
                single_instance::SingleInstance::new(&backend_name).with_context(|| {
                    format!("failed to acquire tray instance lock `{name}` as `{backend_name}`")
                })?;
            if !instance.is_single() {
                return Ok(None);
            }

            Ok(Some(Self {
                _instance: instance,
            }))
        }

        #[cfg(target_os = "macos")]
        {
            Self::acquire_macos(name)
        }
    }

    #[cfg(target_os = "linux")]
    fn acquire_linux(name: &str) -> Result<Option<Self>> {
        let backend_name = linux_backend_name(name, effective_uid());
        let socket_type = combine_linux_socket_type(libc::SOCK_STREAM, libc::SOCK_CLOEXEC);

        // SAFETY: socket is called with valid Linux domain/type/protocol constants and no
        // pointers. A nonnegative result is a newly owned descriptor.
        let raw_socket = unsafe { libc::socket(libc::AF_UNIX, socket_type, 0) };
        if raw_socket < 0 {
            return Err(std::io::Error::last_os_error()).with_context(|| {
                format!("failed to create tray instance socket `{backend_name}`")
            });
        }
        // SAFETY: raw_socket was just returned as an owned descriptor and has not been wrapped
        // or closed. OwnedFd now closes it on every return path.
        let socket = unsafe { OwnedFd::from_raw_fd(raw_socket) };
        let (address, address_len) = linux_abstract_socket_address(&backend_name)?;

        // SAFETY: socket is a valid AF_UNIX descriptor; address points to an initialized
        // sockaddr_un and address_len covers only its family and populated abstract name.
        let rc = unsafe {
            libc::bind(
                socket.as_raw_fd(),
                std::ptr::addr_of!(address).cast::<libc::sockaddr>(),
                address_len,
            )
        };
        if rc == 0 {
            return Ok(Some(Self { _socket: socket }));
        }

        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::EADDRINUSE) {
            Ok(None)
        } else {
            Err(error)
                .with_context(|| format!("failed to bind tray instance socket `{backend_name}`"))
        }
    }

    #[cfg(target_os = "macos")]
    fn acquire_macos(name: &str) -> Result<Option<Self>> {
        let lock_path = macos_lock_path(name)?;
        let mut options = std::fs::OpenOptions::new();
        options.read(true).write(true).create(true);
        options.mode(0o600);
        options.custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW);
        let lock_file = options
            .open(&lock_path)
            .with_context(|| format!("failed to open tray lock {}", lock_path.display()))?;
        lock_file
            .set_permissions(std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("failed to secure tray lock {}", lock_path.display()))?;

        // SAFETY: lock_file owns a valid descriptor for this call, and flock does not retain it
        // or access Rust-managed memory.
        let rc = unsafe { libc::flock(lock_file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        let errno = if rc == 0 {
            0
        } else {
            std::io::Error::last_os_error()
                .raw_os_error()
                .unwrap_or(libc::EIO)
        };

        match classify_macos_flock_result(rc, errno)
            .with_context(|| format!("failed to lock tray instance file {}", lock_path.display()))?
        {
            MacosFlockOutcome::Acquired => Ok(Some(Self {
                _lock_file: lock_file,
            })),
            MacosFlockOutcome::Contended => Ok(None),
        }
    }
}

#[cfg(target_os = "linux")]
fn effective_uid() -> libc::uid_t {
    // SAFETY: geteuid has no preconditions and does not dereference any pointers.
    unsafe { libc::geteuid() }
}

#[cfg(target_os = "linux")]
fn linux_backend_name(name: &str, euid: libc::uid_t) -> String {
    format!("{name}.uid-{euid}")
}

#[cfg(any(target_os = "linux", test))]
fn combine_linux_socket_type(stream: i32, cloexec: i32) -> i32 {
    stream | cloexec
}

#[cfg(target_os = "linux")]
fn linux_abstract_socket_address(name: &str) -> Result<(libc::sockaddr_un, libc::socklen_t)> {
    // SAFETY: sockaddr_un contains only integer fields and a c_char array, for which all-zero is
    // a valid bit pattern. Zeroing also establishes the leading NUL for an abstract address.
    let mut address = unsafe { std::mem::zeroed::<libc::sockaddr_un>() };
    address.sun_family = libc::AF_UNIX as libc::sa_family_t;

    let name_bytes = name.as_bytes();
    let maximum_name_len = address.sun_path.len().saturating_sub(1);
    if name_bytes.len() > maximum_name_len {
        anyhow::bail!(
            "tray instance socket name is {} bytes; maximum is {maximum_name_len}",
            name_bytes.len()
        );
    }
    address.sun_path[0] = 0;
    for (index, byte) in name_bytes.iter().enumerate() {
        address.sun_path[index + 1] = *byte as libc::c_char;
    }

    let address_len = std::mem::offset_of!(libc::sockaddr_un, sun_path) + 1 + name_bytes.len();
    let address_len = libc::socklen_t::try_from(address_len)
        .context("tray instance socket address length overflowed socklen_t")?;
    Ok((address, address_len))
}

#[cfg(target_os = "macos")]
#[derive(Debug, Eq, PartialEq)]
enum MacosFlockOutcome {
    Acquired,
    Contended,
}

#[cfg(target_os = "macos")]
fn classify_macos_flock_result(
    rc: libc::c_int,
    errno: libc::c_int,
) -> std::io::Result<MacosFlockOutcome> {
    if rc == 0 {
        Ok(MacosFlockOutcome::Acquired)
    } else if errno == libc::EWOULDBLOCK {
        Ok(MacosFlockOutcome::Contended)
    } else {
        Err(std::io::Error::from_raw_os_error(errno))
    }
}

#[cfg(target_os = "macos")]
fn macos_lock_path(name: &str) -> Result<PathBuf> {
    let temp_dir = std::env::temp_dir()
        .canonicalize()
        .context("failed to resolve the per-user temporary directory")?;
    let lock_dir = temp_dir.join("Wakezilla");
    let mut builder = std::fs::DirBuilder::new();
    builder.mode(0o700);
    match builder.create(&lock_dir) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => {
            return Err(error).with_context(|| format!("failed to create {}", lock_dir.display()));
        }
    }

    let metadata = std::fs::symlink_metadata(&lock_dir)
        .with_context(|| format!("failed to inspect {}", lock_dir.display()))?;
    if !metadata.file_type().is_dir() {
        anyhow::bail!(
            "tray lock directory is not a directory: {}",
            lock_dir.display()
        );
    }
    if metadata.permissions().mode() & 0o7777 != 0o700 {
        std::fs::set_permissions(&lock_dir, std::fs::Permissions::from_mode(0o700))
            .with_context(|| format!("failed to secure {}", lock_dir.display()))?;
    }

    let file_name = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    Ok(lock_dir.join(format!("instance-{file_name}.lock")))
}

#[derive(Debug, Clone, Copy)]
struct ServiceStatuses {
    proxy: ModeStatus,
    client: ModeStatus,
}

#[derive(Debug, Clone, Copy)]
struct ModeStatus {
    installed: bool,
    running: bool,
}

pub fn run() -> Result<()> {
    let Some(_instance_guard) = TrayInstanceGuard::acquire_named(TRAY_INSTANCE_NAME)? else {
        return Ok(());
    };

    #[cfg(target_os = "linux")]
    gtk::init().context("failed to initialize GTK")?;

    let config = config::Config::load();
    let dashboard_url = dashboard_url(&config);

    let mut builder = EventLoop::<UserEvent>::with_user_event();
    #[cfg(target_os = "macos")]
    builder.with_activation_policy(ActivationPolicy::Accessory);
    let event_loop = builder
        .build()
        .context("failed to create tray event loop")?;
    event_loop.set_control_flow(ControlFlow::Wait);

    let proxy = event_loop.create_proxy();
    install_menu_event_handler(proxy.clone());
    start_refresh_timer(proxy.clone());

    let mut app = TrayApp {
        dashboard_url,
        proxy,
        menu: None,
        tray_icon: None,
        startup_error: None,
        status_refresh_in_flight: false,
    };

    event_loop
        .run_app(&mut app)
        .context("tray event loop failed")?;

    if let Some(error) = app.startup_error {
        anyhow::bail!(error);
    }
    Ok(())
}

fn install_menu_event_handler(proxy: EventLoopProxy<UserEvent>) {
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let _ = proxy.send_event(UserEvent::Menu(event.id().as_ref().to_string()));
    }));
}

fn start_refresh_timer(proxy: EventLoopProxy<UserEvent>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(10));
        if proxy.send_event(UserEvent::Refresh).is_err() {
            break;
        }
    });
}

impl ApplicationHandler<UserEvent> for TrayApp {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        if !matches!(cause, StartCause::Init) || self.tray_icon.is_some() {
            return;
        }

        if let Err(error) = self.create_tray_icon() {
            self.startup_error = Some(error.to_string());
            tracing::error!("Tray startup failed: {error:#}");
            event_loop.exit();
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Menu(id) => self.handle_menu_event(event_loop, &id),
            UserEvent::Refresh => self.refresh_status(),
            UserEvent::Status(statuses) => self.apply_status(statuses),
            UserEvent::Message(message) => self.set_message(message),
        }
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        _event: WindowEvent,
    ) {
    }
}

impl TrayApp {
    fn create_tray_icon(&mut self) -> Result<()> {
        let (root_menu, tray_menu) = build_menu()?;
        let icon = load_tray_icon()?;

        let tray_icon = TrayIconBuilder::new()
            .with_tooltip("Wakezilla")
            .with_icon(icon)
            .with_menu(Box::new(root_menu))
            .with_menu_on_left_click(true)
            .build()
            .context("failed to build tray icon")?;

        self.menu = Some(tray_menu);
        self.tray_icon = Some(tray_icon);
        self.refresh_status();
        Ok(())
    }

    fn handle_menu_event(&mut self, event_loop: &ActiveEventLoop, id: &str) {
        match id {
            OPEN_DASHBOARD_ID => self.open_dashboard(),
            COPY_DASHBOARD_URL_ID => self.copy_dashboard_url(),
            SETUP_ID => self.configure_startup(),
            CHECK_UPDATES_ID => self.check_for_updates(),
            QUIT_ID => event_loop.exit(),
            PROXY_START_ID => self.run_service_control(service::Mode::Proxy, ServiceControl::Start),
            PROXY_STOP_ID => self.run_service_control(service::Mode::Proxy, ServiceControl::Stop),
            PROXY_RESTART_ID => {
                self.run_service_control(service::Mode::Proxy, ServiceControl::Restart)
            }
            PROXY_LOGS_ID => self.open_logs(service::Mode::Proxy),
            CLIENT_START_ID => {
                self.run_service_control(service::Mode::Client, ServiceControl::Start)
            }
            CLIENT_STOP_ID => self.run_service_control(service::Mode::Client, ServiceControl::Stop),
            CLIENT_RESTART_ID => {
                self.run_service_control(service::Mode::Client, ServiceControl::Restart)
            }
            CLIENT_LOGS_ID => self.open_logs(service::Mode::Client),
            _ => {}
        }
    }

    fn open_dashboard(&mut self) {
        match open::that(&self.dashboard_url) {
            Ok(()) => self.set_message(format!("Opened {}", self.dashboard_url)),
            Err(error) => self.set_message(format!("Failed to open dashboard: {error}")),
        }
    }

    fn copy_dashboard_url(&mut self) {
        let result = arboard::Clipboard::new()
            .and_then(|mut clipboard| clipboard.set_text(self.dashboard_url.clone()));

        match result {
            Ok(()) => self.set_message("Dashboard URL copied.".to_string()),
            Err(error) => self.set_message(format!("Failed to copy dashboard URL: {error}")),
        }
    }

    fn configure_startup(&mut self) {
        let autostart = install_tray_autostart();
        let setup = open_wakezilla_command(true, &["setup"], true);

        match (autostart, setup) {
            (Ok(path), Ok(())) => self.set_message(format!(
                "Tray autostart installed at {}; opened service setup.",
                path.display()
            )),
            (Ok(path), Err(error)) => self.set_message(format!(
                "Tray autostart installed at {}; failed to open service setup: {error}",
                path.display()
            )),
            (Err(error), Ok(())) => self.set_message(format!(
                "Failed to install tray autostart: {error}; opened service setup."
            )),
            (Err(autostart_error), Err(setup_error)) => self.set_message(format!(
                "Startup setup failed: {autostart_error}; service setup failed: {setup_error}"
            )),
        }
    }

    fn open_logs(&mut self, mode: service::Mode) {
        let result = open_wakezilla_command(
            true,
            &[
                "--no-update-check",
                "service",
                "logs",
                "--mode",
                mode.service_arg(),
                "--lines",
                "100",
            ],
            true,
        );

        match result {
            Ok(()) => self.set_message(format!("Opened {} logs.", mode_label(mode))),
            Err(error) => {
                self.set_message(format!("Failed to open {} logs: {error}", mode_label(mode)))
            }
        }
    }

    fn check_for_updates(&mut self) {
        self.set_message("Checking for updates...".to_string());
        let proxy = self.proxy.clone();

        std::thread::spawn(move || {
            let message = match check_latest_version() {
                Ok(message) => message,
                Err(error) => format!("Update check failed: {error}"),
            };
            let _ = proxy.send_event(UserEvent::Message(message));
        });
    }

    fn run_service_control(&mut self, mode: service::Mode, control: ServiceControl) {
        self.set_message(format!(
            "{} {} requested...",
            mode_label(mode),
            control.verb()
        ));

        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            let message = match run_service_control(mode, control) {
                Ok(message) => message,
                Err(error) => format!("{} {} failed: {error}", mode_label(mode), control.verb()),
            };
            let _ = proxy.send_event(UserEvent::Message(message));
            let _ = proxy.send_event(UserEvent::Refresh);
        });
    }

    fn refresh_status(&mut self) {
        if self.menu.is_none() || self.status_refresh_in_flight {
            return;
        }

        self.status_refresh_in_flight = true;
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            let statuses = ServiceStatuses {
                proxy: query_mode_status(service::Mode::Proxy),
                client: query_mode_status(service::Mode::Client),
            };
            let _ = proxy.send_event(UserEvent::Status(statuses));
        });
    }

    fn apply_status(&mut self, statuses: ServiceStatuses) {
        self.status_refresh_in_flight = false;
        if let Some(menu) = &self.menu {
            update_mode_menu(service::Mode::Proxy, &menu.proxy, statuses.proxy);
            update_mode_menu(service::Mode::Client, &menu.client, statuses.client);
        }
    }

    fn set_message(&mut self, message: String) {
        if let Some(menu) = &self.menu {
            menu.message.set_text(message);
        }
    }
}

impl ServiceControl {
    fn verb(self) -> &'static str {
        match self {
            ServiceControl::Start => "start",
            ServiceControl::Stop => "stop",
            ServiceControl::Restart => "restart",
        }
    }
}

fn build_menu() -> Result<(Menu, TrayMenu)> {
    let open_dashboard = MenuItem::with_id(OPEN_DASHBOARD_ID, "Open dashboard", true, None);
    let copy_dashboard_url =
        MenuItem::with_id(COPY_DASHBOARD_URL_ID, "Copy dashboard URL", true, None);
    let setup = MenuItem::with_id(SETUP_ID, "Configure startup", true, None);
    let check_updates = MenuItem::with_id(CHECK_UPDATES_ID, "Check for updates", true, None);
    let quit = MenuItem::with_id(QUIT_ID, "Quit tray", true, None);
    let message = MenuItem::with_id("tray_message", "Ready", false, None);

    let (proxy_submenu, proxy) = build_mode_submenu(service::Mode::Proxy)?;
    let (client_submenu, client) = build_mode_submenu(service::Mode::Client)?;

    let separator1 = PredefinedMenuItem::separator();
    let separator2 = PredefinedMenuItem::separator();
    let separator3 = PredefinedMenuItem::separator();
    let separator4 = PredefinedMenuItem::separator();

    let root = Menu::new();
    root.append_items(&[
        &message,
        &separator1,
        &open_dashboard,
        &copy_dashboard_url,
        &separator2,
        &proxy_submenu,
        &client_submenu,
        &separator3,
        &setup,
        &check_updates,
        &separator4,
        &quit,
    ])
    .context("failed to build tray menu")?;

    Ok((
        root,
        TrayMenu {
            message,
            proxy,
            client,
        },
    ))
}

fn build_mode_submenu(mode: service::Mode) -> Result<(Submenu, ModeMenu)> {
    let (status_id, start_id, stop_id, restart_id, logs_id) = match mode {
        service::Mode::Proxy => (
            "proxy_status",
            PROXY_START_ID,
            PROXY_STOP_ID,
            PROXY_RESTART_ID,
            PROXY_LOGS_ID,
        ),
        service::Mode::Client => (
            "client_status",
            CLIENT_START_ID,
            CLIENT_STOP_ID,
            CLIENT_RESTART_ID,
            CLIENT_LOGS_ID,
        ),
    };

    let status = MenuItem::with_id(
        status_id,
        format!("{}: unknown", mode_label(mode)),
        false,
        None,
    );
    let start = MenuItem::with_id(start_id, "Start", true, None);
    let stop = MenuItem::with_id(stop_id, "Stop", true, None);
    let restart = MenuItem::with_id(restart_id, "Restart", true, None);
    let logs = MenuItem::with_id(logs_id, "Logs", true, None);
    let separator1 = PredefinedMenuItem::separator();
    let separator2 = PredefinedMenuItem::separator();

    let submenu = Submenu::with_id(mode.service_arg(), mode_label(mode), true);
    submenu
        .append_items(&[
            &status,
            &separator1,
            &start,
            &stop,
            &restart,
            &separator2,
            &logs,
        ])
        .with_context(|| format!("failed to build {} tray menu", mode_label(mode)))?;

    Ok((
        submenu,
        ModeMenu {
            status,
            start,
            stop,
            restart,
            logs,
        },
    ))
}

fn update_mode_menu(mode: service::Mode, menu: &ModeMenu, status: ModeStatus) {
    let installed = status.installed;
    let running = status.running;
    let label = service_status_label(status);

    menu.status
        .set_text(format!("{}: {label}", mode_label(mode)));
    menu.start.set_enabled(installed && !running);
    menu.stop.set_enabled(installed && running);
    menu.restart.set_enabled(installed);
    menu.logs.set_enabled(installed);
}

fn query_mode_status(mode: service::Mode) -> ModeStatus {
    let installed = service::is_installed(mode);
    let running = installed && service::is_running(mode);

    ModeStatus { installed, running }
}

fn service_status_label(status: ModeStatus) -> &'static str {
    if !status.installed {
        "not installed"
    } else if status.running {
        "running"
    } else {
        "stopped"
    }
}

fn run_service_control(mode: service::Mode, control: ServiceControl) -> Result<String> {
    if service::is_elevated() {
        match control {
            ServiceControl::Start => service::start(mode),
            ServiceControl::Stop => service::stop(mode),
            ServiceControl::Restart => service::restart(mode),
        }
        .with_context(|| format!("failed to {} {} service", control.verb(), mode_label(mode)))?;

        return Ok(format!(
            "{} {} completed.",
            mode_label(mode),
            control.verb()
        ));
    }

    open_wakezilla_command(
        true,
        &[
            "--no-update-check",
            "service",
            control.verb(),
            "--mode",
            mode.service_arg(),
        ],
        true,
    )?;
    Ok(format!(
        "Opened elevated {} {} command.",
        mode_label(mode),
        control.verb()
    ))
}

fn check_latest_version() -> Result<String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to create update check runtime")?;

    runtime.block_on(async {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .context("failed to create update check HTTP client")?;

        match update::check_latest(&client, env!("CARGO_PKG_VERSION")).await? {
            update::UpdateStatus::Current { current } => {
                Ok(format!("Wakezilla is up to date ({current})."))
            }
            update::UpdateStatus::Available { current, latest } => Ok(format!(
                "Wakezilla {latest} is available (current {current})."
            )),
        }
    })
}

fn dashboard_url(config: &config::Config) -> String {
    format!("http://127.0.0.1:{}", config.server.proxy_port)
}

fn mode_label(mode: service::Mode) -> &'static str {
    match mode {
        service::Mode::Proxy => "Proxy",
        service::Mode::Client => "Client",
    }
}

fn load_tray_icon() -> Result<Icon> {
    let bytes = include_bytes!("../../frontend/public/images/wakezilla.png");
    let mut decoder = png::Decoder::new(Cursor::new(&bytes[..]));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().context("failed to decode tray icon")?;
    let output_size = reader
        .output_buffer_size()
        .context("tray icon output buffer is too large")?;
    let mut buffer = vec![0; output_size];
    let frame = reader
        .next_frame(&mut buffer)
        .context("failed to read tray icon frame")?;
    let bytes = &buffer[..frame.buffer_size()];
    let rgba = rgba_from_png_frame(bytes, frame.color_type)?;

    Icon::from_rgba(rgba, frame.width, frame.height).context("failed to create tray icon")
}

fn rgba_from_png_frame(bytes: &[u8], color_type: png::ColorType) -> Result<Vec<u8>> {
    match color_type {
        png::ColorType::Rgba => Ok(bytes.to_vec()),
        png::ColorType::Rgb => {
            let mut rgba = Vec::with_capacity(bytes.len() / 3 * 4);
            for chunk in bytes.chunks_exact(3) {
                rgba.extend_from_slice(chunk);
                rgba.push(255);
            }
            Ok(rgba)
        }
        png::ColorType::Grayscale => {
            let mut rgba = Vec::with_capacity(bytes.len() * 4);
            for gray in bytes {
                rgba.extend_from_slice(&[*gray, *gray, *gray, 255]);
            }
            Ok(rgba)
        }
        png::ColorType::GrayscaleAlpha => {
            let mut rgba = Vec::with_capacity(bytes.len() / 2 * 4);
            for chunk in bytes.chunks_exact(2) {
                rgba.extend_from_slice(&[chunk[0], chunk[0], chunk[0], chunk[1]]);
            }
            Ok(rgba)
        }
        png::ColorType::Indexed => Err(anyhow!("indexed tray icon was not expanded to RGBA")),
    }
}

fn open_wakezilla_command(elevated: bool, args: &[&str], keep_open: bool) -> Result<()> {
    let exe = wakezilla_cli_exe()?;
    open_command(elevated, &exe, args, keep_open)
}

fn wakezilla_cli_exe() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("failed to resolve wakezilla executable")?;
    if !is_wakezilla_tray_exe(&exe) {
        return Ok(exe);
    }

    sibling_exe(&exe, "wakezilla").with_context(|| {
        format!(
            "failed to find wakezilla CLI executable next to {}",
            exe.display()
        )
    })
}

fn wakezilla_tray_command() -> Result<(PathBuf, Vec<&'static str>)> {
    let exe = std::env::current_exe().context("failed to resolve wakezilla executable")?;
    if is_wakezilla_tray_exe(&exe) {
        return Ok((exe, Vec::new()));
    }

    if let Some(tray_exe) = sibling_exe(&exe, "wakezilla-tray") {
        return Ok((tray_exe, Vec::new()));
    }

    Ok((exe, vec!["tray"]))
}

fn is_wakezilla_tray_exe(exe: &Path) -> bool {
    exe.file_stem()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("wakezilla-tray"))
}

fn sibling_exe(exe: &Path, name: &str) -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    let file_name = format!("{name}.exe");
    #[cfg(not(target_os = "windows"))]
    let file_name = name;

    let candidate = exe.parent()?.join(file_name);
    candidate.is_file().then_some(candidate)
}

#[cfg(target_os = "linux")]
fn install_tray_autostart() -> Result<std::path::PathBuf> {
    let (exe, args) = wakezilla_tray_command()?;
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".config"))
        })
        .context("HOME or XDG_CONFIG_HOME is required to install tray autostart")?;
    let autostart_dir = config_home.join("autostart");
    std::fs::create_dir_all(&autostart_dir)
        .with_context(|| format!("failed to create {}", autostart_dir.display()))?;
    let path = autostart_dir.join("wakezilla-tray.desktop");
    let content = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Wakezilla Tray\n\
         Exec={}\n\
         Terminal=false\n\
         X-GNOME-Autostart-enabled=true\n",
        desktop_entry_command(&exe, &args),
    );
    std::fs::write(&path, content)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(path)
}

#[cfg(target_os = "macos")]
fn install_tray_autostart() -> Result<std::path::PathBuf> {
    let (exe, args) = wakezilla_tray_command()?;
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .context("HOME is required to install tray autostart")?;
    let launch_agents = home.join("Library/LaunchAgents");
    std::fs::create_dir_all(&launch_agents)
        .with_context(|| format!("failed to create {}", launch_agents.display()))?;
    let path = launch_agents.join("dev.wakezilla.tray.plist");
    let content = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n\
         \t<key>Label</key>\n\
         \t<string>dev.wakezilla.tray</string>\n\
         \t<key>ProgramArguments</key>\n\
         \t<array>\n\
         \t\t<string>{}</string>\n\
         {}\
         \t</array>\n\
         \t<key>RunAtLoad</key>\n\
         \t<true/>\n\
         </dict>\n\
         </plist>\n",
        xml_escape(&exe.to_string_lossy()),
        plist_args(&args),
    );
    std::fs::write(&path, content)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(path)
}

#[cfg(target_os = "windows")]
fn install_tray_autostart() -> Result<std::path::PathBuf> {
    let (exe, args) = wakezilla_tray_command()?;
    let command = windows_command(&exe, &args);
    let status = Command::new("reg")
        .args([
            "add",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
            "/v",
            "WakezillaTray",
            "/t",
            "REG_SZ",
            "/d",
        ])
        .arg(&command)
        .arg("/f")
        .status()
        .context("failed to invoke reg.exe")?;
    if !status.success() {
        anyhow::bail!("reg.exe failed to install tray autostart with status {status}");
    }
    Ok(exe)
}

#[cfg(target_os = "linux")]
fn desktop_entry_command(exe: &Path, args: &[&str]) -> String {
    std::iter::once(desktop_entry_quote(&exe.to_string_lossy()))
        .chain(args.iter().map(|arg| desktop_entry_quote(arg)))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(target_os = "linux")]
fn desktop_entry_quote(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('%', "%%")
    )
}

#[cfg(target_os = "macos")]
fn plist_args(args: &[&str]) -> String {
    args.iter()
        .map(|arg| format!("\t\t<string>{}</string>\n", xml_escape(arg)))
        .collect()
}

#[cfg(target_os = "macos")]
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(target_os = "windows")]
fn windows_command(exe: &Path, args: &[&str]) -> String {
    std::iter::once(format!("\"{}\"", exe.display()))
        .chain(args.iter().map(|arg| format!("\"{arg}\"")))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn open_command(elevated: bool, exe: &Path, args: &[&str], keep_open: bool) -> Result<()> {
    let mut parts = Vec::with_capacity(args.len() + 2);
    if elevated {
        parts.push("sudo".to_string());
    }
    parts.push(exe.to_string_lossy().into_owned());
    parts.extend(args.iter().map(|arg| (*arg).to_string()));

    #[cfg(target_os = "linux")]
    {
        open_linux_terminal(&parts, keep_open)
    }
    #[cfg(target_os = "macos")]
    {
        open_macos_terminal(&parts, keep_open)
    }
}

#[cfg(target_os = "linux")]
fn open_linux_terminal(parts: &[String], keep_open: bool) -> Result<()> {
    let script = shell_script(parts, keep_open);
    let candidates: [(&str, Vec<&str>); 5] = [
        ("x-terminal-emulator", vec!["-e", "sh", "-lc", &script]),
        ("gnome-terminal", vec!["--", "sh", "-lc", &script]),
        ("konsole", vec!["-e", "sh", "-lc", &script]),
        ("xfce4-terminal", vec!["-e", &script]),
        ("xterm", vec!["-e", "sh", "-lc", &script]),
    ];

    for (program, args) in candidates {
        if Command::new(program).args(args).spawn().is_ok() {
            return Ok(());
        }
    }

    Err(anyhow!(
        "no supported terminal emulator found (tried x-terminal-emulator, gnome-terminal, konsole, xfce4-terminal, xterm)"
    ))
}

#[cfg(target_os = "macos")]
fn open_macos_terminal(parts: &[String], keep_open: bool) -> Result<()> {
    let script = shell_script(parts, keep_open);
    let script = script.replace('\\', "\\\\").replace('"', "\\\"");
    let apple_script = format!("tell application \"Terminal\" to do script \"{script}\"");

    Command::new("osascript")
        .args(["-e", &apple_script])
        .spawn()
        .context("failed to open macOS Terminal")?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn open_command(elevated: bool, exe: &Path, args: &[&str], keep_open: bool) -> Result<()> {
    let ps_command = powershell_invocation(exe, args);
    let encoded_command = powershell_encoded_command(&ps_command);
    let mut powershell_args = vec!["-NoProfile", "-ExecutionPolicy", "Bypass"];
    if keep_open {
        powershell_args.push("-NoExit");
    }
    powershell_args.push("-EncodedCommand");
    powershell_args.push(&encoded_command);
    let argument_list = powershell_array_literal(&powershell_args);

    if elevated {
        let script = format!(
            "Start-Process -FilePath powershell -Verb RunAs -ArgumentList @({argument_list})"
        );
        let mut command = Command::new("powershell");
        command.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command"]);
        command.arg(&script);
        command
            .spawn()
            .context("failed to open elevated PowerShell")?;
    } else {
        let script = format!("Start-Process -FilePath powershell -ArgumentList @({argument_list})");
        let mut command = Command::new("powershell");
        command.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command"]);
        command
            .arg(&script)
            .spawn()
            .context("failed to open PowerShell")?;
    }

    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn open_command(_elevated: bool, _exe: &Path, _args: &[&str], _keep_open: bool) -> Result<()> {
    Err(anyhow!("tray commands are not supported on this OS"))
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn shell_script(parts: &[String], keep_open: bool) -> String {
    let command = parts
        .iter()
        .map(|part| shell_quote(part))
        .collect::<Vec<_>>()
        .join(" ");

    if keep_open {
        format!("{command}; echo; printf 'Press Enter to close...'; read _")
    } else {
        command
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(target_os = "windows")]
fn powershell_invocation(exe: &Path, args: &[&str]) -> String {
    let invocation = std::iter::once(exe.to_string_lossy().into_owned())
        .chain(args.iter().map(|arg| (*arg).to_string()))
        .map(|part| powershell_quote(&part))
        .collect::<Vec<_>>()
        .join(" ");
    format!("& {invocation}")
}

#[cfg(target_os = "windows")]
fn powershell_encoded_command(command: &str) -> String {
    let bytes: Vec<u8> = command
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();
    BASE64_STANDARD.encode(bytes)
}

#[cfg(target_os = "windows")]
fn powershell_array_literal(values: &[&str]) -> String {
    values
        .iter()
        .map(|value| powershell_quote(value))
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(target_os = "windows")]
fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_instance_rejects_a_second_guard() {
        const CHILD_ENV: &str = "WAKEZILLA_TEST_TRAY_INSTANCE_CHILD";

        if let Some(name) = std::env::var_os(CHILD_ENV) {
            let name = name.to_string_lossy();
            assert!(TrayInstanceGuard::acquire_named(&name)
                .expect("child acquire")
                .is_none());
            return;
        }

        let name = format!("dev.wakezilla.tray.test.{}", std::process::id());
        let first = TrayInstanceGuard::acquire_named(&name)
            .expect("first acquire")
            .expect("first instance");

        assert!(TrayInstanceGuard::acquire_named(&name)
            .expect("second acquire")
            .is_none());

        let child = std::process::Command::new(std::env::current_exe().expect("test executable"))
            .args([
                "--exact",
                "tray::desktop::tests::tray_instance_rejects_a_second_guard",
            ])
            .env(CHILD_ENV, &name)
            .status()
            .expect("run child test process");
        assert!(child.success(), "child should observe the held guard");

        drop(first);

        let reacquired = TrayInstanceGuard::acquire_named(&name)
            .expect("acquire after drop")
            .expect("instance after drop");
        drop(reacquired);

        #[cfg(target_os = "macos")]
        std::fs::remove_file(macos_lock_path(&name).expect("test lock path"))
            .expect("remove test lock file");
    }

    #[test]
    fn tray_instance_linux_socket_type_combines_cloexec_atomically() {
        assert_eq!(combine_linux_socket_type(0b0001, 0b1000), 0b1001);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn tray_instance_linux_backend_name_is_scoped_by_euid() {
        let first_user = linux_backend_name(TRAY_INSTANCE_NAME, 1000);
        let second_user = linux_backend_name(TRAY_INSTANCE_NAME, 1001);

        assert_eq!(first_user, "dev.wakezilla.tray.uid-1000");
        assert_ne!(first_user, second_user);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn tray_instance_linux_socket_does_not_survive_exec() {
        use std::io::{BufRead as _, Read as _, Write as _};
        use std::process::{Child, Stdio};

        const CHILD_ENV: &str = "WAKEZILLA_TEST_TRAY_INSTANCE_CLOEXEC_CHILD";
        const READY_MARKER: &str = "WAKEZILLA_TRAY_INSTANCE_CHILD_READY";

        if std::env::var_os(CHILD_ENV).is_some() {
            let mut stdout = std::io::stdout().lock();
            writeln!(stdout, "{READY_MARKER}").expect("write child ready marker");
            stdout.flush().expect("flush child ready marker");
            drop(stdout);

            let mut release = [0_u8; 1];
            std::io::stdin()
                .read_exact(&mut release)
                .expect("wait for parent release");
            return;
        }

        struct ChildGuard(Option<Child>);

        impl ChildGuard {
            fn child_mut(&mut self) -> &mut Child {
                self.0.as_mut().expect("child process")
            }

            fn wait(mut self) -> std::io::Result<std::process::ExitStatus> {
                let result = self.0.as_mut().expect("child process").wait();
                if result.is_ok() {
                    self.0.take();
                }
                result
            }
        }

        impl Drop for ChildGuard {
            fn drop(&mut self) {
                if let Some(mut child) = self.0.take() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        }

        let name = format!("dev.wakezilla.tray.cloexec.test.{}", std::process::id());
        let first = TrayInstanceGuard::acquire_named(&name)
            .expect("first acquire")
            .expect("first instance");
        let child = std::process::Command::new(std::env::current_exe().expect("test executable"))
            .args([
                "--exact",
                "tray::desktop::tests::tray_instance_linux_socket_does_not_survive_exec",
                "--nocapture",
            ])
            .env(CHILD_ENV, "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn child test process");
        let mut child = ChildGuard(Some(child));
        let stdout = child.child_mut().stdout.take().expect("child stdout pipe");
        let mut stdout = std::io::BufReader::new(stdout);
        let mut line = String::new();
        loop {
            line.clear();
            assert_ne!(stdout.read_line(&mut line).expect("read child output"), 0);
            if line.contains(READY_MARKER) {
                break;
            }
        }

        drop(first);
        let reacquired = TrayInstanceGuard::acquire_named(&name)
            .expect("reacquire while child lives")
            .expect("socket fd must close during exec");
        drop(reacquired);

        child
            .child_mut()
            .stdin
            .as_mut()
            .expect("child stdin pipe")
            .write_all(b"x")
            .expect("release child");
        let status = child.wait().expect("wait for child test process");
        assert!(status.success(), "child test process should exit cleanly");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn tray_instance_flock_success_is_acquired() {
        assert_eq!(
            classify_macos_flock_result(0, 0).expect("successful flock"),
            MacosFlockOutcome::Acquired
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn tray_instance_flock_would_block_is_duplicate() {
        assert_eq!(
            classify_macos_flock_result(-1, libc::EWOULDBLOCK).expect("contended flock"),
            MacosFlockOutcome::Contended
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn tray_instance_flock_other_errno_is_error() {
        let error = classify_macos_flock_result(-1, libc::EINVAL)
            .expect_err("unexpected flock errno must fail closed");

        assert_eq!(error.raw_os_error(), Some(libc::EINVAL));
    }

    #[test]
    fn dashboard_url_uses_proxy_port_from_config() {
        let mut config = config::Config::default();
        config.server.proxy_port = 4567;

        assert_eq!(dashboard_url(&config), "http://127.0.0.1:4567");
    }

    #[test]
    fn mode_labels_match_menu_text() {
        assert_eq!(mode_label(service::Mode::Proxy), "Proxy");
        assert_eq!(mode_label(service::Mode::Client), "Client");
    }

    #[test]
    fn service_status_labels_match_state() {
        assert_eq!(
            service_status_label(ModeStatus {
                installed: false,
                running: false
            }),
            "not installed"
        );
        assert_eq!(
            service_status_label(ModeStatus {
                installed: true,
                running: false
            }),
            "stopped"
        );
        assert_eq!(
            service_status_label(ModeStatus {
                installed: true,
                running: true
            }),
            "running"
        );
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn shell_quote_wraps_single_quotes() {
        assert_eq!(shell_quote("a'b"), "'a'\\''b'");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn desktop_entry_quote_escapes_quotes() {
        assert_eq!(desktop_entry_quote("/tmp/a\"b%20"), "\"/tmp/a\\\"b%%20\"");
    }
}
