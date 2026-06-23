---
name: latitude-command-api
description: "Operate Latitude through its CLI first, with direct local command API requests as the fallback. Use when an agent needs to publish a one-page HTML or Markdown document, create or update Latitude projects, configure reverse proxy or static deployments, inspect or replace Latitude config, or verify local Latitude health."
---

# Latitude CLI And Command API

## Purpose

Use the Latitude CLI as the preferred agent interface. The CLI sends commands to Latitude's local command API, keeps agent workflows consistent, and returns JSON that agents can inspect.

The command API is unauthenticated and must remain loopback-only. Use direct HTTP requests only when the CLI is unavailable.

Default command base URL: `http://127.0.0.1:7600`.
Default public preview URL: `http://127.0.0.1:8080`.
If `/health` reports `public_bind` as `0.0.0.0:8080`, use `127.0.0.1:8080` for local preview.
Public preview pages require the configured `public_password`. The starter password is `test`.

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
latitude deploy static demo mock --root ./sites/mock --spa-fallback
latitude deploy proxy demo frontend --upstream http://127.0.0.1:5173
```

4. Verify the returned `public_url`. Deployments mount at `/{project}/{deployment}` on the public listener.

The CLI prints JSON responses. Publish and deploy commands include `public_url` plus the deployment object returned by Latitude.

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

`config put` replaces the full active config. Preserve unrelated fields, especially `public_password`.

### Projects

```bash
latitude project list
latitude project get demo
latitude project ensure demo --project-dir .
```

`project ensure` creates the project only if it is missing, so it does not erase existing deployments.

### Publish A Markdown Or HTML Page

Use this for status reports, handoff notes, generated documentation, and quick agent output.

```bash
latitude publish page demo report --file report.md --title "Agent Report" --format markdown
```

If the project may not exist yet:

```bash
latitude publish page demo report --file report.md --project-dir . --title "Agent Report" --format markdown
```

Page payload rules:

- `--format markdown` publishes Markdown.
- `--format html` publishes HTML.
- `--format auto` infers from the file extension and content.
- Omit `--file` or pass `--file -` to read content from stdin.
- Page content must be UTF-8 and at most 2 MiB.
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

## Safety Rules

- Never bind or expose the command API outside loopback. `command_bind` must use `127.0.0.1` or `[::1]`.
- Treat config replacement as a full replacement. Fetch the current config first and preserve unrelated fields.
- Preserve or intentionally update `public_password`; it controls public preview access and Git actions.
- Remember listener bind changes are persisted but require a Latitude restart. Project and deployment changes apply immediately.
- Use only URL-safe names for projects and deployments: ASCII letters, digits, `-`, and `_`.

## Direct API Fallback

Use direct HTTP requests only when the CLI is unavailable. Prefer idempotent `PUT` requests when an operation may be repeated.

### Health

```bash
curl http://127.0.0.1:7600/health
```

### Read Current Config

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

To include a title, send JSON:

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

### Replace Full Config

```bash
curl -X PUT \
  -H "Content-Type: application/json" \
  --data @latitude.example.json \
  http://127.0.0.1:7600/api/config
```

The config shape is:

```json
{
  "public_bind": "0.0.0.0:8080",
  "command_bind": "127.0.0.1:7600",
  "public_password": "test",
  "projects": []
}
```

## Endpoint Reference

| Method | Path | Use |
| --- | --- | --- |
| `GET` | `/health` | Check process health, listener binds, and counts. |
| `GET` | `/api/config` | Read active config. |
| `PUT` | `/api/config` | Replace and persist the full config. |
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
| `PUT` or `POST` | `/api/projects/{project}/pages/{name}` | Create or replace a page deployment from raw text or JSON. |

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
- `500`: File or config persistence failure.
