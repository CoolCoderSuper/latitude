# Running Latitude With Cloudflare Tunnel

This runbook explains how to run Latitude locally and expose its public proxy through Cloudflare Tunnel.

Latitude has two listeners:

- Public proxy: `0.0.0.0:5597` or `127.0.0.1:5597`
- Local command API: `127.0.0.1:7600`

Only expose the public proxy through Cloudflare. Do not expose the command API; it is intentionally unauthenticated and local-only.

The public proxy requires the configured `public_password` before it serves pages or runs Git actions. Deployment share links under `/__latitude/share/<token>/` can instead use their own optional per-link password. The starter config currently uses `test`.

## Local Startup

From the repository root:

```powershell
cd C:\CodingCool\Code\Projects\latitude
cargo build
target\debug\latitude.exe --config latitude.json --public-bind 0.0.0.0:5597 --command-bind 127.0.0.1:7600
```

Use `127.0.0.1:5597` instead of `0.0.0.0:5597` if Cloudflare Tunnel is the only way traffic should reach Latitude:

```powershell
target\debug\latitude.exe --config latitude.json --public-bind 127.0.0.1:5597 --command-bind 127.0.0.1:7600
```

Check the local server:

```powershell
Invoke-RestMethod http://127.0.0.1:7600/health
Invoke-WebRequest http://127.0.0.1:5597/ -UseBasicParsing
```

## Development Tunnel

For quick testing without a Cloudflare account or DNS record:

```powershell
cloudflared tunnel --url http://localhost:5597
```

Cloudflare prints a random `https://...trycloudflare.com` URL. This URL is temporary and changes whenever the tunnel is recreated.

To run the quick tunnel detached with logs:

```powershell
$root = "C:\CodingCool\Code\Projects\latitude"
$logs = Join-Path $root "logs"
New-Item -ItemType Directory -Force -Path $logs | Out-Null
$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$latitudeExe = Join-Path $root "target\debug\latitude.exe"
$cloudflaredExe = (Get-Command cloudflared).Source

Start-Process `
  -FilePath $latitudeExe `
  -ArgumentList @("--config","latitude.json","--public-bind","0.0.0.0:5597","--command-bind","127.0.0.1:7600") `
  -WorkingDirectory $root `
  -RedirectStandardOutput (Join-Path $logs "latitude-$stamp.out.log") `
  -RedirectStandardError (Join-Path $logs "latitude-$stamp.err.log") `
  -WindowStyle Hidden

Start-Sleep -Seconds 2

Start-Process `
  -FilePath $cloudflaredExe `
  -ArgumentList @("tunnel","--url","http://localhost:5597") `
  -WorkingDirectory $root `
  -RedirectStandardOutput (Join-Path $logs "cloudflared-$stamp.out.log") `
  -RedirectStandardError (Join-Path $logs "cloudflared-$stamp.err.log") `
  -WindowStyle Hidden
```

Find the generated quick-tunnel URL:

```powershell
Select-String -Path .\logs\cloudflared-*.out.log,.\logs\cloudflared-*.err.log -Pattern "https://[a-z0-9-]+\.trycloudflare\.com"
```

## Proper Cloudflare Tunnel

For a stable public hostname, use a named Cloudflare Tunnel. This requires a Cloudflare account and a domain using Cloudflare DNS.

There are two common ways to manage the tunnel:

- Remotely managed in the Cloudflare dashboard. This is usually easiest for a durable Windows setup.
- Locally managed with `cloudflared tunnel create`, a local `config.yml`, and `cloudflared tunnel run`.

### Option A: Dashboard Managed

1. Open Cloudflare Zero Trust.
2. Go to `Networks` -> `Tunnels`.
3. Create a tunnel named `latitude`.
4. Choose `cloudflared` as the connector.
5. Copy the Windows service install command shown by Cloudflare. It will look like this:

```cmd
cloudflared.exe service install <TUNNEL_TOKEN>
```

Run it from an administrator Command Prompt.

Then add a published application route:

- Hostname: `latitude.example.com`
- Service URL: `http://localhost:5597`

After saving, open:

```text
https://latitude.example.com/
```

### Option B: Locally Managed

Authenticate `cloudflared`:

```powershell
cloudflared tunnel login
```

Create the named tunnel:

```powershell
cloudflared tunnel create latitude
cloudflared tunnel list
```

Create `%USERPROFILE%\.cloudflared\config.yml`:

```yaml
tunnel: <Tunnel-UUID>
credentials-file: C:\Users\<You>\.cloudflared\<Tunnel-UUID>.json

ingress:
  - hostname: latitude.example.com
    service: http://localhost:5597
  - service: http_status:404
```

Create the DNS route:

```powershell
cloudflared tunnel route dns latitude latitude.example.com
```

Validate the config:

```powershell
cloudflared tunnel ingress validate
cloudflared tunnel ingress rule https://latitude.example.com/
```

Run the tunnel:

```powershell
cloudflared tunnel run latitude
```

## Windows Service Notes

Cloudflare recommends running `cloudflared` as a service for durable availability. For dashboard-managed tunnels, use the token-based service install command shown in the Cloudflare dashboard:

```cmd
cloudflared.exe service install <TUNNEL_TOKEN>
```

For locally managed tunnels, `cloudflared` expects a config file containing at least:

- `tunnel`
- `credentials-file`

The default config location is:

```text
%USERPROFILE%\.cloudflared\config.yml
```

Latitude itself must also be kept running. During development, the detached `Start-Process` commands above are enough. For a real always-on machine, run `target\debug\latitude.exe` or a release binary from a service manager or scheduled task.

Build a release binary:

```powershell
cargo build --release
target\release\latitude.exe --config latitude.json --public-bind 127.0.0.1:5597 --command-bind 127.0.0.1:7600
```

## Firewall Notes

Cloudflare Tunnel uses outbound connections from `cloudflared` to Cloudflare. You usually do not need to open inbound Windows firewall port `5597` if all public traffic comes through Cloudflare.

Open the inbound port only if you also want direct LAN or public-IP access:

```powershell
New-NetFirewallRule -DisplayName "Latitude TCP 5597" -Direction Inbound -Action Allow -Protocol TCP -LocalPort 5597
```

Remove an old direct-access rule:

```powershell
Remove-NetFirewallRule -DisplayName "Latitude TCP 6697" -ErrorAction SilentlyContinue
```

## Operational Checks

Check local health:

```powershell
Invoke-RestMethod http://127.0.0.1:7600/health
```

Check the public gateway:

```powershell
Invoke-WebRequest http://127.0.0.1:5597/ -UseBasicParsing
Invoke-WebRequest https://latitude.example.com/ -UseBasicParsing
```

Check running processes:

```powershell
Get-Process latitude,cloudflared -ErrorAction SilentlyContinue |
  Select-Object ProcessName, Id, StartTime
```

Check recent tunnel logs:

```powershell
Get-ChildItem .\logs -Filter "cloudflared-*.log" |
  Sort-Object LastWriteTime -Descending |
  Select-Object -First 5
```

## Troubleshooting

If the quick tunnel hostname stops resolving, the quick tunnel process probably exited. Start a new quick tunnel and use the new URL.

If Cloudflare returns an origin error, make sure Latitude is listening locally:

```powershell
Invoke-WebRequest http://127.0.0.1:5597/ -UseBasicParsing
```

If `cloudflared` is not found after installation, refresh the shell environment:

```cmd
refreshenv
where cloudflared
cloudflared --version
```

If a named tunnel does not route, check:

```powershell
cloudflared tunnel list
cloudflared tunnel info latitude
cloudflared tunnel ingress validate
cloudflared tunnel ingress rule https://latitude.example.com/
```

## References

- Cloudflare Quick Tunnels: https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/do-more-with-tunnels/trycloudflare/
- Cloudflare dashboard-managed tunnel setup: https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/get-started/create-remote-tunnel/
- Cloudflare locally managed tunnel setup: https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/do-more-with-tunnels/local-management/create-local-tunnel/
- Cloudflare tunnel configuration file: https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/do-more-with-tunnels/local-management/configuration-file/
- Cloudflare Windows service setup: https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/do-more-with-tunnels/local-management/as-a-service/windows/
