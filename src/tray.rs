#[cfg(all(
    feature = "desktop-tray",
    any(target_os = "macos", target_os = "windows", target_os = "linux")
))]
mod desktop;

#[cfg(all(
    feature = "desktop-tray",
    any(target_os = "macos", target_os = "windows", target_os = "linux")
))]
pub use desktop::run;

#[cfg(not(all(
    feature = "desktop-tray",
    any(target_os = "macos", target_os = "windows", target_os = "linux")
)))]
pub fn run() -> anyhow::Result<()> {
    use anyhow::{bail, Context};
    use std::process::Command;

    let exe = std::env::current_exe().context("failed to resolve wakezilla executable")?;
    if let Some(tray_exe) = sibling_exe(&exe, "wakezilla-tray") {
        let status = Command::new(&tray_exe)
            .status()
            .with_context(|| format!("failed to start {}", tray_exe.display()))?;

        if status.success() {
            return Ok(());
        }

        bail!("{} exited with {status}", tray_exe.display());
    }

    anyhow::bail!(
        "desktop tray support is not enabled in this build, and no `wakezilla-tray` helper was found next to {}. Rebuild Wakezilla with `--features desktop-tray` on Linux, macOS, or Windows, or install the matching `wakezilla-tray` binary.",
        exe.display()
    )
}

#[cfg(not(all(
    feature = "desktop-tray",
    any(target_os = "macos", target_os = "windows", target_os = "linux")
)))]
fn sibling_exe(exe: &std::path::Path, name: &str) -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    let file_name = format!("{name}.exe");
    #[cfg(not(target_os = "windows"))]
    let file_name = name;

    let candidate = exe.parent()?.join(file_name);
    candidate.is_file().then_some(candidate)
}

#[cfg(all(
    test,
    not(all(
        feature = "desktop-tray",
        any(target_os = "macos", target_os = "windows", target_os = "linux")
    ))
))]
mod tests {
    use super::sibling_exe;

    #[test]
    fn fallback_finds_sibling_tray_helper() {
        let temp = tempfile::tempdir().expect("tempdir");
        let exe = temp.path().join("wakezilla");
        let tray_name = if cfg!(target_os = "windows") {
            "wakezilla-tray.exe"
        } else {
            "wakezilla-tray"
        };
        let tray = temp.path().join(tray_name);
        std::fs::write(&tray, b"").expect("write tray helper");

        assert_eq!(sibling_exe(&exe, "wakezilla-tray"), Some(tray));
    }

    #[test]
    fn fallback_ignores_missing_sibling_tray_helper() {
        let temp = tempfile::tempdir().expect("tempdir");
        let exe = temp.path().join("wakezilla");

        assert_eq!(sibling_exe(&exe, "wakezilla-tray"), None);
    }
}
