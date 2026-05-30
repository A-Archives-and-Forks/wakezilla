//! OS-native system service installation for the `setup` subcommand.

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
    pub fn service_name(self) -> &'static str {
        match self {
            Mode::Proxy => "wakezilla-proxy",
            Mode::Client => "wakezilla-client",
        }
    }

    /// launchd label (reverse-DNS).
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

/// Render a launchd LaunchDaemon plist.
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
         </dict>\n\
         </plist>\n",
        label = mode.launchd_label(),
        exe = exe,
        sub = mode.subcommand(),
    )
}
