import Foundation
import Combine

class GeniuzService: ObservableObject {
    @Published var memoryCount: Int = 0
    @Published var embeddingCount: Int = 0
    @Published var lastSignalGist: String? = nil
    @Published var lastSignalDate: String? = nil
    @Published var mcpInstalled: Bool = false
    @Published var stationExists: Bool = false

    private var timer: Timer?

    private var geniuzBinary: String {
        Bundle.main.path(forResource: "geniuz", ofType: nil) ?? "/usr/local/bin/geniuz"
    }

    private var realHome: String {
        if let pw = getpwuid(getuid()), let home = pw.pointee.pw_dir {
            return String(cString: home)
        }
        return NSHomeDirectory()
    }

    private var claudeConfigPath: String {
        return "\(realHome)/Library/Application Support/Claude/claude_desktop_config.json"
    }

    init() {
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

            // Use CLI for station data — bypasses sandbox
            let status = self.runCli(["status"])
            let recent = self.runCli(["tune", "--recent", "-l", "1", "--json"])
            let mcp = self.checkMcpInstalled()

            // Parse status output: "Memories: 6\nEmbeddings: 6/6 cached"
            var memories = 0
            var embeddings = 0
            var exists = false
            for line in status.components(separatedBy: "\n") {
                if line.hasPrefix("Memories:") {
                    memories = Int(line.replacingOccurrences(of: "Memories: ", with: "").trimmingCharacters(in: .whitespaces)) ?? 0
                    exists = true
                } else if line.hasPrefix("Embeddings:") {
                    let parts = line.replacingOccurrences(of: "Embeddings: ", with: "")
                    if let slash = parts.firstIndex(of: "/") {
                        embeddings = Int(parts[parts.startIndex..<slash]) ?? 0
                    }
                }
            }

            // Parse recent JSON for last signal
            var lastGist: String? = nil
            var lastDate: String? = nil
            if let data = recent.data(using: .utf8),
               let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
               let signals = json["signals"] as? [[String: Any]],
               let first = signals.first {
                lastGist = first["gist"] as? String
                if let date = first["created_at"] as? String {
                    lastDate = String(date.prefix(16))
                }
            }

            DispatchQueue.main.async {
                self.stationExists = exists
                self.memoryCount = memories
                self.embeddingCount = embeddings
                self.lastSignalGist = lastGist
                self.lastSignalDate = lastDate
                self.mcpInstalled = mcp
            }
        }
    }

    // MARK: - CLI subprocess

    private func runCli(_ args: [String]) -> String {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: geniuzBinary)
        process.arguments = args

        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = FileHandle.nullDevice

        do {
            try process.run()
            process.waitUntilExit()
            let data = pipe.fileHandleForReading.readDataToEndOfFile()
            return String(data: data, encoding: .utf8) ?? ""
        } catch {
            return ""
        }
    }

    // MARK: - MCP config

    func checkMcpInstalled() -> Bool {
        guard let data = FileManager.default.contents(atPath: claudeConfigPath),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let servers = json["mcpServers"] as? [String: Any] else {
            return false
        }
        return servers.keys.contains { $0.lowercased() == "geniuz" }
    }

    func installMcp() {
        let binary = geniuzBinary

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
