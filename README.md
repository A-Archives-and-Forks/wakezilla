# Wakezilla 🦖

[![Crates.io](https://img.shields.io/crates/v/wakezilla)](https://crates.io/crates/wakezilla)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![CI](https://github.com/guibeira/wakezilla/actions/workflows/ci.yml/badge.svg)](https://github.com/guibeira/wakezilla/actions/workflows/ci.yml)

<p align="center">
  <img width="200" height="159" src="https://github.com/user-attachments/assets/e88f084b-47b8-467b-a5c6-d64327805792" alt="Wakezilla logo">
</p>

Wakezilla wakes machines on demand, forwards traffic to them, and can shut them
down again after a period of inactivity. Manage everything from a web dashboard,
terminal UI, or desktop tray.

[Documentation](https://wakezilla.dev/docs/) ·
[Installation](https://wakezilla.dev/docs/getting-started/installation/) ·
[Quick start](https://wakezilla.dev/docs/getting-started/quick-start/) ·
[Releases](https://github.com/guibeira/wakezilla/releases)

## See it in action

![Wakezilla lifecycle: request, wake, proxy, response, and idle shutdown](docs/assets/wakezilla-lifecycle.gif)

## Features

- Wake machines remotely with Wake-on-LAN.
- Forward TCP services and wake their target machine automatically.
- Manage machines, ports, activity, and network discovery from the web dashboard.
- Use a responsive dashboard with light and dark themes, machine types, and inline editing.
- Configure the client from generated commands, verify the connection, or resume setup later.
- Shut down idle machines through authenticated proxy-to-client requests.
- Run the proxy and client as native system services.
- Operate Wakezilla through the web, terminal UI, or desktop tray.

## How it works

Wakezilla's proxy runs on an always-on machine. When traffic reaches a configured
port, it wakes the target machine and forwards the connection. An optional client
service on the target machine enables authenticated remote and automatic shutdown.

Set a machine's inactivity period to `0` to disable automatic shutdown while keeping
client authentication and manual shutdown. The dashboard activity list covers the
current browser session. Per-service access history is available in each machine's dialog.

See [How Wakezilla works](https://wakezilla.dev/docs/getting-started/how-it-works/)
for the architecture, network flow, and deployment model.

## Get started

Use the documentation for current platform requirements, installers, Docker,
service setup, and the first machine configuration:

1. [Install Wakezilla](https://wakezilla.dev/docs/getting-started/installation/).
2. Follow the [quick start](https://wakezilla.dev/docs/getting-started/quick-start/).
3. Open the [web dashboard guide](https://wakezilla.dev/docs/guides/web-dashboard/).

## Docs

[Read the Wakezilla documentation](https://wakezilla.dev/docs/).

## Development

Wakezilla is a Rust workspace with a Leptos frontend. Run Rust tooling directly:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked -- --skip test_scan_network_basic
```

For local development, start the frontend from `frontend/` with `trunk serve`
and run the proxy from the repository root with `cargo run --bin wakezilla -- proxy-server`.
Trunk listens on `0.0.0.0:8080` and forwards `/api/` to the proxy on port `3000`.
The frontend uses the browser's origin, so it also works from another machine.

To serve the frontend inside the Rust binary, first run `trunk build --release`
from `frontend/`, then build the server with `cargo build --release`. Debug requests
for `/` redirect to Trunk on port `8080` using the request host, so development also
works from another machine. Use `WAKEZILLA_USE_PREBUILT_FRONTEND=1` for backend
checks that must reuse the compiled assets.

The skipped scanner test sends real requests on the local network. Run it separately
only when a live scan is intended. Desktop tray checks with `--all-features` require
the platform's GTK dependencies on Linux.

See [dashboard browser checks](scripts/README.md) for container-based desktop and
mobile verification with simulated API responses.

Issues and pull requests are welcome. Please include tests for behavior changes
and keep the standard checks green.

## Security

Wakezilla is designed primarily for trusted networks. Client shutdown requests
can be authenticated, but access to the dashboard and API must be restricted
separately. Review the [security guide](https://wakezilla.dev/docs/reference/security/)
before exposing Wakezilla outside a private network.

## License

Wakezilla is available under the [MIT License](LICENSE).
