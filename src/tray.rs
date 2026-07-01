#[cfg(all(
    feature = "desktop-tray",
    any(
        target_os = "macos",
        target_os = "windows",
        all(target_os = "linux", not(target_env = "musl"))
    )
))]
mod desktop;

#[cfg(all(
    feature = "desktop-tray",
    any(
        target_os = "macos",
        target_os = "windows",
        all(target_os = "linux", not(target_env = "musl"))
    )
))]
pub use desktop::run;

#[cfg(not(all(
    feature = "desktop-tray",
    any(
        target_os = "macos",
        target_os = "windows",
        all(target_os = "linux", not(target_env = "musl"))
    )
)))]
pub fn run() -> anyhow::Result<()> {
    anyhow::bail!(
        "desktop tray support is not enabled in this build. Rebuild Wakezilla with `--features desktop-tray` on Linux GNU, macOS, or Windows."
    )
}
