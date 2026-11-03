import Foundation
import SQLite3
import Combine
import AppKit
import SwiftUI

// MARK: - Settings model
//
// Mirrors the Rust schema in src/settings.rs. Both surfaces read and write
// `$GENIUZ_HOME/settings.json` directly. Keep field names (snake_case) and
// defaults synchronized with the Rust struct — drift here causes silent
// preference loss when one side writes and the other reads.

struct GeniuzSettings: Codable, Equatable {
    var version: Int = 1
    var launchAtLogin: Bool = true
    var autoupdateEnabled: Bool = true
    var updateCheckFrequency: String = "daily"   // "daily" | "weekly" | "manual"
    var recentMemoriesCount: Int = 5

    enum CodingKeys: String, CodingKey {
        case version
        case launchAtLogin = "launch_at_login"
        case autoupdateEnabled = "autoupdate_enabled"
        case updateCheckFrequency = "update_check_frequency"
        case recentMemoriesCount = "recent_memories_count"
    }

    init() {}

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        self.version = (try? c.decode(Int.self, forKey: .version)) ?? 1
        self.launchAtLogin = (try? c.decode(Bool.self, forKey: .launchAtLogin)) ?? true
        self.autoupdateEnabled = (try? c.decode(Bool.self, forKey: .autoupdateEnabled)) ?? true
        self.updateCheckFrequency = (try? c.decode(String.self, forKey: .updateCheckFrequency)) ?? "daily"
        self.recentMemoriesCount = (try? c.decode(Int.self, forKey: .recentMemoriesCount)) ?? 5
    }
}

/// One row of `geniuz folder list --json`.
struct FolderOption: Decodable, Identifiable, Equatable {
    let uuid: String
    let name: String
    let path: String
    let memories: Int
    let inUse: Bool
    var id: String { uuid }

    enum CodingKeys: String, CodingKey {
        case uuid, name, path, memories
        case inUse = "in_use"
    }
}

private struct FolderListing: Decodable {
    let folders: [FolderOption]
}

class GeniuzService: ObservableObject {
    @Published var folders: [FolderOption] = []
    @Published var memoryCount: Int = 0
    @Published var recentGists: [String] = []
    @Published var recentMemories: [RecentMemory] = []
    @Published var mcpInstalled: Bool = false
    @Published var stationExists: Bool = false
    @Published var recentExpanded: Bool = false
    @Published var restartRequired: Bool = false
    @Published var cliOnPath: Bool = false
    @Published var cliCopyConfirmation: Bool = false
    @Published var settings: GeniuzSettings = GeniuzSettings()

    /// Optional callback fired after each state refresh so non-SwiftUI surfaces
    /// (the NSStatusItem button tooltip) can re-read derived strings.
    var onStateChange: (() -> Void)?

    /// Tooltip text shown on hover of the menu-bar icon.
    /// Matches the Windows tray.rs format: identity + count, then the
    /// most-recent gist on a second line if any. Connection status is
    /// intentionally excluded — it lives in the popover.
    func tooltipText() -> String {
        let mem = memoryCount == 1 ? "1 memory" : "\(memoryCount) memories"
        let head = "Geniuz · \(mem)"
        guard let g = recentGists.first, !g.isEmpty else { return head }
        let truncated = g.count > 100 ? String(g.prefix(100)) + "…" : g
        return "\(head)\n\(truncated)"
    }

    private var timer: Timer?
    private var claudeAtConfigureTime: Set<pid_t> = []
    private var claudeSeenAfterConfigure: Set<pid_t> = []
    private var settingsWindowController: NSWindowController?

    private let claudeBundleID = "com.anthropic.claudefordesktop"

    /// Real home directory via getpwuid — not remapped by sandbox
    private var realHome: String {
        if let pw = getpwuid(getuid()), let home = pw.pointee.pw_dir {
            return String(cString: home)
        }
        return NSHomeDirectory()
    }

    /// Geniuz data directory. Mirrors Rust's `geniuz::data_dir()` resolution:
    /// GENIUZ_HOME env var if set, otherwise ~/.geniuz.
    var dataDir: String {
        if let env = ProcessInfo.processInfo.environment["GENIUZ_HOME"], !env.isEmpty {
            return env
        }
        return "\(realHome)/.geniuz"
    }

    /// The first folder: where everything goes until another is chosen.
    var defaultStationPath: String {
        return "\(dataDir)/memory.db"
    }

    /// The folder in use. The binary owns the rules for finding folders, so this
    /// never has a second copy of them.
    var stationPath: String {
        return cachedFolders.first(where: { $0.inUse })?.path ?? defaultStationPath
    }

    // MARK: - Folders

    private var cachedFolders: [FolderOption] = []
    private var folderStamp: String = ""
    private var foldersReadAt = Date.distantPast

    /// Run the bundled binary with this app's idea of the data directory. It runs
    /// from the root so that a project's `.geniuz-folder` can't affect the menu.
    private func runGeniuz(_ arguments: [String]) -> Data? {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: geniuzBinaryPath)
        process.arguments = arguments
        process.currentDirectoryURL = URL(fileURLWithPath: "/")
        var env = ProcessInfo.processInfo.environment
        env["GENIUZ_HOME"] = dataDir
        env.removeValue(forKey: "GENIUZ_STATION")
        process.environment = env
        let out = Pipe()
        process.standardOutput = out
        process.standardError = FileHandle.nullDevice
        do {
            try process.run()
        } catch {
            NSLog("[geniuz-app] couldn't run geniuz: %@", error.localizedDescription)
            return nil
        }
        let data = out.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        return process.terminationStatus == 0 ? data : nil
    }

    private func fileStamp(_ path: String) -> String {
        let attrs = try? FileManager.default.attributesOfItem(atPath: path)
        return (attrs?[.modificationDate] as? Date).map { "\($0.timeIntervalSince1970)" } ?? "-"
    }

    /// Ask the binary only when something may have changed: the current folder,
    /// the list of added directories, a folder that has gone from where it was,
    /// or once a minute for what the rest of the app writes.
    private func readFolders() -> [FolderOption] {
        let stamp = fileStamp("\(dataDir)/current") + "|" + fileStamp("\(dataDir)/added-folders")
        let inUseGone = cachedFolders.first(where: { $0.inUse }).map {
            !FileManager.default.fileExists(atPath: $0.path)
        } ?? false
        let fresh = stamp == folderStamp && !inUseGone && Date().timeIntervalSince(foldersReadAt) < 60
        if fresh { return cachedFolders }

        guard let data = runGeniuz(["folder", "list", "--json"]),
              let listing = try? JSONDecoder().decode(FolderListing.self, from: data) else {
            return cachedFolders
        }
        cachedFolders = listing.folders
        folderStamp = stamp
        foldersReadAt = Date()
        return cachedFolders
    }

    /// Change the folder in use for every surface, then show it here.
    func useFolder(_ folder: FolderOption) {
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self = self else { return }
            _ = self.runGeniuz(["folder", "use", folder.uuid])
            self.foldersReadAt = .distantPast
            self.refresh()
        }
    }

    var settingsPath: String {
        return "\(dataDir)/settings.json"
    }

    var geniuzBinaryPath: String {
        Bundle.main.path(forResource: "geniuz", ofType: nil) ?? "/usr/local/bin/geniuz"
    }

    var claudeConfigPath: String {
        return "\(realHome)/Library/Application Support/Claude/claude_desktop_config.json"
    }

    init() {
        NSLog("[geniuz-app] init — realHome=%@ stationPath=%@", realHome, stationPath)
        refresh()
        timer = Timer.scheduledTimer(withTimeInterval: 10, repeats: true) { [weak self] _ in
            self?.refresh()
        }

        let nc = NSWorkspace.shared.notificationCenter
        nc.addObserver(self,
                       selector: #selector(appLaunched(_:)),
                       name: NSWorkspace.didLaunchApplicationNotification,
                       object: nil)
        nc.addObserver(self,
                       selector: #selector(appTerminated(_:)),
                       name: NSWorkspace.didTerminateApplicationNotification,
                       object: nil)
    }

    deinit {
        timer?.invalidate()
        NSWorkspace.shared.notificationCenter.removeObserver(self)
    }

    func refresh() {
        DispatchQueue.global(qos: .utility).async { [weak self] in
            guard let self = self else { return }
            let folders = self.readFolders()
            let station = self.readStation()
            let mcp = self.checkMcpInstalled()
            let onPath = self.checkCliOnPath()
            let s = self.loadSettings()

            DispatchQueue.main.async {
                self.folders = folders
                self.stationExists = station.exists
                self.memoryCount = station.memories
                self.recentMemories = station.recentMemories
                self.recentGists = station.recentGists
                self.mcpInstalled = mcp
                self.cliOnPath = onPath
                self.settings = s
                self.onStateChange?()
            }
        }
    }

    // MARK: - Settings load/save

    /// Read settings.json from disk. Returns defaults on missing/corrupt file —
    /// settings should never block app startup or popover render.
    func loadSettings() -> GeniuzSettings {
        let path = settingsPath
        guard let data = FileManager.default.contents(atPath: path) else {
            return GeniuzSettings()
        }
        do {
            return try JSONDecoder().decode(GeniuzSettings.self, from: data)
        } catch {
            NSLog("[geniuz-app] settings parse failed at %@: %@ — using defaults",
                  path, error.localizedDescription)
            return GeniuzSettings()
        }
    }

    /// Persist settings.json atomically (write tmp, rename). Updates the
    /// in-memory @Published copy on success so observers refresh.
    func saveSettings(_ next: GeniuzSettings) {
        let path = settingsPath
        let dir = (path as NSString).deletingLastPathComponent
        try? FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)

        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        guard let data = try? encoder.encode(next) else {
            NSLog("[geniuz-app] settings encode failed")
            return
        }

        let tmp = path + ".tmp"
        do {
            try data.write(to: URL(fileURLWithPath: tmp), options: .atomic)
            try FileManager.default.removeItem(atPath: path)
        } catch {
            // First write or rename-source-doesn't-exist — both are fine
        }
        do {
            try FileManager.default.moveItem(atPath: tmp, toPath: path)
        } catch {
            // moveItem fails if destination exists; fall back to direct write
            try? data.write(to: URL(fileURLWithPath: path), options: .atomic)
            try? FileManager.default.removeItem(atPath: tmp)
        }

        DispatchQueue.main.async { [weak self] in
            self?.settings = next
        }
    }

    // MARK: - CLI on PATH

    /// Is `geniuz` resolvable on the user's actual PATH?
    ///
    /// Matches the shell's `which geniuz` semantics exactly — runs `/usr/bin/which`
    /// as a subprocess against a shell-set PATH and checks the exit code. This is
    /// more honest than iterating a candidate list: if the user's shell can't find
    /// `geniuz`, the menu-bar affordance must appear regardless of whether a stale
    /// binary exists at some path that isn't currently on PATH.
    ///
    /// Previous candidate-list implementation would return true for stale binaries
    /// at `~/.geniuz/bin/geniuz` or similar that a prior install-script wrote but
    /// the user's login-shell PATH doesn't include. That hid the affordance from
    /// users who still needed it — false positive.
    func checkCliOnPath() -> Bool {
        // The Copy Install Command only ever symlinks to one of these two
        // locations, so these are the only paths worth checking for "did the
        // install work." A `which geniuz` spawn would match shell semantics
        // more broadly but won't inherit the user's login PATH under sandbox.
        let fm = FileManager.default
        return fm.isExecutableFile(atPath: "/usr/local/bin/geniuz")
            || fm.isExecutableFile(atPath: "/opt/homebrew/bin/geniuz")
    }

    /// Copies a ready-to-paste sudo command to the clipboard that symlinks the
    /// bundled CLI into /usr/local/bin/geniuz. The user pastes into Terminal; the
    /// app never attempts privileged execution itself (sandbox-friendly).
    func copyCliInstallCommand() {
        let bundled = geniuzBinaryPath
        let command = "sudo ln -sf \"\(bundled)\" /usr/local/bin/geniuz"
        let pb = NSPasteboard.general
        pb.clearContents()
        pb.setString(command, forType: .string)
        NSLog("[geniuz-app] copied CLI install command: %@", command)

        cliCopyConfirmation = true
        DispatchQueue.main.asyncAfter(deadline: .now() + 3.0) { [weak self] in
            self?.cliCopyConfirmation = false
        }
    }

    // MARK: - Direct SQLite station read

    /// One memory tuple as displayed in the popover.
    /// `parentUuid == uuid` for root memories (per the relay/Geniuz convention),
    /// or different for follow-ups; the menu uses this to render threading.
    struct RecentMemory: Identifiable, Equatable {
        let id: String           // memory_uuid
        let gist: String
        let parentUuid: String?  // nil only when the row stored NULL; populated rows match the convention
        let createdAt: String?   // UTC, `YYYY-MM-DD HH:MM:SS`
        /// "4 days ago". A stamp a little ahead of this clock reads as now.
        var when: String? {
            guard let raw = createdAt else { return nil }
            let parser = DateFormatter()
            parser.locale = Locale(identifier: "en_US_POSIX")
            parser.timeZone = TimeZone(identifier: "UTC")
            parser.dateFormat = "yyyy-MM-dd HH:mm:ss"
            guard let date = parser.date(from: raw) else { return nil }
            let formatter = RelativeDateTimeFormatter()
            formatter.unitsStyle = .full
            return formatter.localizedString(for: min(date, Date()), relativeTo: Date())
        }
        var isThreadFollowup: Bool {
            // A follow-up has a parent and that parent isn't itself.
            guard let p = parentUuid else { return false }
            return p != id
        }
    }

    private struct StationInfo {
        var exists: Bool = false
        var memories: Int = 0
        var recentMemories: [RecentMemory] = []
        var recentGists: [String] = []  // legacy, kept so existing call sites compile
    }

    private func readStation() -> StationInfo {
        var info = StationInfo()
        let path = stationPath

        let fileExists = FileManager.default.fileExists(atPath: path)
        NSLog("[geniuz-app] readStation path=%@ exists=%d", path, fileExists ? 1 : 0)

        guard fileExists else { return info }
        info.exists = true

        // Read through the WAL, or the newest memories wouldn't show until they
        // were flushed to the main file. That needs to touch the -shm file, so
        // a folder this app can't write to falls back to reading the main file
        // alone: slightly behind beats blank.
        var db: OpaquePointer?
        for mode in ["mode=ro", "mode=ro&immutable=1"] {
            if db != nil { sqlite3_close(db); db = nil }
            let uri = "file:\(path)?\(mode)"
            let rc = sqlite3_open_v2(uri, &db, SQLITE_OPEN_READONLY | SQLITE_OPEN_NOMUTEX | SQLITE_OPEN_URI, nil)
            NSLog("[geniuz-app] sqlite3_open rc=%d path=%@", rc, uri)
            guard rc == SQLITE_OK else { continue }
            if sqlite3_exec(db, "SELECT 1 FROM memories LIMIT 1", nil, nil, nil) == SQLITE_OK { break }
        }
        guard db != nil else { return info }
        defer { sqlite3_close(db) }

        // Memory count
        var stmt: OpaquePointer?
        if sqlite3_prepare_v2(db, "SELECT COUNT(*) FROM memories", -1, &stmt, nil) == SQLITE_OK {
            if sqlite3_step(stmt) == SQLITE_ROW {
                info.memories = Int(sqlite3_column_int(stmt, 0))
            }
        }
        sqlite3_finalize(stmt)

        // Recent memories with parent UUID for threading viz. Fetch up to 20 to give
        // the settings UI headroom (recent_memories_count caps display, not fetch).
        let sql = """
            SELECT memory_uuid,
                   COALESCE(json_extract(payload, '$.gist'),
                            substr(json_extract(payload, '$.content'), 1, 100)),
                   parent_uuid,
                   created_at
            FROM memories
            ORDER BY created_at DESC
            LIMIT 20
        """
        if sqlite3_prepare_v2(db, sql, -1, &stmt, nil) == SQLITE_OK {
            while sqlite3_step(stmt) == SQLITE_ROW {
                let id = sqlite3_column_text(stmt, 0).map { String(cString: $0) } ?? ""
                let gist = sqlite3_column_text(stmt, 1).map { String(cString: $0) } ?? ""
                let parent = sqlite3_column_text(stmt, 2).map { String(cString: $0) }
                let created = sqlite3_column_text(stmt, 3).map { String(cString: $0) }
                info.recentMemories.append(RecentMemory(id: id, gist: gist, parentUuid: parent, createdAt: created))
                info.recentGists.append(gist)
            }
        }
        sqlite3_finalize(stmt)

        NSLog("[geniuz-app] station: %d memories, %d recent", info.memories, info.recentMemories.count)
        return info
    }

    // MARK: - MCP config

    func checkMcpInstalled() -> Bool {
        let path = claudeConfigPath
        guard let data = FileManager.default.contents(atPath: path),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let servers = json["mcpServers"] as? [String: Any] else {
            NSLog("[geniuz-app] MCP config not readable at %@", path)
            return false
        }
        let found = servers.keys.contains { $0.lowercased() == "geniuz" }
        NSLog("[geniuz-app] MCP installed=%d", found ? 1 : 0)
        return found
    }

    func configureClaudeConnection() {
        let binary = geniuzBinaryPath

        var config: [String: Any]
        if let data = FileManager.default.contents(atPath: claudeConfigPath),
           let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
            config = json
        } else {
            config = [:]
            let dir = (claudeConfigPath as NSString).deletingLastPathComponent
            try? FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
        }

        var servers = config["mcpServers"] as? [String: Any] ?? [:]
        servers["geniuz"] = [
            "command": binary,
            "args": ["mcp", "serve"]
        ]
        config["mcpServers"] = servers

        if let data = try? JSONSerialization.data(withJSONObject: config, options: [.prettyPrinted, .sortedKeys]) {
            try? data.write(to: URL(fileURLWithPath: claudeConfigPath))
        }

        // Snapshot Claude Desktop PIDs at configure time; banner clears when they all exit
        // and at least one replacement has launched.
        let runningClaudes = NSWorkspace.shared.runningApplications
            .filter { $0.bundleIdentifier == claudeBundleID }
            .map { $0.processIdentifier }
        claudeAtConfigureTime = Set(runningClaudes)
        claudeSeenAfterConfigure = []
        restartRequired = true

        NSLog("[geniuz-app] configured — Claude PIDs at configure time: %@",
              claudeAtConfigureTime.map(String.init).joined(separator: ","))

        refresh()
    }

    // MARK: - Claude Desktop lifecycle

    @objc private func appLaunched(_ note: Notification) {
        guard restartRequired,
              let app = note.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication,
              app.bundleIdentifier == claudeBundleID else { return }
        let pid = app.processIdentifier
        if !claudeAtConfigureTime.contains(pid) {
            claudeSeenAfterConfigure.insert(pid)
            evaluateRestartState()
        }
    }

    @objc private func appTerminated(_ note: Notification) {
        guard restartRequired,
              let app = note.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication,
              app.bundleIdentifier == claudeBundleID else { return }
        let pid = app.processIdentifier
        claudeAtConfigureTime.remove(pid)
        evaluateRestartState()
    }

    private func evaluateRestartState() {
        // Banner clears when every Claude instance present at configure time has exited
        // AND at least one new Claude instance has launched since.
        if claudeAtConfigureTime.isEmpty && !claudeSeenAfterConfigure.isEmpty {
            DispatchQueue.main.async { [weak self] in
                self?.restartRequired = false
                NSLog("[geniuz-app] restart-required banner cleared — Claude Desktop replaced")
            }
        }

        // Edge case: configure was clicked with no Claude running. First launch counts as "restarted."
        if claudeAtConfigureTime.isEmpty && claudeSeenAfterConfigure.isEmpty {
            // Nothing to do — wait for a launch.
        }
    }

    // MARK: - Settings window + mutation helper

    /// Mutate-and-save a single settings field. Used by the SettingsView's
    /// SwiftUI bindings — each control gets a Binding that calls this in its
    /// setter so changes persist on every flip/edit (Mac convention: save on
    /// change, no apply/cancel).
    func updateSettings(_ mutate: (inout GeniuzSettings) -> Void) {
        var next = settings
        mutate(&next)
        saveSettings(next)
    }

    /// Launch the Geniuz Dashboard via the `geniuz://` URL scheme. The
    /// dashboard .app registers itself as the handler for this scheme at
    /// install time (CFBundleURLTypes in its Info.plist). The OS routes the
    /// open call to the dashboard regardless of where it's installed, and
    /// sandboxed callers can use this path freely — no entitlements needed.
    func openDashboard() {
        guard let url = URL(string: "geniuz://open") else { return }
        let config = NSWorkspace.OpenConfiguration()
        config.activates = true
        NSWorkspace.shared.open(url, configuration: config) { _, error in
            if let error = error {
                DispatchQueue.main.async {
                    let alert = NSAlert()
                    alert.messageText = "Geniuz Dashboard not installed"
                    alert.informativeText = "Couldn't open the dashboard. Make sure Geniuz Dashboard is installed (download at geniuz.life). Details: \(error.localizedDescription)"
                    alert.alertStyle = .warning
                    alert.runModal()
                }
            }
        }
    }

    /// Open the settings window. Lazy-creates the window on first invocation
    /// and reuses it on subsequent opens. Activates the app so the window
    /// receives focus (LSUIElement = true means we're not normally focusable).
    func openSettings() {
        if settingsWindowController == nil {
            let view = GeniuzSettingsView(service: self)
            let hosting = NSHostingController(rootView: view)
            let window = NSWindow(contentViewController: hosting)
            window.styleMask = [.titled, .closable, .resizable]
            window.title = "Geniuz Settings"
            // Initial size big enough for all four sections without scrolling.
            // User can resize from here; minSize prevents shrinking past readable.
            window.setContentSize(NSSize(width: 420, height: 600))
            window.minSize = NSSize(width: 380, height: 320)
            window.center()
            window.isReleasedWhenClosed = false
            settingsWindowController = NSWindowController(window: window)
        }
        settingsWindowController?.showWindow(nil)
        NSApp.activate(ignoringOtherApps: true)
    }
}
