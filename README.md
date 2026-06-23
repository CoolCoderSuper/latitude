# Latitude

Latitude is a small local gateway that lets agents send useful details to an end user.

An agent can publish a page, screenshot, video, static site, live app preview, project diff, or terminal session into Latitude, then hand the user a private browser URL to review it. Latitude keeps the end-user surface simple while giving agents a consistent way to share work in progress.

## What It Is For

- Sharing agent results without asking the user to inspect local files.
- Previewing generated pages, apps, images, videos, and reports.
- Giving the user a browser-based view of project status, diffs, and terminals.
- Serving local work through one authenticated public gateway.

## Running Locally

```powershell
copy latitude.example.json latitude.json
cargo run -- --config latitude.json
```

Open `http://127.0.0.1:8080/` and sign in with the configured public password. The example config uses `test`; change it before exposing Latitude outside your machine.

## Agent Setup

Agents can configure Latitude for you.

In normal use, you should not need to hand-write project, page, proxy, or static-site entries. Ask the agent to publish what it wants you to see, and it can use the Latitude CLI or local command API to create the right project and URL.

The command API is intended for local agent use. Keep it bound to localhost, and only expose the authenticated public gateway when sharing Latitude through a tunnel.

## More

- [Cloudflare Tunnel setup](docs/RUNNING_WITH_CLOUDFLARE.md)
- [Agent command API skill](skills/latitude-command-api/SKILL.md)
