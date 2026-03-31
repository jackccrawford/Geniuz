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

    /// Real home directory via getpwuid — not remapped by sandbox
    private var realHome: String {
        if let pw = getpwuid(getuid()), let home = pw.pointee.pw_dir {
            return String(cString: home)
        }
        return NSHomeDirectory()
    }

    var stationPath: String {
        return "\(realHome)/.geniuz/station.db"
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
    }

    deinit {
        timer?.invalidate()
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

    // MARK: - Direct SQLite station read

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

        let fileExists = FileManager.default.fileExists(atPath: path)
        NSLog("[geniuz-app] readStation path=%@ exists=%d", path, fileExists ? 1 : 0)

        guard fileExists else { return info }
        info.exists = true

        var db: OpaquePointer?
        // Open as immutable — skips WAL, no write access needed
        let uri = "file:\(path)?mode=ro&immutable=1"
        let rc = sqlite3_open_v2(uri, &db, SQLITE_OPEN_READONLY | SQLITE_OPEN_NOMUTEX | SQLITE_OPEN_URI, nil)
        NSLog("[geniuz-app] sqlite3_open rc=%d path=%@", rc, uri)
        guard rc == SQLITE_OK else { return info }
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

        // Last signal — gist is inside payload JSON
        if sqlite3_prepare_v2(db, "SELECT COALESCE(json_extract(payload, '$.gist'), substr(json_extract(payload, '$.content'), 1, 100)), created_at FROM signals ORDER BY created_at DESC LIMIT 1", -1, &stmt, nil) == SQLITE_OK {
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

        NSLog("[geniuz-app] station: %d memories, %d embeddings, lastGist=%@", info.memories, info.embeddings, info.lastGist ?? "nil")
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

    func installMcp() {
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

        refresh()
    }

    func uninstallMcp() {
        guard let data = FileManager.default.contents(atPath: claudeConfigPath),
              var json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              var servers = json["mcpServers"] as? [String: Any] else {
            return
        }

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
