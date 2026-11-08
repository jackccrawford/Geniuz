import Foundation
import SQLite3
import Combine
import AppKit
import SwiftUI
import ServiceManagement

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
    let pinned: Bool?
    let lastWritten: String?
    var id: String { uuid }

    enum CodingKeys: String, CodingKey {
        case uuid, name, path, memories, pinned
        case inUse = "in_use"
        case lastWritten = "last_written"
    }
}

/// With many folders, a menu of all of them is a wall. Show the one in use,
/// the pinned ones and those written in the last week (newest first, up to
/// ten); the dashboard has the rest.
/// Geniuz stores times as UTC "yyyy-MM-dd HH:mm:ss". A fixed-format parser
/// needs the POSIX locale, or a 12-hour or non-Gregorian setting breaks it.
private let storedTime: DateFormatter = {
    let fmt = DateFormatter()
    fmt.locale = Locale(identifier: "en_US_POSIX")
    fmt.calendar = Calendar(identifier: .gregorian)
    fmt.timeZone = TimeZone(identifier: "UTC")
    fmt.dateFormat = "yyyy-MM-dd HH:mm:ss"
    return fmt
}()

func shortlist(_ folders: [FolderOption], showing: String? = nil, over limit: Int = 20) -> [FolderOption] {
    guard folders.count > limit else { return folders }
    let weekAgo = Date().addingTimeInterval(-7 * 86400)
    let recent = folders
        .filter { f in f.lastWritten.flatMap { storedTime.date(from: String($0.prefix(19)).replacingOccurrences(of: "T", with: " ")) }.map { $0 > weekAgo } ?? false }
        .sorted { ($0.lastWritten ?? "") > ($1.lastWritten ?? "") }
        .prefix(10)
    var seen = Set<String>()
    return (folders.filter { $0.uuid == showing } + folders.filter { $0.inUse } + folders.filter { $0.pinned == true } + recent)
        .filter { seen.insert($0.uuid).inserted }
}

private struct FolderListing: Decodable {
    let folders: [FolderOption]
}

// MARK: - Updates
//
// "Update available" (Jack, 7 Nov: "it's how good apps work"). The app asks
// GitHub for the latest published release (drafts and pre-releases are never
// "latest") and compares versions. It downloads nothing and installs nothing:
// Download opens the disk image in the browser. Installing in place (Sparkle)
// needs an update-signing key, which is Jack's decision.

let releasesAPI = URL(string: "https://api.github.com/repos/jackccrawford/Geniuz/releases/latest")!
let releasesPage = URL(string: "https://github.com/jackccrawford/Geniuz/releases/latest")!

struct AvailableUpdate: Equatable {
    let version: String
    let url: URL
}

/// "v5.0.10" > "5.0.9": compare numerically, part by part; a missing part is 0.
/// Anything after a "-" (a pre-release tag) is ignored, and a malformed part counts as 0.
func isNewer(_ candidate: String, than current: String) -> Bool {
    func parts(_ v: String) -> [Int] {
        let trimmed: String = v.trimmingCharacters(in: .whitespaces)
        let bare: Substring = trimmed.drop(while: { $0 == "v" || $0 == "V" })
        let core: Substring = bare.split(separator: "-", maxSplits: 1).first ?? ""
        return core.split(separator: ".").map { Int($0) ?? 0 }
    }
    let a = parts(candidate), b = parts(current)
    for i in 0..<max(a.count, b.count) {
        let x = i < a.count ? a[i] : 0, y = i < b.count ? b[i] : 0
        if x != y { return x > y }
    }
    return false
}

private struct GitHubRelease: Decodable {
    struct Asset: Decodable {
        let name: String
        let browserDownloadURL: URL
        enum CodingKeys: String, CodingKey {
            case name
            case browserDownloadURL = "browser_download_url"
        }
    }
    let tagName: String
    let htmlURL: URL
    let draft: Bool
    let prerelease: Bool
    let assets: [Asset]
    enum CodingKeys: String, CodingKey {
        case tagName = "tag_name"
        case htmlURL = "html_url"
        case draft, prerelease, assets
    }
}

class GeniuzService: ObservableObject {
    @Published var folders: [FolderOption] = []
    @Published var memoryCount: Int = 0
    @Published var recentGists: [String] = []
    @Published var recentMemories: [RecentMemory] = []
    @Published var mcpInstalled: Bool = false
    @Published var claudeFound: Bool = true
    @Published var claudeConfigUnreadable: Bool = false
    @Published var claudeCodeConnected: Bool = false
    @Published var claudeCodeFailed: Bool = false
    @Published var stationExists: Bool = false
    @Published var recentExpanded: Bool = false
    @Published var restartRequired: Bool = false
    @Published var cliOnPath: Bool = false
    @Published var cliInstallFailed: Bool = false
    @Published var settings: GeniuzSettings = GeniuzSettings()
    @Published var availableUpdate: AvailableUpdate?
    /// The result of the last check people asked for, shown under the button.
    @Published var updateCheckMessage: String?
    @Published var checkingForUpdate = false

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
    private var verifiedThisLaunch = false
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

    /// The folder this menu shows: the one picked here, else the default folder,
    /// else the first. Picking here only changes what the menu shows (Jack, 7 Nov
    /// 2026: looking is not choosing where memories go); the default folder is a
    /// setting. The binary owns the rules for finding folders, so this never has a
    /// second copy of them.
    var stationPath: String { stationPath(shown: shownFolderID) }

    /// The same, for a folder picked in the menu read on the main thread beforehand
    /// (refresh runs its reads on a background queue).
    func stationPath(shown: String?) -> String {
        if let id = shown, let f = cachedFolders.first(where: { $0.uuid == id }) { return f.path }
        return cachedFolders.first(where: { $0.inUse })?.path ?? defaultStationPath
    }

    /// The folder picked in this menu, if any (kept for this run of the app).
    @Published var shownFolderID: String?

    /// The default folder setting, as Geniuz stores it: a folder's UUID, "none"
    /// (a command that names no folder must ask for one), or nil for never set
    /// (the first folder).
    @Published var defaultFolderSetting: String?

    private func readDefaultSetting() -> String? {
        guard let raw = try? String(contentsOfFile: "\(dataDir)/current", encoding: .utf8) else { return nil }
        let v = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        return v.isEmpty ? nil : v
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
        env.removeValue(forKey: "GENIUZ_DB")
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

    /// Show this folder in the menu. Changes nothing anywhere else.
    func showFolder(_ folder: FolderOption) {
        shownFolderID = folder.uuid
        refresh()
    }

    /// The default folder setting: a folder's UUID, or "none" to ask instead.
    func setDefaultFolder(_ value: String) {
        work.async { [weak self] in
            guard let self = self else { return }
            _ = self.runGeniuz(value == "none" ? ["folder", "use", "--none"] : ["folder", "use", value])
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

    /// macOS runs a quarantined app from a random path that disappears when it quits,
    /// and a disk image disappears when it's ejected. Nothing lasting may point there.
    var runningFromTemporaryLocation: Bool {
        let path = Bundle.main.bundlePath
        return path.contains("/AppTranslocation/") || path.hasPrefix("/Volumes/")
    }

    var claudeConfigPath: String {
        return "\(realHome)/Library/Application Support/Claude/claude_desktop_config.json"
    }

    init() {
        NSLog("[geniuz-app] init — realHome=%@ stationPath=%@", realHome, stationPath)
        availableUpdate = readUpdateFile()
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

    /// Where the menu's background reads run, one at a time.
    private let work = DispatchQueue(label: "geniuz.menu.work", qos: .utility)

    func refresh() {
        // Read on the main thread, where it's written; the reads below run elsewhere.
        let shown = Thread.isMainThread ? shownFolderID : nil
        guard Thread.isMainThread else {
            DispatchQueue.main.async { [weak self] in self?.refresh() }
            return
        }
        // One serial queue for all of this: the folder cache and its stamps are only
        // ever touched there (Firn's review, PR #30: concurrent refreshes raced).
        work.async { [weak self] in
            guard let self = self else { return }
            let folders = self.readFolders()
            let station = self.readStation(shown: shown)
            // The full check runs the registered program, so once per launch is enough.
            let entry = self.readClaudeEntry(runIt: !self.verifiedThisLaunch)
            self.verifiedThisLaunch = true
            let claudeCode = self.checkClaudeCodeConnected()
            let onPath = self.checkCliOnPath()
            let s = self.loadSettings()
            let defaultSetting = self.readDefaultSetting()

            DispatchQueue.main.async {
                self.folders = folders
                // A refresh for the folder shown before a newer pick lands late: keep its
                // folder list, drop its memories (Firn's review, PR #30).
                guard shown == self.shownFolderID else { return }
                self.stationExists = station.exists
                self.memoryCount = station.memories
                self.recentMemories = station.recentMemories
                self.recentGists = station.recentGists
                self.settleClaudeConnection(entry)
                self.claudeCodeConnected = claudeCode
                self.cliOnPath = onPath
                self.settings = s
                self.defaultFolderSetting = defaultSetting
                self.onStateChange?()
                self.checkForUpdateIfDue()
            }
        }
    }

    // MARK: - Update check

    private let lastUpdateCheckKey = "geniuz.lastUpdateCheck"
    /// After a failed check: try again at this time, whatever the cadence.
    private let updateRetryKey = "geniuz.updateRetryAt"

    var appVersion: String {
        Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "0.0.0"
    }

    /// Where the dashboard reads the last check's answer, so both surfaces agree
    /// without the dashboard asking GitHub too.
    var updateFilePath: String { "\(dataDir)/update.json" }

    /// Runs on the 10-second refresh; does nothing until a check is due. Daily or
    /// weekly as settings say; "manual", or updates turned off, means never.
    private func checkForUpdateIfDue() {
        guard settings.autoupdateEnabled, !checkingForUpdate else { return }
        let interval: TimeInterval
        switch settings.updateCheckFrequency {
        case "daily": interval = 86_400
        case "weekly": interval = 7 * 86_400
        default: return
        }
        let now = Date()
        if let retry = UserDefaults.standard.object(forKey: updateRetryKey) as? Date,
           retry > now, retry.timeIntervalSince(now) <= 3600 {
            return  // a failed check waits its hour (a clock set back can't stretch it)
        }
        let last = UserDefaults.standard.object(forKey: lastUpdateCheckKey) as? Date ?? .distantPast
        // A clock set back must not stop checks for good.
        guard now.timeIntervalSince(last) >= interval || last > now else { return }
        checkForUpdate(userAsked: false)
    }

    /// Ask GitHub for the latest release. `userAsked` reports the outcome in
    /// Settings, including "up to date" and failures; a background check stays quiet.
    func checkForUpdate(userAsked: Bool) {
        guard !checkingForUpdate else { return }
        checkingForUpdate = true
        if userAsked { updateCheckMessage = "Checking…" }
        var request = URLRequest(url: releasesAPI, cachePolicy: .reloadIgnoringLocalCacheData, timeoutInterval: 15)
        request.setValue("application/vnd.github+json", forHTTPHeaderField: "Accept")
        request.setValue("Geniuz/\(appVersion)", forHTTPHeaderField: "User-Agent")
        URLSession.shared.dataTask(with: request) { [weak self] data, response, error in
            guard let self = self else { return }
            let status = (response as? HTTPURLResponse)?.statusCode ?? 0
            let body = status == 200 ? data : nil
            DispatchQueue.main.async {
                // Decoded here, on the main thread, where the types live (a few KB).
                let release = body.flatMap { try? JSONDecoder().decode(GitHubRelease.self, from: $0) }
                self.checkingForUpdate = false
                guard let release = release, !release.draft, !release.prerelease else {
                    // A failed check doesn't count as a check: try again in an hour,
                    // not every 10 seconds and not a whole day later.
                    UserDefaults.standard.set(Date().addingTimeInterval(3600), forKey: self.updateRetryKey)
                    if userAsked {
                        self.updateCheckMessage = status == 0
                            ? "Couldn't reach GitHub to check. Try again later."
                            : "GitHub didn't answer the check (HTTP \(status)). Try again later."
                    }
                    NSLog("[geniuz-app] update check failed: status=%d %@", status, error?.localizedDescription ?? "")
                    return
                }
                UserDefaults.standard.set(Date(), forKey: self.lastUpdateCheckKey)
                UserDefaults.standard.removeObject(forKey: self.updateRetryKey)
                let latest = String(release.tagName.drop(while: { $0 == "v" || $0 == "V" }))
                let dmg = release.assets.first(where: { $0.name == "Geniuz.dmg" })?.browserDownloadURL
                if isNewer(latest, than: self.appVersion) {
                    self.availableUpdate = AvailableUpdate(version: latest, url: dmg ?? release.htmlURL)
                    if userAsked { self.updateCheckMessage = "Geniuz \(latest) is available." }
                } else {
                    self.availableUpdate = nil
                    if userAsked { self.updateCheckMessage = "Geniuz \(self.appVersion) is up to date." }
                }
                self.writeUpdateFile()
            }
        }.resume()
    }

    /// {"latest": "5.0.4", "url": "...", "checked_at": "..."} or {"latest": null, ...}.
    private func writeUpdateFile() {
        var out: [String: Any] = ["checked_at": ISO8601DateFormatter().string(from: Date())]
        out["latest"] = availableUpdate?.version ?? NSNull()
        out["url"] = availableUpdate?.url.absoluteString ?? NSNull()
        guard let data = try? JSONSerialization.data(withJSONObject: out, options: [.sortedKeys]) else { return }
        try? FileManager.default.createDirectory(atPath: dataDir, withIntermediateDirectories: true)
        try? data.write(to: URL(fileURLWithPath: updateFilePath), options: .atomic)
    }

    /// The last check's answer, so a found update survives a restart. Only a
    /// release newer than this app, and only a link to the project's releases.
    private func readUpdateFile() -> AvailableUpdate? {
        guard let data = FileManager.default.contents(atPath: updateFilePath),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let latest = obj["latest"] as? String,
              let link = obj["url"] as? String,
              link.hasPrefix("https://github.com/jackccrawford/Geniuz/releases/"),
              let url = URL(string: link),
              isNewer(latest, than: appVersion) else { return nil }
        return AvailableUpdate(version: latest, url: url)
    }

    func openUpdate() {
        NSWorkspace.shared.open(availableUpdate?.url ?? releasesPage)
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
            || fm.isExecutableFile(atPath: "\(realHome)/.local/bin/geniuz")
    }

    /// A folder on the person's PATH that's theirs needs no password. Otherwise macOS
    /// asks for one, in its own dialog. No Terminal either way.
    func installCommand() {
        guard !runningFromTemporaryLocation else { return }
        let bundled = geniuzBinaryPath
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self = self else { return }
            let ok = self.linkIntoUserPath(bundled) || self.linkWithAdminPrompt(bundled)
            DispatchQueue.main.async {
                self.cliInstallFailed = !ok
                self.cliOnPath = self.checkCliOnPath()
            }
        }
    }

    private func linkIntoUserPath(_ bundled: String) -> Bool {
        let fm = FileManager.default
        let path = loginShellPath()
        for dir in ["\(realHome)/.local/bin", "/opt/homebrew/bin"] {
            guard fm.isWritableFile(atPath: dir), path?.contains(dir) ?? true else { continue }
            let link = "\(dir)/geniuz"
            // Replace an old link, but never a real file that isn't ours.
            if (try? fm.destinationOfSymbolicLink(atPath: link)) != nil {
                try? fm.removeItem(atPath: link)
            } else if fm.fileExists(atPath: link) {
                continue
            }
            if (try? fm.createSymbolicLink(atPath: link, withDestinationPath: bundled)) != nil {
                return true
            }
        }
        return false
    }

    private func linkWithAdminPrompt(_ bundled: String) -> Bool {
        let command = "mkdir -p /usr/local/bin && ln -sf '\(bundled)' /usr/local/bin/geniuz"
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
        process.arguments = ["-e", "do shell script \"\(command)\" with administrator privileges"]
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        guard (try? process.run()) != nil else { return false }
        process.waitUntilExit()
        return process.terminationStatus == 0
    }

    /// GUI apps don't read shell profiles, so ask the shell what PATH a terminal would have.
    private func loginShellPath() -> [String]? {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/zsh")
        process.arguments = ["-ilc", "print -r -- $PATH"]
        let out = Pipe()
        process.standardOutput = out
        process.standardError = FileHandle.nullDevice
        process.standardInput = FileHandle.nullDevice
        guard (try? process.run()) != nil else { return nil }
        let timeout = DispatchWorkItem { process.terminate() }
        DispatchQueue.global().asyncAfter(deadline: .now() + 3, execute: timeout)
        let data = out.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        timeout.cancel()
        guard process.terminationStatus == 0, let text = String(data: data, encoding: .utf8) else { return nil }
        let lines = text.split(separator: "\n")
        return lines.last.map { $0.split(separator: ":").map(String.init) }
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

    private func readStation(shown: String?) -> StationInfo {
        var info = StationInfo()
        let path = stationPath(shown: shown)

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
            ORDER BY created_at DESC, rowid DESC
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

    private enum ClaudeEntry {
        case absent, broken, healthy, unreadable
    }

    private var claudeIsPresent: Bool {
        NSWorkspace.shared.urlForApplication(withBundleIdentifier: claudeBundleID) != nil
            || FileManager.default.fileExists(atPath: (claudeConfigPath as NSString).deletingLastPathComponent)
    }

    /// Set once Geniuz has connected to Claude Desktop. If the entry is gone after that,
    /// the person removed it, and it stays removed until they ask again.
    private var connectedOnceMarker: String { "\(dataDir)/claude-connected" }
    private var welcomedMarker: String { "\(dataDir)/welcomed" }

    private func writeMarker(_ path: String) {
        try? FileManager.default.createDirectory(atPath: dataDir, withIntermediateDirectories: true)
        FileManager.default.createFile(atPath: path, contents: Data())
    }

    private func answers(_ command: String) -> Bool {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: command)
        process.arguments = ["--version"]
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        do {
            try process.run()
            process.waitUntilExit()
            return process.terminationStatus == 0
        } catch {
            return false
        }
    }

    /// An entry isn't a connection: Claude can't start a program that's gone.
    private func readClaudeEntry(runIt: Bool) -> ClaudeEntry {
        guard let data = FileManager.default.contents(atPath: claudeConfigPath) else { return .absent }
        guard let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return .unreadable }
        let servers = json["mcpServers"] as? [String: Any] ?? [:]
        guard let entry = servers.first(where: { $0.key.lowercased() == "geniuz" })?.value as? [String: Any] else {
            return .absent
        }
        let command = entry["command"] as? String ?? ""
        guard command.hasPrefix("/") else { return .healthy }
        guard FileManager.default.isExecutableFile(atPath: command) else { return .broken }
        return !runIt || answers(command) ? .healthy : .broken
    }

    /// Connect without asking, repair without asking, and leave alone anything the
    /// person removed themselves. Runs on the main thread after each refresh.
    private func settleClaudeConnection(_ entry: ClaudeEntry) {
        claudeFound = claudeIsPresent
        claudeConfigUnreadable = false
        guard !runningFromTemporaryLocation else { return }
        let fm = FileManager.default
        switch entry {
        case .healthy:
            mcpInstalled = true
            if !fm.fileExists(atPath: connectedOnceMarker) { writeMarker(connectedOnceMarker) }
        case .broken:
            mcpInstalled = false
            connectClaude(thenOfferRestart: true)
        case .absent:
            mcpInstalled = false
            if claudeFound && !fm.fileExists(atPath: connectedOnceMarker) {
                connectClaude(thenOfferRestart: true)
            }
        case .unreadable:
            mcpInstalled = false
            claudeConfigUnreadable = true
        }
    }

    func configureClaudeConnection() {
        connectClaude(thenOfferRestart: false)
    }

    private func connectClaude(thenOfferRestart: Bool) {
        guard !runningFromTemporaryLocation else { return }

        var config: [String: Any] = [:]
        if let data = FileManager.default.contents(atPath: claudeConfigPath) {
            // Never replace settings we can't read: they're Claude's, and may be someone's work.
            guard let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
                claudeConfigUnreadable = true
                return
            }
            config = json
        } else {
            let dir = (claudeConfigPath as NSString).deletingLastPathComponent
            try? FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
        }

        var servers = config["mcpServers"] as? [String: Any] ?? [:]
        // One entry, whatever older versions or the command line called it. Keep its env.
        var entry: [String: Any] = [:]
        for key in servers.keys where key.lowercased() == "geniuz" {
            if let old = servers[key] as? [String: Any] { entry = old }
            servers.removeValue(forKey: key)
        }
        entry["command"] = geniuzBinaryPath
        entry["args"] = ["mcp", "serve"]
        servers["Geniuz"] = entry
        config["mcpServers"] = servers

        guard let data = try? JSONSerialization.data(withJSONObject: config, options: [.prettyPrinted, .sortedKeys]),
              (try? data.write(to: URL(fileURLWithPath: claudeConfigPath), options: .atomic)) != nil else {
            NSLog("[geniuz-app] couldn't write the Claude Desktop config")
            return
        }
        writeMarker(connectedOnceMarker)
        mcpInstalled = true

        // Snapshot Claude Desktop PIDs at configure time; banner clears when they all exit
        // and at least one replacement has launched. If Claude isn't open, it'll load Geniuz when it opens.
        let runningClaudes = NSWorkspace.shared.runningApplications
            .filter { $0.bundleIdentifier == claudeBundleID }
            .map { $0.processIdentifier }
        claudeAtConfigureTime = Set(runningClaudes)
        claudeSeenAfterConfigure = []
        restartRequired = !runningClaudes.isEmpty

        NSLog("[geniuz-app] configured — Claude PIDs at configure time: %@",
              claudeAtConfigureTime.map(String.init).joined(separator: ","))

        if thenOfferRestart && restartRequired {
            DispatchQueue.main.asyncAfter(deadline: .now() + 1.5) { [weak self] in self?.offerClaudeRestart() }
        }
    }

    private func offerClaudeRestart() {
        guard restartRequired else { return }
        NSApp.activate(ignoringOtherApps: true)
        let alert = NSAlert()
        alert.messageText = "Geniuz is connected to Claude Desktop"
        alert.informativeText = "Restart Claude once to start using it."
        alert.addButton(withTitle: "Restart Claude")
        alert.addButton(withTitle: "Later")
        if alert.runModal() == .alertFirstButtonReturn {
            restartClaude()
        }
    }

    func restartClaude() {
        let running = NSWorkspace.shared.runningApplications.filter { $0.bundleIdentifier == claudeBundleID }
        guard let url = running.first?.bundleURL
                ?? NSWorkspace.shared.urlForApplication(withBundleIdentifier: claudeBundleID) else { return }
        running.forEach { $0.terminate() }
        reopen(url, attempts: 40)
    }

    /// Claude may ask to save or take a moment to quit, so wait for it rather than guess.
    private func reopen(_ url: URL, attempts: Int) {
        let stillOpen = NSWorkspace.shared.runningApplications.contains { $0.bundleIdentifier == claudeBundleID }
        if stillOpen {
            guard attempts > 0 else { return }
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [weak self] in
                self?.reopen(url, attempts: attempts - 1)
            }
            return
        }
        NSWorkspace.shared.openApplication(at: url, configuration: NSWorkspace.OpenConfiguration()) { _, _ in }
    }

    /// A menubar app shows nothing when it opens, so the first launch opens the dashboard.
    func welcomeOnFirstLaunch() {
        guard !runningFromTemporaryLocation else { return }
        guard !FileManager.default.fileExists(atPath: welcomedMarker) else {
            // Say what's true: someone may have turned this off in System Settings.
            let enabled = SMAppService.mainApp.status == .enabled
            if settings.launchAtLogin != enabled {
                var next = settings
                next.launchAtLogin = enabled
                saveSettings(next)
            }
            return
        }
        writeMarker(welcomedMarker)
        applyLaunchAtLogin(settings.launchAtLogin)
        openDashboard()
    }

    private func applyLaunchAtLogin(_ on: Bool) {
        do {
            if on {
                if SMAppService.mainApp.status != .enabled { try SMAppService.mainApp.register() }
            } else if SMAppService.mainApp.status == .enabled {
                try SMAppService.mainApp.unregister()
            }
        } catch {
            NSLog("[geniuz-app] launch at login: %@", error.localizedDescription)
        }
    }

    // MARK: - Claude Code

    var claudeCodePresent: Bool {
        FileManager.default.fileExists(atPath: "\(realHome)/.claude")
    }

    func checkClaudeCodeConnected() -> Bool {
        guard let data = FileManager.default.contents(atPath: "\(realHome)/.claude.json"),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let servers = json["mcpServers"] as? [String: Any] else { return false }
        return servers.keys.contains { $0.lowercased() == "geniuz" }
    }

    /// Claude Code installs itself outside the PATH a GUI app gets.
    private var claudeCodeBinary: String? {
        let fm = FileManager.default
        return ["\(realHome)/.local/bin/claude", "\(realHome)/.claude/local/claude",
                "/opt/homebrew/bin/claude", "/usr/local/bin/claude"]
            .first { fm.isExecutableFile(atPath: $0) }
    }

    /// Only when asked: this reaches every Claude Code session on the computer.
    func connectClaudeCode() {
        let binary = geniuzBinaryPath
        guard let claude = claudeCodeBinary else {
            NSLog("[geniuz-app] Claude Code: no claude program in the usual places")
            claudeCodeFailed = true
            return
        }
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            let process = Process()
            process.executableURL = URL(fileURLWithPath: claude)
            process.arguments = ["mcp", "add", "--scope", "user", "Geniuz", "--", binary, "mcp", "serve"]
            process.standardOutput = FileHandle.nullDevice
            let err = Pipe()
            process.standardError = err
            let ran = (try? process.run()) != nil
            let detail = ran ? err.fileHandleForReading.readDataToEndOfFile() : Data()
            if ran { process.waitUntilExit() }
            let ok = ran && process.terminationStatus == 0
            if !ok { NSLog("[geniuz-app] Claude Code connect failed: %@", String(decoding: detail, as: UTF8.self)) }
            DispatchQueue.main.async {
                self?.claudeCodeConnected = self?.checkClaudeCodeConnected() ?? false
                self?.claudeCodeFailed = !ok
            }
        }
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
        if next.launchAtLogin != settings.launchAtLogin || next.launchAtLogin {
            applyLaunchAtLogin(next.launchAtLogin)
        }
    }

    /// Launch the dashboard nested inside this app. Asking by URL scheme could reach a
    /// stale registration, since macOS remembers every temporary location an app ran from.
    func openDashboard() {
        guard let nested = Bundle.main.url(forResource: "Geniuz", withExtension: "app") else {
            showDashboardError("This copy of Geniuz is missing its dashboard. Download it again from geniuz.life.")
            return
        }
        let config = NSWorkspace.OpenConfiguration()
        config.activates = true
        NSWorkspace.shared.openApplication(at: nested, configuration: config) { [weak self] _, error in
            if let error = error {
                self?.showDashboardError("Try again in a moment. Details: \(error.localizedDescription)")
            }
        }
    }

    private func showDashboardError(_ detail: String) {
        DispatchQueue.main.async {
            let alert = NSAlert()
            alert.messageText = "Couldn't open the dashboard"
            alert.informativeText = detail
            alert.alertStyle = .warning
            alert.runModal()
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


// MARK: - Move to Applications

/// macOS runs a downloaded app that hasn't been dragged to Applications from a random
/// path that disappears when it quits. Offer to fix that before anything points there.
enum ApplicationsMover {
    private static let destination = "/Applications/Geniuz.app"

    static var isNeeded: Bool {
        let path = Bundle.main.bundlePath
        return path.contains("/AppTranslocation/") || path.hasPrefix("/Volumes/")
    }

    static func offer() {
        NSApp.activate(ignoringOtherApps: true)
        let alert = NSAlert()
        alert.messageText = "Move Geniuz to Applications?"
        alert.informativeText = "Geniuz needs to be in Applications to stay connected to Claude."
        alert.addButton(withTitle: "Move to Applications")
        alert.addButton(withTitle: "Quit")
        guard alert.runModal() == .alertFirstButtonReturn else {
            NSApp.terminate(nil)
            return
        }
        do {
            try move()
            run("/usr/bin/open", ["-n", destination])
        } catch {
            let failed = NSAlert()
            failed.messageText = "Couldn't move Geniuz"
            failed.informativeText = "Drag Geniuz to Applications in Finder, then open it from there. Details: \(error.localizedDescription)"
            failed.runModal()
        }
        NSApp.terminate(nil)
    }

    private static func move() throws {
        let fm = FileManager.default
        if fm.fileExists(atPath: destination) {
            // An older copy goes to the Trash, so it can still be recovered. Memories aren't in it.
            let me = NSRunningApplication.current.processIdentifier
            NSWorkspace.shared.runningApplications
                .filter { $0.bundleIdentifier == Bundle.main.bundleIdentifier && $0.processIdentifier != me }
                .forEach { $0.terminate() }
            try fm.trashItem(at: URL(fileURLWithPath: destination), resultingItemURL: nil)
        }
        guard run("/usr/bin/ditto", [Bundle.main.bundlePath, destination]) == 0 else {
            throw CocoaError(.fileWriteNoPermission)
        }
        run("/usr/bin/xattr", ["-dr", "com.apple.quarantine", destination])
    }

    @discardableResult
    private static func run(_ tool: String, _ arguments: [String]) -> Int32 {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: tool)
        process.arguments = arguments
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        guard (try? process.run()) != nil else { return -1 }
        process.waitUntilExit()
        return process.terminationStatus
    }
}
