# Windows installer

Inno Setup script (`Geniuz.iss`) for building `Geniuz-Setup.exe`, a per-user
Windows installer. Everything below is read from `Geniuz.iss`, the signing
scripts beside it, and the signature on the last built installer. Where this
file and the script disagree, the script is right; fix this file.

## What the installer does (from `Geniuz.iss`)

- **Per-user only.** `PrivilegesRequired=lowest`; no admin elevation, no
  all-users option. Installs to `%LOCALAPPDATA%\Programs\Geniuz\`.
- **Windows 11 or later, x64.** `MinVersion=10.0.22000` hard-blocks at
  install start with a clear message. A Windows 10 recipe is archived under
  `saved-for-later/`.
- **Ships three binaries:** `geniuz.exe`, `geniuz-embed.exe`, and
  `geniuz-dashboard.exe` (the tray app). Plus `Geniuz.ico`.
- **Memory location page.** A wizard page asks where memories live; default
  `%USERPROFILE%\.geniuz`. The folder is created in user context. In a silent
  install the default is used.
- **Registry, all `HKCU`:** the install dir is appended to the user `Path`
  (only if absent); `GENIUZ_HOME` is set to the chosen memory folder; the
  dashboard is autostarted at login via
  `Software\Microsoft\Windows\CurrentVersion\Run` (value removed on uninstall).
  Nothing is written under `HKLM`; there is no machine-level policy key.
- **Post-install steps, hidden:** `icacls` grants ALL APPLICATION PACKAGES
  modify rights on the memory folder (so sandboxed Claude Desktop can open the
  database); `geniuz mcp install --env GENIUZ_HOME=<folder>` writes the Claude
  Desktop MCP config with the path embedded (sandboxed Claude does not inherit
  the user's environment). Then the dashboard is launched; that step is
  skipped in a silent install.
- **Uninstall** removes the `Path` entry and `GENIUZ_HOME`, and leaves the
  memory folder and database in place by design.

Uninstall first stops the running tray by the name in `MyAppTrayExeName`
(`geniuz-dashboard.exe`). Until 4.0.0 this step named the old binary,
`geniuz-tray.exe`, so the dashboard survived uninstall and its executable
could not be deleted.

Version: `MyAppVersion` is defined at the top of `Geniuz.iss` and is set by
hand; keep it in step with the crate version when cutting a release (both
are 4.0.0 as of this commit).

## Silent / managed install

Standard Inno Setup switches work unchanged:

```powershell
Geniuz-Setup.exe /VERYSILENT /SUPPRESSMSGBOXES /NORESTART
```

Because the install is per-user, a management tool must run it in the
logged-on user's context, not as SYSTEM. The memory folder takes its default,
the MCP config is written, the autostart entry is set, and the dashboard is
not launched until the next login.

## Build

Requires Inno Setup 6+ (`ISCC.exe`) on a Windows machine:

```powershell
choco install innosetup -y
```

Copy the three signed binaries from `target/x86_64-pc-windows-msvc/release/`
alongside `Geniuz.iss`, then:

```powershell
& 'C:\Program Files (x86)\Inno Setup 6\ISCC.exe' Geniuz.iss
```

Output: `output\Geniuz-Setup.exe` (the 6 June 2026 build is about 23 MB).

## Sign

Dual-signing: the three inner binaries are signed before ISCC bundles them,
and the outer `Geniuz-Setup.exe` is signed after. The inner pass is what
stops Windows raising a fresh warning the first time a user launches
`geniuz.exe` or the dashboard after install.

Two signing paths exist in this directory:

- **`sign-installer-trustedsigning.sh` (current).** Azure Trusted Signing
  through `jsign`, account `MVLLC`, profile `geniuz-free-prod`, endpoint
  `wus2.codesigning.azure.net`, RFC 3161 timestamp from Microsoft. Needs
  `brew install jsign azure-cli` and `az login`; a fresh access token is
  fetched per run and nothing is cached. The installer in `output/` is signed
  this way: its signature chains to "Microsoft ID Verified Code Signing PCA
  2021" for subject "Managed Ventures LLC", timestamped 6 June 2026 (checked
  with `osslsigncode verify`).
- **`sign-installer.sh` (retained).** The EV certificate on the YubiKey FIPS
  (PIV slot 9A) through `osslsigncode`, timestamp from `ts.ssl.com`, prompts
  for the PIN once per file. Kept as the fallback when Azure is unavailable.

`sign-binaries.sh` signs the three inner binaries in place with Trusted
Signing by default; set `GENIUZ_SIGN_CMD=./sign-installer.sh` to use the
hardware key instead. Its binary list must match the `[Files]` entries in
`Geniuz.iss`.

Verify any signed file from the Mac with `osslsigncode verify -in <file>`.
The Mac copy of that tool may lack Microsoft's timestamp root and report the
timestamp chain as unverified; the signer identity is still shown. On Windows
use `signtool verify /pa /v <file>`.

## Why per-user, not Program Files

Per-user means no admin elevation, no UAC prompt, faster install. Geniuz is a
personal-memory tool: installing it under one Windows account does not make
sense to share with another. The Mac install pattern is the same.

## Why Inno Setup, not NSIS or MSI

NSIS and Inno Setup direct downloads both failed repeatedly (mirror drift);
Chocolatey absorbed it. Inno Setup has the better default wizard for
non-technical users. MSI via WiX would be more enterprise-friendly but is a
heavier toolchain; for a managed deployment the silent switches above are
enough.
