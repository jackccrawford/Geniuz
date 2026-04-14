# Windows installer

Inno Setup script for building `Geniuz-Setup.exe` — per-user Windows installer.

## Build

Requires Inno Setup 6+ (`ISCC.exe`). Easiest install on Windows:

```powershell
choco install innosetup -y
```

Then from this directory, with `geniuz.exe` and `geniuz-embed.exe` from
`target/x86_64-pc-windows-msvc/release/` copied alongside `Geniuz.iss`:

```powershell
cd path\to\staging-dir
& 'C:\Program Files (x86)\Inno Setup 6\ISCC.exe' Geniuz.iss
```

Output: `output\Geniuz-Setup.exe` (~13 MB, LZMA2 compressed).

## What it does

- Per-user install (no admin required)
- Installs to `%LOCALAPPDATA%\Programs\Geniuz\`
- Adds install dir to user PATH (idempotent)
- Runs `geniuz mcp install` postinstall — wires Claude Desktop config at
  `%APPDATA%\Claude\claude_desktop_config.json`
- Generates uninstaller (`unins000.exe`) that reverses everything but
  preserves `~/.geniuz/memory.db` (user data is sacred)

## Why per-user, not Program Files

Per-user means no admin elevation, no UAC prompt, faster install. Geniuz is a
personal-memory tool — installing it under one Windows account doesn't make
sense to share with another. Mac install pattern is the same (`~/Applications`
or `/Applications` is a user choice, MCP config is per-user).

## Why Inno Setup, not NSIS or MSI

Tried NSIS first — direct downloads from SourceForge consistently failed
across multiple mirrors (corrupted bytes, 404s). Tried Inno Setup direct
downloads from jrsoftware.org — same problem (404s on multiple version URLs).
Chocolatey absorbed the URL drift cleanly. Inno Setup also has nicer default
UX than NSIS for non-technical users.

MSI via WiX would be more enterprise-friendly but heavier toolchain. Inno
Setup is the right balance for a consumer product shipped to individual users.
