//! Windows resource embedding — identifies Geniuz binaries to Task Manager.
//!
//! Without a VersionInfo resource, Windows falls back to the executable
//! filename in Task Manager's Name column. With one, Task Manager reads
//! the FileDescription string — which is why Claude Desktop shows up as
//! "Claude" and Dell Optimizer shows up as "Dell Optimizer" while other
//! apps show up as "something.exe".
//!
//! We set one identity for both binaries built here (geniuz.exe and
//! geniuz-embed.exe), so Task Manager reads "Geniuz". The tray on Windows is
//! the dashboard (desktop/dashboard), which has its own.
//!
//! This build script is a no-op on non-Windows targets.

fn main() {
    #[cfg(target_os = "windows")]
    {
        let mut res = winresource::WindowsResource::new();
        res.set("ProductName", "Geniuz");
        res.set("FileDescription", "Geniuz");
        res.set("CompanyName", "Managed Ventures LLC");
        res.set("LegalCopyright", "Copyright (C) 2026 Managed Ventures LLC");
        res.set("ProductVersion", env!("CARGO_PKG_VERSION"));
        res.set("FileVersion", env!("CARGO_PKG_VERSION"));
        if let Err(e) = res.compile() {
            // Don't fail the build if resource embedding fails — the binary
            // still runs, just with the filename fallback in Task Manager.
            eprintln!("[build.rs] winresource embedding failed: {}", e);
        }
    }
}
