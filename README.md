# Wakezilla 🦖
![Crates.io Version](https://img.shields.io/crates/v/wakezilla) [![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT) [![CI](https://github.com/guibeira/wakezilla/actions/workflows/ci.yml/badge.svg)](https://github.com/guibeira/wakezilla/actions/workflows/ci.yml)
<img width="200" height="159" src="https://github.com/user-attachments/assets/e88f084b-47b8-467b-a5c6-d64327805792" align="left" alt="wakezilla"/>

⚡ Wake-on-LAN made simple → power on your machines remotely whenever needed.

🌐 Reverse proxy → intercepts traffic and wakes the server automatically if it’s offline.

🔌 Automatic shutdown → saves energy by powering down idle machines after configurable thresholds.



## Web interface
<img width="531" height="727" alt="image" src="https://github.com/user-attachments/assets/e9e744c4-35ec-4ca0-8de2-696e447cce7a" />

## Features

- **Wake-on-LAN**: Send magic packets to wake sleeping machines
- **TCP Proxy**: Forward ports to remote machines with automatic WOL
- **Web Interface**: Manage machines, ports, and monitor activity through a web dashboard
- **Automatic Shutdown**: Automatically turn off machines after inactivity periods
- **Network Scanner**: Discover machines on your local network

## Installation

### Install on Windows

Run in PowerShell:

```powershell
irm https://wakezilla.dev/install.ps1 | iex
```

To pin a version:

```powershell
iex "& { $(irm https://wakezilla.dev/install.ps1) } -Version 0.2.4"
```

By default this installs `wakezilla.exe` to
`%LOCALAPPDATA%\Programs\wakezilla\bin` and adds that directory to your user
PATH. Open a new terminal after installation. Override the destination with
`-InstallDir`:

```powershell
iex "& { $(irm https://wakezilla.dev/install.ps1) } -InstallDir $env:USERPROFILE\bin"
```

The Windows installer downloads prebuilt binaries from GitHub Releases and
validates them against `SHA256SUMS`.

### Install on Linux/macOS with script

```bash
curl -fsSL https://wakezilla.dev/install.sh | sh
```

To pin a version:

```bash
curl -fsSL https://wakezilla.dev/install.sh | sh -s -- 0.1.49
```

By default this installs `wakezilla` to `$HOME/.local/bin`. Override the destination with `BIN_DIR`:

```bash
curl -fsSL https://wakezilla.dev/install.sh | BIN_DIR=/usr/local/bin sh -s -- 0.1.49
```

The script installs prebuilt binaries from GitHub Releases and requires `curl`, `jq`, `tar`, and either `sha256sum` or `shasum`.

### Install from cargo (recommended)

```bash
cargo install wakezilla
```

### Install via Homebrew

```bash
brew tap guibeira/wakezilla https://github.com/guibeira/wakezilla
brew install wakezilla
```

### Using pre-built docker image

1. **Run the proxy server**:
```bash
docker run -d \
 --name wakezilla-proxy \
 --network host \
 -e WAKEZILLA__SERVER__PROXY_PORT=3000 \
 -v ${PWD}/wakezilla-data:/opt/wakezilla \
 guibeira/wakezilla:latest proxy-server
```
Note:
- `--network host` is required for Wake-on-LAN to work properly.
- add `-v ${PWD}/wakezilla-data:/opt/wakezilla` to save configuration data persistently.

2. **Run the client server**:
```bash
docker run -d \
 --name wakezilla-client \
 -p 3001:3001 \
 guibeira/wakezilla:latest client-server
```

### Install from source

1. **Install Rust**:
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   source $HOME/.cargo/env
   ```

2. **Build and Install**:
   ```bash
   git clone git@github.com:guibeira/wakezilla.git
   cd wakezilla
   make install
   ```

3. **Verify Installation**:
   ```bash
   wakezilla --version
   ```

### Update Wakezilla

Install the latest GitHub Release for your platform:

```bash
wakezilla update
```

To install a specific version, pass it without the leading `v`:

```bash
wakezilla update --version 0.2.3
```

Wakezilla checks for newer releases when the binary starts and prints a warning
when one is available. Use `--no-update-check` for offline or scripted runs:

```bash
wakezilla --no-update-check proxy-server
```

If Wakezilla was installed into a system directory such as `/usr/local/bin`, the
update may require elevated privileges. A failed update leaves the existing
binary untouched.

### Run proxy server 

1. **Run the Server**:
   ```bash
    wakezilla proxy-server
   ```
   
   By default, the web interface runs on port 3000.

### Run Client 

1. **Run the Server**:
   ```bash
    wakezilla client-server
   ```
   
   By default, the web interface runs on port 3001.
   You can check the health of the client server by visiting:
   http://<client-ip>:3001/health

### Run the terminal UI (TUI)

Manage your machines from the terminal — no browser required. Point the TUI at
a running proxy server:

```bash
 wakezilla tui --api-url http://192.168.1.200:3000
```

`--api-url` defaults to `http://127.0.0.1:3000`, so it can be omitted when the
proxy server runs on the same host:

```bash
 wakezilla tui
```

### Set up auto-start (system service)

1. **Run the interactive setup wizard** (requires `sudo`/admin privileges):
   Linux/macOS:

   ```bash
   sudo wakezilla setup
   ```

   Windows PowerShell (run as Administrator):

   ```powershell
   wakezilla setup
   ```

   This interactively configures the host to auto-start the proxy or client
   server as a system service (systemd on Linux, launchd on macOS, or the
   Windows Service Manager). It writes an OS-standard config file, installs and
   enables the service, then validates that the service is reachable after
   install. Pass `--mode <proxy|client>` and `--port <PORT>` to skip the prompts.

   If a configuration or service already exists, `setup` shows a summary of the
   current config (and installed services) and asks for confirmation before
   overwriting. Existing settings for the *other* server are preserved — only
   the target server's port is updated. Pass `-y`/`--yes` to skip the
   confirmation for non-interactive use.

   Services installed by `setup` run with `--no-update-check`, so background
   services do not make startup network requests. Run `wakezilla update`
   manually when you want to upgrade the installed binary.

2. **Control an installed service** (requires `sudo`/admin privileges):
   Linux/macOS:

   ```bash
   sudo wakezilla service start
   sudo wakezilla service stop
   sudo wakezilla service restart
   sudo wakezilla service status            # is it running?
   sudo wakezilla service logs              # status + recent logs
   sudo wakezilla service logs -f -n 100    # follow, last 100 lines
   ```

   Windows PowerShell (run as Administrator):

   ```powershell
   wakezilla service start
   wakezilla service stop
   wakezilla service restart
   wakezilla service status
   wakezilla service logs
   ```

   Controls a service previously installed with `setup`. If both the proxy and
   client are installed, an interactive picker asks which to act on; pass
   `--mode <proxy|client>` to skip the prompt. If only one is installed, it is
   selected automatically.

   `logs` reads from journald on Linux and from the daemon's redirected log file
   on macOS (`/Library/Logs/wakezilla/`). Log streaming is not captured for the
   Windows service; use Windows Event Viewer for service logs.


## Usage

### Web Interface
Access the web interface at `http://<server-ip>:3000` to:
- Add and manage machines
- Configure port forwards
- View network scan results
- Send WOL packets manually
- Configure automatic shutdown settings

### Terminal UI (TUI)

Prefer the terminal? Run `wakezilla tui --api-url http://<server-ip>:3000` to
browse your machines, watch their online/offline status, and act on them without
leaving the shell.

![Wakezilla TUI demo](https://vhs.charm.sh/vhs-2RN0TE9RlTeQaDowRdIAZ8.gif)

The left pane lists every registered machine with a live **ON**/**OFF** status;
the right pane shows the details and port forwards of the selected machine.

Key bindings:

| Key     | Action                                  |
|---------|-----------------------------------------|
| `j` / `↓` | Move selection down                   |
| `k` / `↑` | Move selection up                     |
| `r`     | Refresh the machine list and statuses   |
| `w`     | Send a Wake-on-LAN packet to the machine |
| `t`     | Turn off the machine                    |
| `d`     | Delete the machine (asks to confirm)    |
| `q` / `Esc` | Quit                                |

### Adding Machines
1. Navigate to the web interface
2. Click "Add Machine" or use the network scanner
3. Fill in MAC address, IP, and name
4. Configure:
   - Turn-off port (if remote shutdown is needed)
   - Inactivity Period: Time in minutes before automatic shutdown (default: 30 minutes)
   - Port forwards as needed

### Configuring Automatic Shutdown
1. When adding or editing a machine, enable "Can be turned off remotely"
2. Set the "Turn Off Port" (typically 3001 for the client server)
3. Configure the Inactivity Period:
   - Set the number of minutes of inactivity before automatic shutdown
   - The system monitors when the last request was received for each machine
   - If no requests are received within the inactivity period, the machine will be automatically shut down
4. The machine will automatically shut down after the configured inactivity period of no activity

### Port Forwarding
1. Add a machine to the system
2. Configure port forwards for that machine:
   - Local Port: Port on the server to listen on
   - Target Port: Port on the remote machine to forward to
3. When traffic hits the local port, the machine will be woken up if needed and traffic forwarded


### Machine Configuration
Each machine can be configured with:
- MAC Address
- IP Address
- Name and Description
- Turn-off Port (for remote shutdown)
- Inactivity Period: Time in minutes before automatic shutdown (default: 30 minutes)
- Port Forwards:
  - Local Port: Port on the server
  - Target Port: Port on the remote machine

## How It Works

1. **Server Mode**: Runs the web interface and proxy services
2. **Client Mode**: Runs on target machines to enable remote shutdown
3. **WOL Process**: 
   - When traffic hits a configured port, the server sends a WOL packet
   - Waits for the machine to become reachable
   - Forwards traffic once the machine is up
4. **Automatic Shutdown**: 
   - A **single global inactivity monitor** runs continuously, checking all machines every second
   - Each machine's `last_request` timestamp is automatically updated whenever a connection is accepted
   - The monitor compares the time since `last_request` against the configured `inactivity_period` (in minutes)
   - If no requests are received within the inactivity period, a shutdown signal is sent via HTTP to the client
   - When a machine configuration is updated (e.g., inactivity period changed), the monitor is automatically stopped and restarted with the new settings
   - This ensures only one monitor instance runs at a time, preventing duplicate shutdown signals

## Security Considerations

- The server should be run on a trusted network
- Access to the web interface should be restricted if exposed to the internet
- The turn-off endpoint on clients should only be accessible from the server

## Development
### Prerequisites
- Rust and Cargo installed
- Clone the repository
- Install dependencies with `make dependencies`

on frontend folder run:
```bash
trunk serve
```
this will initialize the frontend in watch mode on port 8080

on the root folder run:
```bash
cargo watch -x 'run -- proxy-server'
```
this will initialize the backend in watch mode on port 3000


## Troubleshooting

### Common Issues

1. **Machine not waking up**:
   - Verify the MAC address is correct
   - Ensure WOL is enabled in the machine's BIOS/UEFI
   - Check firewall settings on the target machine
   - Verify the target machine supports WOL

2. **Proxy not working**:
   - Check that the target port is correct
   - Verify the machine is reachable after WOL
   - Ensure no firewall is blocking the connection

3. **Network scanner not finding devices**:
   - Windows release builds do not currently include ARP network scanning because
     the upstream `pnet` Windows backend requires the external Npcap/WinPcap
     `Packet.lib` SDK at link time
   - On Linux/macOS, run Wakezilla with `sudo` if raw socket permissions are denied
   - Verify the selected network interface is the LAN interface you expect

4. **Automatic shutdown not working**:
   - Verify the turn-off port is configured correctly
   - Ensure the client is running on the target machine
   - Check that the client can receive HTTP requests from the server
   - Verify the inactivity period is configured correctly (in minutes)
   - Check logs to see when the last request was received for the machine
   - Ensure traffic is actually reaching the proxy (requests update the last_request timestamp)

### Logs
Check the terminal output for detailed logs about:
- WOL packets sent
- Connection attempts
- Proxy activity
- Shutdown requests
- Errors and warnings

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Run `cargo fmt` and `cargo clippy`
5. Commit your changes
6. Push to the branch
7. Create a pull request

## License

This project is licensed under the MIT License - see the LICENSE file for details.
