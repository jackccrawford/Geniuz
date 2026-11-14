import Foundation

/// Puts the `geniuz` command where Terminal finds it, with no password.
///
/// The Mac has no installer: the app is dragged into Applications, so nothing
/// else can do this. A person in a demo could not find the button for it and did
/// not know what a menu bar item was (Jack, 13 Nov 2026: "the path should be
/// automated"). So the app does it at launch:
///
/// 1. If a folder of the person's own is already on their PATH (`~/.local/bin`,
///    or Homebrew's), a link there is all it takes.
/// 2. Otherwise it makes `~/.local/bin`, links there, and adds one marked line
///    to the shell's startup file so new Terminal windows find it. This is what
///    rustup and similar installers do. `/usr/local/bin` would need a password.
///
/// Nothing here asks for anything, and nothing is replaced that isn't a link.
/// It takes its surroundings as values so it can be run against a scratch home.
struct CommandInstaller {
    /// The person's home directory.
    let home: String
    /// The `geniuz` inside the app, which the link points to.
    let bundled: String
    /// Their login shell, e.g. `/bin/zsh`.
    let shell: String
    /// The PATH a new Terminal window would have, or nil if it couldn't be read.
    let pathDirs: [String]?
    /// System-wide folders an earlier install may have linked into. A test passes none.
    var systemDirs = ["/usr/local/bin", "/opt/homebrew/bin"]
    /// False once this app has added its PATH line before: if the person has since
    /// taken the line out, that is their answer, and it isn't put back.
    var mayAddPathLine = true

    static let marker = "# Added by Geniuz, so `geniuz` works in Terminal. Safe to remove."
    static let pathLine = "export PATH=\"$HOME/.local/bin:$PATH\""

    enum Outcome: Equatable {
        /// Linked into a folder that was already on the PATH.
        case linked(dir: String)
        /// Linked into `~/.local/bin`, and that folder was added to the PATH in `file`.
        case linkedAndPathAdded(dir: String, file: String)
        /// Linked into `~/.local/bin`, whose PATH line was already in `file`.
        case linkedPathAlreadyAdded(dir: String, file: String)
        /// Not done: an unfamiliar shell, or something in the way that isn't ours.
        case notPossible
    }

    var localBin: String { "\(home)/.local/bin" }

    /// The startup file every new Terminal window of this shell reads, or nil for
    /// a shell this doesn't know how to set up (fish, nushell and so on).
    var startupFile: String? {
        switch (shell as NSString).lastPathComponent {
        case "zsh": return "\(home)/.zshenv"
        case "bash": return "\(home)/.bash_profile"
        default: return nil
        }
    }

    /// Whether `geniuz` already works in a new Terminal window through a link of ours.
    func alreadyInstalled() -> Bool {
        let fm = FileManager.default
        // A system-wide link counts only where Terminal looks: Homebrew's folder isn't
        // on every PATH. With the PATH unknown, a link there is taken at its word.
        for dir in systemDirs where fm.isExecutableFile(atPath: "\(dir)/geniuz") {
            if pathDirs?.contains(dir) ?? true { return true }
        }
        guard fm.isExecutableFile(atPath: "\(localBin)/geniuz") else { return false }
        if pathDirs?.contains(localBin) ?? false { return true }
        return startupFile.map(hasOurLine) ?? false
    }

    func install() -> Outcome {
        let fm = FileManager.default
        // 1. A folder of theirs that is already on the PATH.
        if let onPath = pathDirs {
            for dir in [localBin, "/opt/homebrew/bin"] where onPath.contains(dir) && fm.isWritableFile(atPath: dir) {
                if link(in: dir) { return .linked(dir: dir) }
            }
        }
        // 2. Their own ~/.local/bin, added to the PATH in the shell's startup file.
        guard let file = startupFile else { return .notPossible }
        // A startup file that is a link (a dotfiles setup) or can't be written is theirs to manage.
        if (try? fm.destinationOfSymbolicLink(atPath: file)) != nil { return .notPossible }
        if fm.fileExists(atPath: file) && !fm.isWritableFile(atPath: file) { return .notPossible }
        // A file that is there but can't be read as text is never rewritten: taking it
        // for empty would replace someone's shell setup with two lines.
        var existing = ""
        if fm.fileExists(atPath: file) {
            guard let text = try? String(contentsOfFile: file, encoding: .utf8) else { return .notPossible }
            existing = text
        }
        let lineIsThere = hasOurLine(file)
        guard lineIsThere || mayAddPathLine else { return .notPossible }
        do {
            try fm.createDirectory(atPath: localBin, withIntermediateDirectories: true)
        } catch {
            return .notPossible
        }
        guard link(in: localBin) else { return .notPossible }
        if lineIsThere { return .linkedPathAlreadyAdded(dir: localBin, file: file) }
        let gap = existing.isEmpty || existing.hasSuffix("\n") ? "" : "\n"
        let added = existing + gap + "\n\(Self.marker)\n\(Self.pathLine)\n"
        do {
            try added.write(toFile: file, atomically: true, encoding: .utf8)
        } catch {
            return .notPossible
        }
        return .linkedAndPathAdded(dir: localBin, file: file)
    }

    /// Our PATH line itself, not only the comment above it: a comment left behind
    /// after the line was removed puts nothing on the PATH.
    private func hasOurLine(_ file: String) -> Bool {
        guard let text = try? String(contentsOfFile: file, encoding: .utf8) else { return false }
        return text.split(separator: "\n").contains { $0.trimmingCharacters(in: .whitespaces) == Self.pathLine }
    }

    /// Replace an old link, but never a real file that isn't ours.
    private func link(in dir: String) -> Bool {
        let fm = FileManager.default
        let path = "\(dir)/geniuz"
        if (try? fm.destinationOfSymbolicLink(atPath: path)) != nil {
            try? fm.removeItem(atPath: path)
        } else if fm.fileExists(atPath: path) {
            return false
        }
        return (try? fm.createSymbolicLink(atPath: path, withDestinationPath: bundled)) != nil
    }
}
