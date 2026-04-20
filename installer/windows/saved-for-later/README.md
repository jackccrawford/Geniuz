# saved-for-later — Windows 10 build recipe

This directory is intentionally empty (except this README). It marks a build
capability we've archived rather than deleted, because a future commercial
customer may need Windows 10 support.

## What was here

Through v1.0.6, the Windows installer bundled two DLLs from Microsoft's ONNX
Runtime 1.22 release, sitting alongside the Geniuz binaries:

- `onnxruntime.dll` (~12 MB)
- `onnxruntime_providers_shared.dll` (~22 KB)

They existed because Win10 systems often lack DirectML 1.13+, which pyke's
default `ort` crate build expects (`DMLCreateDevice1` symbol). Bundling
Microsoft's CPU-only ORT 1.22 DLLs made Geniuz work on Win10 without the
system DirectML.dll.

## Why we removed them

In v1.1.6 (April 2026), Geniuz free/open-source scope narrowed to Windows 11
only. On Win11, pyke's default ORT works cleanly without the bundled
Microsoft 1.22 DLLs — the static ORT is baked into geniuz.exe, DirectML.dll
ships alongside via pyke's copy-dylibs. Shipping the Win10 DLLs as dead
weight would have added ~12 MB to the installer for no Win11 benefit.

The installer now hard-blocks Win10 via `MinVersion=10.0.22000` in
Geniuz.iss. A user on Win10 who tries to install gets a clear "This program
requires Windows 11" message before any files are written.

## How to resurrect Win10 support (for a commercial customer)

When a paying customer needs Win10:

### 1. Download Microsoft's ONNX Runtime 1.22 Windows x64 bundle

```
curl -L -o onnxruntime-win-x64-1.22.0.zip \
  https://github.com/microsoft/onnxruntime/releases/download/v1.22.0/onnxruntime-win-x64-1.22.0.zip
```

Extract it somewhere persistent on the build machine — we used
`C:\Users\orbit\ort-win\onnxruntime-win-x64-1.22.0\` on Orbit.

### 2. Build Geniuz against that ORT with dynamic linking

On the Windows build machine (Orbit):

```
cd C:\Users\orbit\Dev\Geniuz
cargo clean
set ORT_LIB_LOCATION=C:\Users\orbit\ort-win\onnxruntime-win-x64-1.22.0\lib
set ORT_PREFER_DYNAMIC_LINK=1
cargo build --release --features tray
```

Expected binary sizes: `geniuz.exe` ~8.5 MB (dynamic, vs 28+ MB static),
`geniuz-embed.exe` ~6 MB.

### 3. Put the two DLLs back into installer/windows/

Copy from the extracted ORT bundle:
- `onnxruntime-win-x64-1.22.0\lib\onnxruntime.dll`
- `onnxruntime-win-x64-1.22.0\lib\onnxruntime_providers_shared.dll`

into `installer/windows/` (at the same level as `geniuz.exe`).

### 4. Restore the two Source: lines in Geniuz.iss

In the `[Files]` section, re-add:

```
Source: "onnxruntime.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "onnxruntime_providers_shared.dll"; DestDir: "{app}"; Flags: ignoreversion
```

### 5. Lower MinVersion for Win10 support

In the `[Setup]` section, change:

```
MinVersion=10.0.22000
```

to (for Win10 1903 / build 18362+):

```
MinVersion=10.0.18362
```

Or remove the line entirely to accept any Win10+.

### 6. Rebuild with ISCC and test on a Win10 machine

This is the full recipe that produced Windows v1.0.6 successfully. See
station signal 8EF1BC5E for the operational log of the April 15 build that
used this pattern.

## Pattern origin

This dynamic-link + bundle-ORT pattern was borrowed from clawmark commit
7958389, originally developed for ARM64 Linux (Pi 5) where the default
pyke-downloaded ORT required too-new glibc. Same structure, different
operating system. See station signal 00F98E52 for the Pi 5 runbook —
architecturally symmetric.

## If something in Geniuz's build process changes

If `Cargo.toml` changes the `ort` crate version significantly, or if the
crate's `features` list shifts, the env-var protocol may need adjustment.
Newer `ort` crate versions have sometimes renamed the env vars. Check the
current `ort` crate docs at build-restoration time.

— Geniuz team, April 2026
