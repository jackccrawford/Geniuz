import Foundation
import SQLite3
import Combine

class GeniuzService: ObservableObject {
    @Published var memoryCount: Int = 0
    @Published var embeddingCount: Int = 0
    @Published var lastSignalGist: String? = nil
    @Published var lastSignalDate: String? = nil
    @Published var mcpInstalled: Bool = false
    @Published var stationExists: Bool = false

    private var timer: Timer?

    var stationPath: String {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        return "\(home)/.geniuz/station.db"
    }

    var geniuzBinaryPath: String {
        // Bundled binary inside the .app
        Bundle.main.path(forResource: "geniuz", ofType: nil) ?? "/usr/local/bin/geniuz"
    }

    var claudeConfigPath: String {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        return "\(home)/Library/Application Support/Claude/claude_desktop_config.json"
    }

    init() {
        refresh()
        // Poll every 10 seconds for updates
        timer = Timer.scheduledTimer(withTimeInterval: 10, repeats: true) { [weak self] _ in
            self?.refresh()
        }
    }

    func refresh() {
        DispatchQueue.global(qos: .utility).async { [weak self] in
            guard let self = self else { return }
            let station = self.readStation()
            let mcp = self.checkMcpInstalled()

            DispatchQueue.main.async {
                self.stationExists = station.exists
                self.memoryCount = station.memories
                self.embeddingCount = station.embeddings
                self.lastSignalGist = station.lastGist
                self.lastSignalDate = station.lastDate
                self.mcpInstalled = mcp
            }
        }
    }

    // MARK: - Station reading

    private struct StationInfo {
        var exists: Bool = false
        var memories: Int = 0
        var embeddings: Int = 0
        var lastGist: String? = nil
        var lastDate: String? = nil
    }

    private func readStation() -> StationInfo {
        var info = StationInfo()
        let path = stationPath

        guard FileManager.default.fileExists(atPath: path) else { return info }
        info.exists = true

        var db: OpaquePointer?
        guard sqlite3_open_v2(path, &db, SQLITE_OPEN_READONLY, nil) == SQLITE_OK else { return info }
        defer { sqlite3_close(db) }

        // Memory count
        var stmt: OpaquePointer?
        if sqlite3_prepare_v2(db, "SELECT COUNT(*) FROM signals", -1, &stmt, nil) == SQLITE_OK {
            if sqlite3_step(stmt) == SQLITE_ROW {
                info.memories = Int(sqlite3_column_int(stmt, 0))
            }
        }
        sqlite3_finalize(stmt)

        // Embedding count
        if sqlite3_prepare_v2(db, "SELECT COUNT(*) FROM signal_embeddings", -1, &stmt, nil) == SQLITE_OK {
            if sqlite3_step(stmt) == SQLITE_ROW {
                info.embeddings = Int(sqlite3_column_int(stmt, 0))
            }
        }
        sqlite3_finalize(stmt)

        // Last signal
        if sqlite3_prepare_v2(db, "SELECT gist, created_at FROM signals ORDER BY created_at DESC LIMIT 1", -1, &stmt, nil) == SQLITE_OK {
            if sqlite3_step(stmt) == SQLITE_ROW {
                if let cStr = sqlite3_column_text(stmt, 0) {
                    info.lastGist = String(cString: cStr)
                }
                if let cStr = sqlite3_column_text(stmt, 1) {
                    let full = String(cString: cStr)
                    info.lastDate = String(full.prefix(16))
                }
            }
        }
        sqlite3_finalize(stmt)

        return info
    }

    // MARK: - MCP config

    func checkMcpInstalled() -> Bool {
        guard let data = FileManager.default.contents(atPath: claudeConfigPath),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let servers = json["mcpServers"] as? [String: Any] else {
            return false
        }
        // Check for any geniuz entry (case-insensitive key match)
        return servers.keys.contains { $0.lowercased() == "geniuz" }
    }

    func installMcp() {
        let binary = geniuzBinaryPath

        // Read or create config
        var config: [String: Any]
        if let data = FileManager.default.contents(atPath: claudeConfigPath),
           let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
            config = json
        } else {
            config = [:]
            // Create directory if needed
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

        refresh()
    }

    func uninstallMcp() {
        guard let data = FileManager.default.contents(atPath: claudeConfigPath),
              var json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              var servers = json["mcpServers"] as? [String: Any] else {
            return
        }

        // Remove geniuz entry (case-insensitive)
        let key = servers.keys.first { $0.lowercased() == "geniuz" }
        if let key = key {
            servers.removeValue(forKey: key)
            json["mcpServers"] = servers
            if let data = try? JSONSerialization.data(withJSONObject: json, options: [.prettyPrinted, .sortedKeys]) {
                try? data.write(to: URL(fileURLWithPath: claudeConfigPath))
            }
        }

        refresh()
    }
}
