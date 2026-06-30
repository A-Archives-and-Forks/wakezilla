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
fn managed_modes_include_proxy_and_client() {
    assert_eq!(service::managed_modes(), [Mode::Proxy, Mode::Client]);
}

#[test]
fn service_program_args_disable_update_checks() {
    assert_eq!(
        service::service_program_args(Mode::Proxy),
        ["--no-update-check", "proxy-server"]
    );
    assert_eq!(
        service::service_program_args(Mode::Client),
        ["--no-update-check", "client-server"]
    );
}

#[test]
fn windows_service_program_args_use_hidden_entrypoint() {
    assert_eq!(
        service::windows_service_program_args(Mode::Proxy),
        ["--no-update-check", "windows-service", "proxy"]
    );
    assert_eq!(
        service::windows_service_program_args(Mode::Client),
        ["--no-update-check", "windows-service", "client"]
    );
}

#[test]
fn firewall_rule_names_are_stable_per_mode() {
    assert_eq!(
        service::firewall_rule_name(Mode::Proxy),
        "Wakezilla Proxy Server"
    );
    assert_eq!(
        service::firewall_rule_name(Mode::Client),
        "Wakezilla Client Server"
    );
}

#[test]
fn systemd_unit_contains_exec_start_with_exe_and_subcommand() {
    let unit = service::generate_systemd_unit(Mode::Proxy, "/usr/local/bin/wakezilla");
    assert!(unit.contains("ExecStart=/usr/local/bin/wakezilla --no-update-check proxy-server"));
    assert!(unit.contains("[Service]"));
    assert!(unit.contains("WantedBy=multi-user.target"));
}

#[test]
fn launchd_plist_contains_label_exe_and_subcommand() {
    let plist = service::generate_launchd_plist(Mode::Client, "/usr/local/bin/wakezilla");
    assert!(plist.contains("dev.wakezilla.client"));
    assert!(plist.contains("/usr/local/bin/wakezilla"));
    assert!(plist.contains("<string>--no-update-check</string>"));
    assert!(plist.contains("<string>client-server</string>"));
    assert!(plist.contains("<key>RunAtLoad</key>"));
    assert!(plist.contains("<key>StandardErrorPath</key>"));
    assert!(plist.contains("<key>StandardOutPath</key>"));
    assert!(plist.contains("/Library/Logs/wakezilla/dev.wakezilla.client.err.log"));
}

#[test]
fn validate_succeeds_when_port_is_listening() {
    use std::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    // Listener stays alive for the duration of the test.
    service::validate(port, 5).expect("validate connects to open port");
}

#[test]
fn validate_fails_when_port_is_closed() {
    // Bind then drop to obtain an almost-certainly-free port.
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let result = service::validate(port, 2);
    assert!(result.is_err(), "validate should fail on closed port");
}

#[test]
fn build_config_sets_correct_port_for_mode() {
    let proxy = wakezilla::setup::build_config(Mode::Proxy, 5000);
    assert_eq!(proxy.server.proxy_port, 5000);
    assert_eq!(proxy.server.client_port, 3001); // untouched default

    let client = wakezilla::setup::build_config(Mode::Client, 6000);
    assert_eq!(client.server.client_port, 6000);
    assert_eq!(client.server.proxy_port, 3000); // untouched default
}
