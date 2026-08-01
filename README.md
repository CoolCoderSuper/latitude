# Latitude

Latitude is a small local gateway that lets agents send useful details to an end user.

An agent can publish a page, screenshot, video, static site, live app preview, project diff, or terminal session into Latitude, then hand the user a private browser URL to review it. Latitude keeps the end-user surface simple while giving agents a consistent way to share work in progress.

## What It Is For

- Sharing agent results without asking the user to inspect local files.
- Previewing generated pages, apps, images, videos, and reports.
- Giving the user a browser-based view of project status, diffs, and terminals.
- Serving local work through one authenticated public gateway.
- Creating deployment share links that can be open, password-protected, auto-expiring, or manually deleted.

## Running Locally

```powershell
copy latitude.example.json latitude.json
cargo run -- --config latitude.json
```

Open `http://127.0.0.1:8080/` and sign in with the configured public password. The example config uses `test`; change it before exposing Latitude outside your machine.

Latitude stores projects, deployments, page content, and share links in the configured data directory. The config file contains boot settings such as listener binds, public password, desktop options, and `data_dir`.

### Web UI development

Latitude checks its generated browser bundles into `src/server/assets`, so normal Rust builds do not require Node.js. When changing the file viewer or terminal viewer sources, rebuild and verify the bundles with:

```powershell
npm ci
npm run build
npm run check
npm test
```

Edit `file-viewer.js` and `terminal-viewer.js`, not their generated `*.bundle.js` or `*.bundle.css` files. The build keeps CodeMirror and xterm self-contained inside the Latitude binary.

Bundled dependency licenses are recorded in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

## Desktop Control

Latitude can expose a root-level desktop viewer at `/_desktop` when `desktop.enabled` is set to `true`.

On Windows, Latitude encodes the interactive desktop as H.264, sends video through WebRTC, and carries pointer, wheel, keyboard, and text commands over a WebRTC data channel when `view_only` is `false`. The authenticated WebSocket is used only to exchange the WebRTC offer, answer, and incremental ICE candidates.

Tune `max_fps` from 1 to 60 and `bitrate_kbps` from 250 to 25000. The defaults are 30 FPS and 4000 kbps. `max_width` and `max_height` cap the encoded stream at 1920x1080 by default. Larger or multi-monitor desktops are scaled to fit that box while retaining their aspect ratio and normalized input mapping. Lower caps reduce capture-copy, color-conversion, and encoding cost.

On supported Windows graphics adapters, the desktop producer uses DXGI Desktop Duplication update metadata, keeps frames and scaling on D3D11 surfaces, converts BGRA to BT.709 NV12 with the D3D11 video processor, and sends those surfaces to the adapter-matched Media Foundation hardware H.264 encoder. Static desktops therefore avoid frame conversion, CPU readback, and encoding work. Latitude automatically falls back to GDI capture and OpenH264 when hardware video processing or encoding is unavailable, including basic or remote display adapters and unsupported multi-adapter or rotated-display layouts. The software fallback also skips YUV conversion and encoding when the captured pixels and cursor state have not changed.

Both encoder paths use the H.264 level required by the configured stream cap, frame rate, and bitrate, and the bundled clients advertise the matching receive level during WebRTC negotiation. Debug logs identify whether the GPU or fallback producer was selected.

Direct connections use host ICE candidates. For clients across NAT or restrictive networks, add STUN or TURN entries to `ice_servers`; each entry accepts `urls`, `username`, and `credential`.

In a normal foreground run, desktop control operates in the same Windows integrity context as Latitude, so Windows can reject input directed at elevated applications or the secure desktop. Latitude can instead run as an automatic Windows service. Service mode keeps the HTTP server alive from boot and launches a short-lived LocalSystem host in the active interactive session, falling back to the console session. That host owns WebRTC capture and input, follows `Default`/`Winlogon` input-desktop changes, and falls back from DXGI to the desktop-aware GDI capture path when Windows switches to a protected desktop.

Only one client controls Windows input at a time. Additional control-enabled clients remain connected as viewers and automatically receive control when the current controller disconnects.

### Always-on Windows service

Build the release executable, set a strong `public_password` in `latitude.json`, and install from an elevated PowerShell:

```powershell
cargo build --release
.\target\release\latitude.exe --config .\latitude.json service install
.\target\release\latitude.exe service status
```

The service starts automatically at boot. Its HTTP and command listeners run even before sign-in. Two short-lived helpers are created for the active interactive session and replaced if that session changes:

- `session-host` runs as LocalSystem and owns WebRTC capture, protected-desktop switching, and input.
- `workspace-host` runs as the signed-in Windows user and owns terminals, Git commands, file browsing/search/editing, and T3 Code processes.

Both helpers listen only on random loopback ports, use independent random per-process bearer tokens, and are placed in kill-on-close Windows jobs so they cannot survive the service. Before a user signs in, the API and protected desktop remain available but workspace operations return a service-unavailable response.

Service management commands are:

```powershell
.\target\release\latitude.exe service stop
.\target\release\latitude.exe service start
.\target\release\latitude.exe service uninstall
```

Stop the service before rebuilding the installed executable because Windows holds a running executable open. Re-run `service install` after moving the executable or config; installation updates the registered paths and restarts the service unless `--no-start` is supplied.

The coordinator service and desktop host run as LocalSystem so desktop control can cross integrity levels and access protected desktops. User workspace operations do not inherit that identity: they are executed by `workspace-host` with the signed-in user's profile, environment, Git configuration, credentials, and filesystem permissions. The public API still controls a privileged service, so expose it only through a trusted network or authenticated tunnel and never use the example `test` password. Installation refuses the default password.

Service mode follows an active RDP session while it is connected and otherwise controls the physical console. It does not keep a separate disconnected RDP desktop rendered. Before sign-in it can reach the Windows sign-in desktop, but ordinary user applications do not exist until that user signs in.

## T3 Code

Enable `t3code` in the boot config to add an **Open in T3 Code** action to every project. An authenticated click connects to the configured T3 Code server, registers the repository, creates a five-minute one-time pairing credential, and opens a new T3 Code draft for that project. Latitude only starts T3 Code when `start_if_needed` is explicitly enabled.

For access from another computer or a VM, configure `gateway_bind` (for example `0.0.0.0:5598`) and set `base_url` to `auto`. Latitude then exposes the existing loopback T3 server through a separate password-protected HTTP/WebSocket listener. `auto` keeps the hostname used to reach Latitude and substitutes the gateway port, so the same config works through `localhost`, a LAN hostname, or a VM address. Expose or tunnel the gateway port alongside the main Latitude port. If TLS or port mapping changes the public gateway URL, set `base_url` to that explicit URL instead.

## Agent Setup

Agents can configure Latitude for you.

In normal use, you should not need to hand-write project, page, proxy, or static-site entries. Ask the agent to publish what it wants you to see, and it can use the Latitude CLI or local command API to create the right project and URL.

The command API is intended for local agent use. Keep it bound to localhost, and only expose the authenticated public gateway when sharing Latitude through a tunnel.

## Deployment Share Links

Share links expose one deployment without requiring the recipient to know the main project URL. They can be open, protected by a per-link password, expire automatically, and be deleted manually.

```powershell
cargo run -- share create demo preview
cargo run -- share create demo preview --password "review-only" --expires-in 2h
cargo run -- share list
cargo run -- share delete <token>
```

The generated URL uses `/__latitude/share/<token>/`. Deleting the token or reaching `expires_at` immediately disables that share path.

## More

- [Cloudflare Tunnel setup](docs/RUNNING_WITH_CLOUDFLARE.md)
- [Agent command API skill](skills/latitude-command-api/SKILL.md)
