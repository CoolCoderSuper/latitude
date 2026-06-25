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

## Desktop VNC

Latitude can expose a root-level desktop viewer at `/_desktop` when `desktop.enabled` is set to `true`.

Use `desktop.mode: "external"` to bridge to an already-running VNC server. The default external target is `127.0.0.1:5900`, view-only in the noVNC client, and non-loopback VNC hosts are rejected unless `allow_non_loopback` is explicitly enabled.

Use `desktop.mode: "managed"` on Windows to let Latitude start a vendored UltraVNC helper on demand. Run `.\init-ultravnc.ps1` from the repository root to download UltraVNC into `tools/ultravnc/`, or point `desktop.managed_executable` at another helper. Latitude writes an `ultravnc.ini` beside the executable, starts `winvnc.exe -multi -run`, binds it to a random loopback-only port, disables UltraVNC password auth, and relies on Latitude's public auth plus the local-only listener.

UltraVNC is GPL software. Keep its license and source-offer materials with any redistributed helper bundle; Latitude's MIT source stays separate from the helper process.

## Agent Setup

Agents can configure Latitude for you.

In normal use, you should not need to hand-write project, page, proxy, or static-site entries. Ask the agent to publish what it wants you to see, and it can use the Latitude CLI or local command API to create the right project and URL.

The command API is intended for local agent use. Keep it bound to localhost, and only expose the authenticated public gateway when sharing Latitude through a tunnel.

## Deployment Share Links

Share links expose one deployment without requiring the recipient to know the main project URL. They can be open, protected by a per-link password, expire automatically, and be deleted manually.

```powershell
cargo run -- share create demo mock
cargo run -- share create demo mock --password "review-only" --expires-in 2h
cargo run -- share list
cargo run -- share delete <token>
```

The generated URL uses `/__latitude/share/<token>/`. Deleting the token or reaching `expires_at` immediately disables that share path.

## More

- [Cloudflare Tunnel setup](docs/RUNNING_WITH_CLOUDFLARE.md)
- [Agent command API skill](skills/latitude-command-api/SKILL.md)
