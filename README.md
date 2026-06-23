# Latitude

Latitude is a path-based proxy and static-site gateway.

- Public traffic enters on one port, such as `0.0.0.0:8080`.
- Projects are mounted by name, and deployments live under them, such as `/demo/website`.
- A deployment can forward to a live HTTP server, serve files from a static directory, or publish a stored document.
- Agents should publish through the Latitude CLI, which sends commands to the local command API.
- A separate command API manages projects, deployments, and config.
- Public pages require the configured `public_password`; the starter config uses `test`.

## Quick Start

```powershell
copy latitude.example.json latitude.json
cargo run -- --config latitude.json
```

Then visit:

- `http://127.0.0.1:8080/` for the server project index.
- `http://127.0.0.1:8080/demo` for the demo project index.
- `http://127.0.0.1:8080/demo/_diff` for the demo project's Git diff viewer.
- `http://127.0.0.1:8080/demo/_terminal` for the demo project's remote terminal.
- `http://127.0.0.1:8080/demo/website` for the reverse proxy example.
- `http://127.0.0.1:8080/demo/mock` for the static-site example.
- `http://127.0.0.1:7600/health` for command API health.

Sign in to public pages with the password `test`. Change it through the local command API by updating `public_password` in `/api/config`.

The terminal tool opens authenticated PTY-backed shells inside the project's `project_dir`, using the same public session as the website and mobile app. Sessions stay alive on the Latitude server when clients disconnect, so the website and mobile app can create multiple terminals, switch between them, and reconnect to existing shells. The web UI and mobile app each render their own terminal surface while connecting to the same PTY WebSocket. Change the default public password before exposing Latitude through a tunnel.

## Agent CLI

The preferred agent interface is the Latitude CLI. It talks to the command API on `http://127.0.0.1:7600` by default.

```powershell
cargo run -- health
cargo run -- project ensure demo --project-dir .
cargo run -- publish page demo report --file report.md --title "Agent Report" --format markdown
cargo run -- publish page demo snapshot --file screenshot.png --title "Latest Screenshot"
cargo run -- deploy static demo mock --root ./sites/mock --spa-fallback
cargo run -- deploy proxy demo frontend --upstream http://127.0.0.1:5173
```

Use `--command-url` or `LATITUDE_COMMAND_URL` if the command API is listening somewhere else.

The CLI prints JSON responses. Publish and deploy commands include the local public URL, such as `http://127.0.0.1:8080/demo/report`.

If the CLI is unavailable, agents can still call the command API directly. Publish a single Markdown page:

```powershell
Invoke-RestMethod `
  -Method Put `
  -Uri http://127.0.0.1:7600/api/projects/demo/pages/report `
  -ContentType "text/markdown" `
  -Body "# Agent Report`n`nThe latest work is ready to review."
```

Then open `http://127.0.0.1:8080/demo/report`.

Markdown and HTML documents are rendered as pages. Image and video files up to 25 MiB can also be published as documents; the CLI infers `image/*` and `video/*` content types from the file extension and serves the original bytes:

```powershell
cargo run -- publish page demo walkthrough --file walkthrough.mp4
```

Direct API callers can do the same by sending a raw body with an `image/*` or `video/*` `Content-Type`.

See [skills/latitude-command-api/SKILL.md](skills/latitude-command-api/SKILL.md) for the agent-facing command API skill and [docs/RUNNING_WITH_CLOUDFLARE.md](docs/RUNNING_WITH_CLOUDFLARE.md) for Cloudflare Tunnel setup.
