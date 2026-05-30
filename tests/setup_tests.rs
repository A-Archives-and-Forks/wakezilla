use wakezilla::service::{self, Mode};

#[test]
fn mode_parses_from_str() {
    assert_eq!(Mode::from_str_opt("proxy"), Some(Mode::Proxy));
    assert_eq!(Mode::from_str_opt("client"), Some(Mode::Client));
    assert_eq!(Mode::from_str_opt("nonsense"), None);
}

#[test]
fn mode_exposes_subcommand_and_service_name() {
    assert_eq!(Mode::Proxy.subcommand(), "proxy-server");
    assert_eq!(Mode::Client.subcommand(), "client-server");
    assert_eq!(Mode::Proxy.service_name(), "wakezilla-proxy");
    assert_eq!(Mode::Client.service_name(), "wakezilla-client");
}

#[test]
fn systemd_unit_contains_exec_start_with_exe_and_subcommand() {
    let unit = service::generate_systemd_unit(Mode::Proxy, "/usr/local/bin/wakezilla");
    assert!(unit.contains("ExecStart=/usr/local/bin/wakezilla proxy-server"));
    assert!(unit.contains("[Service]"));
    assert!(unit.contains("WantedBy=multi-user.target"));
}

#[test]
fn launchd_plist_contains_label_exe_and_subcommand() {
    let plist = service::generate_launchd_plist(Mode::Client, "/usr/local/bin/wakezilla");
    assert!(plist.contains("dev.wakezilla.client"));
    assert!(plist.contains("/usr/local/bin/wakezilla"));
    assert!(plist.contains("client-server"));
    assert!(plist.contains("<key>RunAtLoad</key>"));
}
