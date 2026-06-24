# UltraVNC Helper

Place the UltraVNC server helper files for managed desktop mode in this directory.

From the repository root, run:

```powershell
.\init-ultravnc.ps1
```

Latitude expects `winvnc.exe` here by default:

```text
tools/ultravnc/winvnc.exe
```

If you redistribute these files with Latitude, include UltraVNC's GPL license and any required source-offer materials alongside the helper bundle.

UltraVNC 1.8.x reads `ultravnc.ini` from a standard Windows config directory unless `ultravnc.portable` exists beside `winvnc.exe`. The init script creates that marker so Latitude can manage the local helper config in this directory.
