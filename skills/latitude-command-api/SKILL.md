---
name: latitude-command-api
description: "Operate Latitude through its CLI first, with direct local command API requests as the fallback. Use when an agent needs to publish an HTML, Markdown, image, or video document, create or update Latitude projects, configure reverse proxy or static deployments, create or delete deployment share links, inspect or replace Latitude config, or verify local Latitude health."
---

# Latitude CLI And Command API

## Purpose

Use the Latitude CLI as the preferred agent interface. The CLI sends commands to Latitude's local command API, keeps agent workflows consistent, and returns JSON that agents can inspect.

The command API is unauthenticated and must remain loopback-only. Use direct HTTP requests only when the CLI is unavailable.

Default command base URL: `http://127.0.0.1:7600`.
Default public preview URL: `http://127.0.0.1:8080`.
If `/health` reports `public_bind` as `0.0.0.0:8080`, use `127.0.0.1:8080` for local preview.
Public preview pages require the configured `public_password`. The starter password is `test`.
Deployment share links mount at `/__latitude/share/{token}/` and can use no password or a per-link password.

## Preferred CLI Workflow

1. Confirm Latitude is reachable:

```bash
latitude health
```

When running from source, use `cargo run --` before the CLI arguments:

```bash
cargo run -- health
```

2. Ensure the target project exists before publishing into it:

```bash
latitude project ensure demo --project-dir .
```

3. Publish or deploy with idempotent CLI commands:

```bash
latitude publish page demo report --file report.md --title "Agent Report" --format markdown
latitude publish page demo snapshot --file screenshot.png --title "Latest Screenshot"
latitude deploy static demo mock --root ./sites/mock --spa-fallback
latitude deploy proxy demo frontend --upstream http://127.0.0.1:5173
```

4. Verify the returned `public_url`. Deployments mount at `/{project}/{deployment}` on the public listener.

The CLI prints JSON responses. Publish and deploy commands include `public_url` plus the deployment object returned by Latitude.

5. Create a share link when the user needs a deployment-specific URL:

```bash
latitude share create demo mock
latitude share create demo mock --password "review-only" --expires-in 2h
latitude share list
latitude share delete <token>
```

Share create responses include `share_url` plus a redacted share object. The share URL can be sent directly; it does not require the global public password unless the deployment itself redirects elsewhere.

## CLI Reference

### Command API Target

The CLI targets `http://127.0.0.1:7600` by default. Override it with:

```bash
latitude --command-url http://127.0.0.1:7601 health
```

or:

```bash
LATITUDE_COMMAND_URL=http://127.0.0.1:7601 latitude health
```

If you only have a bind address, `--command-bind 127.0.0.1:7601` is also accepted and converted to `http://127.0.0.1:7601` for CLI commands.

### Health

```bash
latitude health
```

### Config

```bash
latitude config get
latitude config put latitude.json
```

Config contains boot settings such as listener binds, `public_password`, `desktop`, and optional `data_dir`. Projects, deployments, page content, and share links are managed through the project, deployment, page, and share commands.

`config put` replaces the active boot config. Preserve unrelated boot fields, especially `public_password`.

### Projects

```bash
latitude project list
latitude project get demo
latitude project ensure demo --project-dir .
```

`project ensure` creates the project only if it is missing, so it does not erase existing deployments.

### Publish A Document

Use this for status reports, handoff notes, generated documentation, quick agent output, screenshots, and short video artifacts.

```bash
latitude publish page demo report --file report.md --title "Agent Report" --format markdown
latitude publish page demo snapshot --file screenshot.png --title "Latest Screenshot"
latitude publish page demo walkthrough --file walkthrough.mp4
```

If the project may not exist yet:

```bash
latitude publish page demo report --file report.md --project-dir . --title "Agent Report" --format markdown
```

Document payload rules:

- `--format markdown` publishes Markdown.
- `--format html` publishes HTML.
- `--format auto` infers Markdown/HTML from the file extension and content, and infers image/video documents from `image/*` or `video/*` file extensions.
- Omit `--file` or pass `--file -` to read content from stdin.
- Markdown and HTML content must be UTF-8 and at most 2 MiB.
- Image and video documents are served as the original bytes and must be at most 25 MiB.
- Titles are trimmed and must be at most 160 characters.
- Full HTML documents are served as-is. HTML fragments and Markdown are wrapped in Latitude's page shell.

### Deploy A Static Site

Use this for built assets or simple file serving.

```bash
latitude deploy static demo mock --root ./sites/mock --index-file index.html --spa-fallback
```

If the project may not exist yet:

```bash
latitude deploy static demo mock --root ./sites/mock --project-dir . --spa-fallback
```

`index_file` must be a single file name. Use `--spa-fallback` for single-page apps so missing paths return the index file.

### Deploy A Reverse Proxy

Use this when a web app dev server or service is already running.

```bash
latitude deploy proxy demo frontend --upstream http://127.0.0.1:5173
```

By default, Latitude strips the `/{project}/{deployment}` prefix before forwarding. Pass `--no-strip-prefix` when the upstream expects the full public path.

### Inspect Or Delete Deployments

```bash
latitude deployment list demo
latitude deployment get demo frontend
latitude deployment delete demo frontend
```

### Share A Deployment

```bash
latitude share create demo frontend
latitude share create demo frontend --password "review-only"
latitude share create demo frontend --expires-in 30m
latitude share create demo frontend --expires-at 4102444800
latitude share list
latitude share get <token>
latitude share delete <token>
```

Omit `--password` for an open share link. Use `--expires-in` with seconds by default or `m`, `h`, and `d` suffixes for minutes, hours, and days. `--expires-at` accepts a Unix timestamp in seconds. `share list` and `share get` redact the password and show `has_password`, `expires_at`, and `expired`.

## Safety Rules

- Never bind or expose the command API outside loopback. `command_bind` must use `127.0.0.1` or `[::1]`.
- Treat config replacement as a boot-settings replacement. Fetch the current boot config first and preserve unrelated boot fields.
- Preserve or intentionally update `public_password`; it controls public preview access and Git actions.
- Manage share links with `latitude share ...` or `/api/shares`.
- Remember listener bind changes are persisted but require a Latitude restart. Project and deployment changes apply immediately.
- Use only URL-safe names for projects and deployments: ASCII letters, digits, `-`, and `_`.

## Direct API Fallback

Use direct HTTP requests only when the CLI is unavailable. Prefer idempotent `PUT` requests when an operation may be repeated.

### Health

```bash
curl http://127.0.0.1:7600/health
```

### Read Boot Config

```bash
curl http://127.0.0.1:7600/api/config
```

### Publish A Markdown Page

```bash
curl -X PUT \
  -H "Content-Type: text/markdown" \
  --data-binary @report.md \
  http://127.0.0.1:7600/api/projects/demo/pages/report
```

The public URL is `http://127.0.0.1:8080/demo/report`.

### Publish An Image Or Video Document

```bash
curl -X PUT \
  -H "Content-Type: image/png" \
  --data-binary @screenshot.png \
  http://127.0.0.1:7600/api/projects/demo/pages/snapshot
```

Use the real media type, such as `image/jpeg`, `image/png`, `image/webp`, `video/mp4`, or `video/webm`. Latitude stores the bytes under the configured data directory and serves them back with the same media type.

For a titled image or video through the direct API, send JSON with base64 content:

```bash
curl -X PUT \
  -H "Content-Type: application/json" \
  -d '{
    "title": "Latest Screenshot",
    "format": "binary",
    "media_type": "image/png",
    "content": "<base64 image bytes>"
  }' \
  http://127.0.0.1:7600/api/projects/demo/pages/snapshot
```

For a titled Markdown or HTML page, send JSON:

```bash
curl -X PUT \
  -H "Content-Type: application/json" \
  -d '{
    "title": "Agent Report",
    "format": "markdown",
    "content": "# Agent Report\n\n- Build passed\n- Demo is ready"
  }' \
  http://127.0.0.1:7600/api/projects/demo/pages/report
```

### Ensure A Project Exists

Use `PUT` for create-or-replace. The path name wins over any mismatched body `name`.

```bash
curl -X PUT \
  -H "Content-Type: application/json" \
  -d '{
    "name": "demo",
    "enabled": true,
    "project_dir": ".",
    "deployments": []
  }' \
  http://127.0.0.1:7600/api/projects/demo
```

Be careful: unlike `latitude project ensure`, this direct `PUT` replaces the whole project, including deployments.

### Deploy A Reverse Proxy

```bash
curl -X PUT \
  -H "Content-Type: application/json" \
  -d '{
    "name": "frontend",
    "enabled": true,
    "kind": "reverse_proxy",
    "upstream": "http://127.0.0.1:5173",
    "strip_prefix": true
  }' \
  http://127.0.0.1:7600/api/projects/demo/deployments/frontend
```

### Deploy A Static Site

```bash
curl -X PUT \
  -H "Content-Type: application/json" \
  -d '{
    "name": "mock",
    "enabled": true,
    "kind": "static",
    "root": "./sites/mock",
    "index_file": "index.html",
    "spa_fallback": true
  }' \
  http://127.0.0.1:7600/api/projects/demo/deployments/mock
```

### Create A Deployment Share Link

Open share:

```bash
curl -X POST \
  -H "Content-Type: application/json" \
  -d '{
    "project": "demo",
    "deployment": "mock"
  }' \
  http://127.0.0.1:7600/api/shares
```

Password-protected, auto-expiring share:

```bash
curl -X POST \
  -H "Content-Type: application/json" \
  -d '{
    "project": "demo",
    "deployment": "mock",
    "password": "review-only",
    "expires_at": 4102444800
  }' \
  http://127.0.0.1:7600/api/shares
```

The response includes `href`, such as `/__latitude/share/<token>/`. Delete a share with:

```bash
curl -X DELETE http://127.0.0.1:7600/api/shares/<token>
```

Page deployment responses contain metadata only. To fetch stored page bytes through the command API, call `GET /api/projects/{project}/pages/{name}/content`.

### Replace Boot Config

```bash
curl -X PUT \
  -H "Content-Type: application/json" \
  -d '{
    "public_bind": "0.0.0.0:8080",
    "command_bind": "127.0.0.1:7600",
    "public_password": "test",
    "data_dir": "latitude-data"
  }' \
  http://127.0.0.1:7600/api/config
```

The config shape is:

```json
{
  "public_bind": "0.0.0.0:8080",
  "command_bind": "127.0.0.1:7600",
  "public_password": "test",
  "data_dir": "latitude-data",
  "desktop": {
    "enabled": false
  }
}
```

## Endpoint Reference

| Method | Path | Use |
| --- | --- | --- |
| `GET` | `/health` | Check process health, listener binds, and counts. |
| `GET` | `/api/config` | Read boot config. |
| `PUT` | `/api/config` | Replace and persist boot config. |
| `GET` | `/api/projects` | List projects. |
| `POST` | `/api/projects` | Create a project, failing on duplicates. |
| `GET` | `/api/projects/{project}` | Read one project. |
| `PUT` | `/api/projects/{project}` | Create or replace one project. |
| `DELETE` | `/api/projects/{project}` | Delete a project and all deployments. |
| `GET` | `/api/projects/{project}/deployments` | List deployments in a project. |
| `POST` | `/api/projects/{project}/deployments` | Create a deployment, failing on duplicates. |
| `GET` | `/api/projects/{project}/deployments/{name}` | Read one deployment. |
| `PUT` | `/api/projects/{project}/deployments/{name}` | Create or replace one deployment. |
| `DELETE` | `/api/projects/{project}/deployments/{name}` | Delete one deployment. |
| `PUT` or `POST` | `/api/projects/{project}/pages/{name}` | Create or replace a page deployment from raw text, raw image/video bytes, or JSON. |
| `GET` | `/api/projects/{project}/pages/{name}/content` | Read stored page deployment bytes. |
| `GET` | `/api/shares` | List deployment share links with password fields redacted. |
| `POST` | `/api/shares` | Create a deployment share link. |
| `GET` | `/api/shares/{token}` | Read one deployment share link with password fields redacted. |
| `DELETE` | `/api/shares/{token}` | Delete one deployment share link. |

## Errors

CLI commands return non-zero when the command API returns an error. Direct API errors return JSON:

```json
{
  "error": "project 'demo' was not found"
}
```

Common API status codes:

- `400`: Invalid config, invalid deployment payload, duplicate project or deployment, or invalid page payload.
- `404`: Project or deployment not found.
- `500`: File, database, or config persistence failure.
