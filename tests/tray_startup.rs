#![cfg(all(target_os = "linux", feature = "desktop-tray"))]

use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn tray_starts_without_a_gtk_initialization_panic() {
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        eprintln!("skipping tray startup test because no graphical display is available");
        return;
    }

    let mut child = Command::new(env!("CARGO_BIN_EXE_wakezilla-tray"))
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start wakezilla-tray");
    let deadline = Instant::now() + Duration::from_secs(2);

    loop {
        if let Some(status) = child.try_wait().expect("poll wakezilla-tray") {
            let mut stderr = String::new();
            child
                .stderr
                .take()
                .expect("capture wakezilla-tray stderr")
                .read_to_string(&mut stderr)
                .expect("read wakezilla-tray stderr");

            assert!(
                !stderr.contains("GTK has not been initialized"),
                "wakezilla-tray panicked before initializing GTK:\n{stderr}"
            );
            panic!("wakezilla-tray exited unexpectedly with {status}:\n{stderr}");
        }

        if Instant::now() >= deadline {
            child.kill().expect("stop wakezilla-tray after startup");
            child.wait().expect("reap wakezilla-tray");
            return;
        }

        thread::sleep(Duration::from_millis(25));
    }
}
