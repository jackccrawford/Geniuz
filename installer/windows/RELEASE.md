# Windows release procedure (current, October 2026)

Build on the Mac, sign on the Mac; a clean Windows machine (`$WIN`, its
ssh address) only builds the Windows binaries and tests the install. This replaced the
May 2026 YubiKey/SSL.com playbook; Trusted Signing is the signer now.

From the Mac that signs, for release `vX.Y.Z` at commit `C`:

```bash
# 1. A fresh export to a versioned folder on that machine (never a checkout there).
git archive --format=tar C | ssh $WIN "mkdir C:\build\geniuz-X.Y.Z && tar -xf - -C C:\build\geniuz-X.Y.Z && cd /d C:\build\geniuz-X.Y.Z && cargo build --release --target x86_64-pc-windows-msvc --bin geniuz --bin geniuz-embed && cd desktop\dashboard && cargo tauri build --no-bundle"

# 2. Bring the three binaries to the Mac and sign them (Azure Trusted Signing; needs az login).
ST=release-artifacts/vX.Y.Z/windows-staging; mkdir -p $ST
scp $WIN:C:/build/geniuz-X.Y.Z/target/x86_64-pc-windows-msvc/release/{geniuz,geniuz-embed}.exe \
    $WIN:C:/build/geniuz-X.Y.Z/desktop/dashboard/target/release/geniuz-dashboard.exe $ST/
(cd installer/windows && ./sign-binaries.sh ../../$ST)

# 3. Send the signed binaries back and compile the installer there (Inno Setup).
scp $ST/*.exe $WIN:C:/build/geniuz-X.Y.Z/installer/windows/
ssh $WIN "cd /d C:\build\geniuz-X.Y.Z\installer\windows && ISCC.exe Geniuz.iss"

# 4. Bring the installer back and sign it on the Mac; verify on that machine.
scp $WIN:C:/build/geniuz-X.Y.Z/installer/windows/output/Geniuz-Setup.exe release-artifacts/vX.Y.Z/
(cd installer/windows && ./sign-installer-trustedsigning.sh ../../release-artifacts/vX.Y.Z/Geniuz-Setup.exe)
# Get-AuthenticodeSignature there should say Valid, Managed Ventures LLC.

# 5. Upload with the release (gh release upload vX.Y.Z release-artifacts/vX.Y.Z/Geniuz-Setup.exe).
```

Testing a download the way a user would: `Invoke-WebRequest` sets no
Mark-of-the-Web, so SmartScreen is not exercised. Set `ZoneId=3` on the file
first, then run it.

The certificate's subject reads `S=California`; Scottsdale is in Arizona. The
correction is made in the Azure Trusted Signing identity, not here.
