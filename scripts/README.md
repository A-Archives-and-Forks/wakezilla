# Dashboard browser checks

`test-dashboard.cjs` runs Chromium against the compiled dashboard. It intercepts
all API requests with test data. It does not send wake, shutdown, or discovery
requests to actual machines.

Build the frontend, then start the Rust server using isolated storage:

```sh
# From frontend/
trunk build --release

# From the repository root
WAKEZILLA_USE_PREBUILT_FRONTEND=1 cargo build --release --bin wakezilla

WAKEZILLA__STORAGE__MACHINES_DB_PATH=target/review-data/machines.json \
WAKEZILLA__STORAGE__ACCESS_HISTORY_PATH=target/review-data/access_history.json \
target/release/wakezilla --no-update-check proxy-server --port 5006
```

Use a LAN HTTP URL to test the clipboard fallback used outside secure contexts.
Replace the example address with the host's IP address. The server binds to
`0.0.0.0`, and the browser container uses host networking on Linux.

```sh
mkdir -p target/dashboard-check
docker run --rm --network host --ipc=host --entrypoint node \
  -e NODE_PATH=/app/node_modules \
  -e DASHBOARD_URL=http://192.168.1.17:5006 \
  -v "$PWD/scripts/test-dashboard.cjs:/test.cjs:ro" \
  -v "$PWD/target/dashboard-check:/out" \
  mcr.microsoft.com/playwright/mcp:latest /test.cjs
```

The checks cover 1440, 390, and 320 pixel viewports; both themes; modal and input
surfaces; layout overflow; search and list view; API errors; create/edit/delete;
configuration commands and copying over HTTP; deferred setup; retained form drafts;
services; access history; key rotation; power status confirmation; discovery;
deep links; Escape; and theme persistence. Screenshots are written to the output
directory. No npm packages or browser dependencies are installed on the host.
