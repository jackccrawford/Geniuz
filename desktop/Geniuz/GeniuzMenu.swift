import SwiftUI

struct GeniuzMenu: View {
    @ObservedObject var service: GeniuzService

    var body: some View {
        // Status is quiet in the header; the one action people come here for,
        // Open Dashboard, sits on the folder line it opens. The
        // connection message takes a row only when there's something to do.
        VStack(alignment: .leading, spacing: 0) {
            header
            if let update = service.availableUpdate {
                updateRow(update)
            }
            if needsAttention {
                connectionStatus
            }
            Divider()
            folderLine
            Divider()

            if service.stationExists {
                recentMemoriesList
            } else {
                emptyState
            }

            Divider()
            quitRow
        }
        .frame(width: 360)
        .background(.regularMaterial)
    }

    // MARK: - Folders

    /// The folder this menu shows (a picker when there are two or more), and
    /// beside it the way into the dashboard.
    private var folderLine: some View {
        HStack(spacing: 8) {
            folderPicker
                .frame(maxWidth: .infinity, alignment: .leading)
            Button(action: { service.openDashboard() }) {
                Text("Open Dashboard")
                    .font(.system(size: 12, weight: .semibold))
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.small)
            .fixedSize()
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
    }

    /// The folder this menu shows: picked here, else the default, else the first.
    private var shownFolder: FolderOption? {
        if let id = service.shownFolderID, let f = service.folders.first(where: { $0.uuid == id }) { return f }
        // With no default set, the menu shows the first folder: name it and check it.
        return service.folders.first(where: { $0.inUse })
            ?? service.folders.first(where: { $0.path == service.defaultStationPath })
    }

    private var currentFolderName: String {
        shownFolder?.name ?? "My memory"
    }

    /// Which folder this menu is showing, and the way to another. One folder
    /// needs no choosing, so it's just named.
    @ViewBuilder
    private var folderPicker: some View {
        if service.folders.count <= 1 {
            HStack(spacing: 8) {
                Image(systemName: "book").font(.system(size: 12))
                Text(currentFolderName)
                    .font(.system(size: 12, weight: .medium))
                    .lineLimit(1)
            }
        } else {
            Menu {
                ForEach(shortlist(service.folders, showing: shownFolder?.uuid)) { folder in
                    Button(action: { service.showFolder(folder) }) {
                        if folder.uuid == shownFolder?.uuid {
                            Label(folder.name, systemImage: "checkmark")
                        } else {
                            Text(folder.name)
                        }
                    }
                }
                if shortlist(service.folders, showing: shownFolder?.uuid).count < service.folders.count {
                    Divider()
                    Button("All \(service.folders.count) Folders…") { service.openDashboard() }
                }
            } label: {
                HStack(spacing: 8) {
                    Image(systemName: "book")
                        .font(.system(size: 12))
                    Text(currentFolderName)
                        .font(.system(size: 12, weight: .medium))
                        .lineLimit(1)
                }
            }
            .menuStyle(.borderlessButton)
        }
    }

    // MARK: - Update available

    /// Shown only when GitHub has a newer release. Download opens the disk
    /// image; installing is the usual drag to Applications.
    private func updateRow(_ update: AvailableUpdate) -> some View {
        HStack(spacing: 8) {
            Image(systemName: "arrow.down.circle.fill")
                .foregroundColor(.accentColor)
                .font(.system(size: 13))
            Text("Geniuz \(update.version) is available")
                .font(.system(size: 12))
                .lineLimit(1)
            Spacer()
            Button("Download") { service.openUpdate() }
                .buttonStyle(.bordered)
                .controlSize(.small)
        }
        .padding(.horizontal, 16)
        .padding(.bottom, 8)
    }

    // MARK: - Connection, quiet when fine

    /// Something to do about the connection: not connected, or a restart owed.
    /// An unreadable Claude config counts too, so its message row shows.
    private var needsAttention: Bool {
        (!service.mcpInstalled && service.claudeFound)
            || (service.mcpInstalled && service.restartRequired)
    }

    /// The header's small status mark: a check only when connected; orange when
    /// the row below asks for something; a plain circle otherwise (Claude Desktop
    /// not installed), never a check that looks like "connected".
    private var statusMark: some View {
        Image(systemName: needsAttention ? "exclamationmark.circle.fill"
              : service.mcpInstalled ? "checkmark.circle" : "circle")
            .font(.system(size: 13))
            .foregroundColor(needsAttention ? .orange : .secondary)
            .help(service.restartRequired && service.mcpInstalled ? "Restart Claude to finish connecting" : connectionText)
            .accessibilityLabel(connectionText)
    }

    // MARK: - Header

    /// How much is held, beside the version: the one number a person uses.
    private var headerDetail: String {
        let version = "v\(Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "0.0.0")"
        guard service.stationExists else { return version }
        let noun = service.memoryCount == 1 ? "memory" : "memories"
        return "\(formattedCount(service.memoryCount)) \(noun) · \(version)"
    }

    private var header: some View {
        HStack(spacing: 10) {
            Image("MenuBarIcon")
                .resizable()
                .renderingMode(.template)
                .foregroundColor(.accentColor)
                .frame(width: 18, height: 18)

            VStack(alignment: .leading, spacing: 1) {
                HStack(spacing: 5) {
                    Text("Geniuz")
                        .font(.custom("Newsreader16pt-Regular", size: 16).weight(.semibold))
                    // Clay dot ties this to the website wordmark (Geniuz ●).
                    Circle()
                        .fill(Color(red: 0.812, green: 0.333, blue: 0.208))
                        .frame(width: 5, height: 5)
                }
                Text(headerDetail)
                    .font(.custom("HankenGrotesk-Regular", size: 11).weight(.semibold))
                    .foregroundColor(.secondary)
            }

            Spacer()

            statusMark

            Button(action: { service.openSettings() }) {
                Image(systemName: "gearshape")
                    .font(.system(size: 13))
                    .foregroundColor(.secondary)
            }
            .buttonStyle(.plain)
            .help("Settings")
        }
        .padding(.horizontal, 16)
        .padding(.top, 12)
        .padding(.bottom, 10)
    }

    // MARK: - Empty state (no station yet)

    private var emptyState: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 6) {
                Image(systemName: "circle.fill")
                    .font(.system(size: 6))
                    .foregroundColor(.secondary)
                Text("No memories yet")
                    .foregroundColor(.secondary)
            }
            Text("Start a conversation in Claude Desktop. Say something worth remembering.")
                .font(.caption)
                .foregroundColor(.secondary)
                .padding(.top, 2)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
    }

    // MARK: - Recent memories with threading viz

    private var recentMemoriesList: some View {
        VStack(alignment: .leading, spacing: 0) {
            // Section label, with a chevron when there's something to expand into.
            Button(action: {
                withAnimation(.easeInOut(duration: 0.15)) {
                    service.recentExpanded.toggle()
                }
            }) {
                HStack(spacing: 4) {
                    Text("RECENT")
                        .font(.system(size: 10, weight: .semibold))
                        .foregroundColor(.secondary)
                        .tracking(0.5)
                    Spacer()
                    if service.settings.recentMemoriesCount > 1 {
                        Image(systemName: service.recentExpanded ? "chevron.up" : "chevron.down")
                            .font(.system(size: 9, weight: .semibold))
                            .foregroundColor(.secondary)
                    }
                }
            }
            .buttonStyle(.plain)
            .padding(.horizontal, 16)
            .padding(.top, 10)
            .padding(.bottom, 6)

            // Visible items: when collapsed, show 1 (regardless of setting > 0); when
            // expanded, honor the user's recent_memories_count cap.
            let cap = max(0, service.settings.recentMemoriesCount)
            let visible: [GeniuzService.RecentMemory] = service.recentExpanded
                ? Array(service.recentMemories.prefix(cap))
                : Array(service.recentMemories.prefix(min(1, cap)))

            VStack(alignment: .leading, spacing: 0) {
                ForEach(visible) { m in
                    memoryRow(m)
                }
            }
            .padding(.bottom, 8)
        }
    }

    /// One recent-memory row. Threading is signaled by a left-side connector
    /// elbow and a slight gist-color demotion for follow-ups.
    private func memoryRow(_ m: GeniuzService.RecentMemory) -> some View {
        HStack(alignment: .top, spacing: 8) {
            if m.isThreadFollowup {
                // Connector elbow: a thin teal L on the left margin signals "this is a follow-up."
                ZStack(alignment: .topLeading) {
                    Path { p in
                        p.move(to: CGPoint(x: 5, y: 0))
                        p.addLine(to: CGPoint(x: 5, y: 8))
                        p.addQuadCurve(to: CGPoint(x: 11, y: 14), control: CGPoint(x: 5, y: 14))
                    }
                    .stroke(Color.accentColor.opacity(0.5), lineWidth: 1)
                    .frame(width: 14, height: 18)
                    Circle()
                        .fill(Color.accentColor.opacity(0.55))
                        .frame(width: 4, height: 4)
                        .offset(x: 14, y: 12)
                }
                .frame(width: 22, height: 18)
                .padding(.top, 0)
            } else {
                Circle()
                    .fill(Color.accentColor)
                    .frame(width: 5, height: 5)
                    .padding(.leading, 4)
                    .padding(.top, 7)
                    .frame(width: 22, alignment: .leading)
            }

            VStack(alignment: .leading, spacing: 2) {
                Text(m.gist)
                    .font(.system(size: 12))
                    .foregroundColor(m.isThreadFollowup ? .secondary : .primary)
                    .lineLimit(2)
                    .fixedSize(horizontal: false, vertical: true)
                if let when = m.when {
                    Text(when)
                        .font(.system(size: 10))
                        .foregroundColor(.secondary)
                }
            }
            .padding(.top, 1)

            Spacer(minLength: 0)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 4)
    }

    // MARK: - Connection status (footer row above Quit)

    private var connectionStatus: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                Image(systemName: service.mcpInstalled && !service.restartRequired ? "checkmark.circle.fill" : "exclamationmark.circle.fill")
                    .foregroundColor(service.mcpInstalled && !service.restartRequired ? .green : .orange)
                    .font(.system(size: 13))
                Text(connectionText)
                    .font(.system(size: 12))
                    .foregroundColor(.primary)
                    .fixedSize(horizontal: false, vertical: true)
                Spacer()
            }

            if !service.mcpInstalled && service.claudeFound && !service.claudeConfigUnreadable {
                Button("Connect to Claude Desktop") {
                    service.configureClaudeConnection()
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.small)
                .tint(.orange)
            }

            if service.mcpInstalled && service.restartRequired {
                Button("Restart Claude to finish") {
                    service.restartClaude()
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.small)
                .tint(.orange)
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
    }

    private var connectionText: String {
        if service.mcpInstalled && service.restartRequired { return "Connected. Restart Claude to finish." }
        if service.mcpInstalled { return "Claude Desktop connected" }
        if service.claudeConfigUnreadable { return "Claude Desktop's settings file couldn't be read, so Geniuz left it alone." }
        if !service.claudeFound { return "Geniuz will connect when Claude Desktop is installed." }
        return "Claude Desktop not connected"
    }

    // MARK: - Quit row (Mac convention: last menu-style item)

    private var quitRow: some View {
        Button(action: { NSApplication.shared.terminate(nil) }) {
            HStack(spacing: 8) {
                Image(systemName: "power")
                    .font(.system(size: 12))
                Text("Quit Geniuz")
                    .font(.system(size: 12))
                Spacer()
            }
            .foregroundColor(.secondary)
            .padding(.horizontal, 16)
            .padding(.vertical, 8)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    // MARK: - Helpers

    private func formattedCount(_ n: Int) -> String {
        let f = NumberFormatter()
        f.numberStyle = .decimal
        return f.string(from: NSNumber(value: n)) ?? "\(n)"
    }
}

// MARK: - Settings window
//
// Opened via the gear icon in the GeniuzMenu header. Save-on-change UX
// (Mac convention) — every Toggle/Picker/Stepper writes through to
// settings.json on the spot, no Apply/Cancel buttons. The window is
// reused across opens; closing just hides it.

struct GeniuzSettingsView: View {
    @ObservedObject var service: GeniuzService

    var body: some View {
        Form {
            Section("General") {
                Toggle("Launch at login", isOn: launchAtLoginBinding)
            }

            if service.claudeCodePresent {
                Section("Claude Code") {
                    if service.claudeCodeConnected {
                        Label("Connected", systemImage: "checkmark.circle.fill")
                    } else {
                        Button("Connect Claude Code") { service.connectClaudeCode() }
                        Text("Gives every Claude Code session on this computer access to your memories.")
                            .font(.caption2)
                            .foregroundColor(.secondary)
                        if service.claudeCodeFailed {
                            Text("That didn't work. In Terminal, run: claude mcp add --scope user Geniuz -- \"\(service.geniuzBinaryPath)\" mcp serve")
                                .font(.caption2)
                                .foregroundColor(.secondary)
                                .textSelection(.enabled)
                        }
                    }
                }
            }

            Section("Command line") {
                if service.cliOnPath {
                    Label("The geniuz command is ready", systemImage: "checkmark.circle.fill")
                } else {
                    Button("Install the geniuz command") { service.installCommand() }
                    Text(service.cliInstallFailed
                         ? "That didn't work. Connecting your AI doesn't need this command."
                         : "Optional, for using geniuz in Terminal. Connecting your AI doesn't need it.")
                        .font(.caption2)
                        .foregroundColor(.secondary)
                }
            }

            Section("Default folder") {
                Picker("New memories go to", selection: defaultFolderBinding) {
                    Text("None: ask which folder").tag("none")
                    if defaultFolderBinding.wrappedValue.isEmpty {
                        // Never chosen (the first folder is used), or a folder that isn't
                        // reachable right now (a drive not connected).
                        Text(service.defaultFolderSetting == nil ? "First folder (not chosen yet)" : "A folder that isn't available").tag("")
                    }
                    ForEach(service.folders) { f in
                        Text(f.name).tag(f.uuid)
                    }
                }
                Text(service.defaultFolderSetting?.lowercased() == "none"
                     ? "Claude, the command line and the MCP must name a folder; nothing is kept in a folder by guess. Agents' homes still use their own folder."
                     : "Where Claude, the command line and the MCP keep memories when no folder is named. Looking at a folder in the menu or the dashboard doesn't change it.")
                    .font(.caption2)
                    .foregroundColor(.secondary)
            }

            Section("Display") {
                Stepper(
                    "Recent memories shown: \(service.settings.recentMemoriesCount)",
                    value: recentCountBinding,
                    in: 0...20
                )
                Text("Set to 0 to hide the recent memories section in the menu.")
                    .font(.caption2)
                    .foregroundColor(.secondary)
            }

            Section("Storage") {
                LabeledContent("Location") {
                    Text(service.dataDir)
                        .font(.system(.caption, design: .monospaced))
                        .textSelection(.enabled)
                        .multilineTextAlignment(.trailing)
                }
                Text("The app always uses this location. GENIUZ_HOME in a shell changes where only the command line looks.")
                    .font(.caption2)
                    .foregroundColor(.secondary)
            }

            Section("Updates") {
                Toggle("Check for updates automatically", isOn: autoupdateBinding)
                if service.settings.autoupdateEnabled {
                    Picker("How often", selection: frequencyBinding) {
                        Text("Daily").tag("daily")
                        Text("Weekly").tag("weekly")
                        Text("Only when I ask").tag("manual")
                    }
                }
                HStack {
                    Button("Check Now") { service.checkForUpdate(userAsked: true) }
                        .disabled(service.checkingForUpdate)
                    if let update = service.availableUpdate {
                        Button("Download \(update.version)") { service.openUpdate() }
                    }
                }
                if let msg = service.updateCheckMessage {
                    Text(msg).font(.caption2).foregroundColor(.secondary)
                }
                Text("A check asks GitHub for the latest Geniuz release. GitHub sees this Mac's internet address and the Geniuz version; none of your memories or folders are sent.")
                    .font(.caption2)
                    .foregroundColor(.secondary)
            }

            Section("About") {
                LabeledContent("Version") {
                    Text(Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "?")
                        .font(.system(.caption, design: .monospaced))
                }
            }
        }
        .formStyle(.grouped)
        .padding(0)
        .frame(minWidth: 380, minHeight: 320)
    }

    // MARK: - Save-on-change bindings

    private var launchAtLoginBinding: Binding<Bool> {
        Binding(
            get: { service.settings.launchAtLogin },
            set: { v in service.updateSettings { $0.launchAtLogin = v } }
        )
    }

    private var defaultFolderBinding: Binding<String> {
        Binding(
            get: {
                guard let v = service.defaultFolderSetting else { return "" }
                if v.lowercased() == "none" { return "none" }
                return service.folders.first(where: { $0.uuid.caseInsensitiveCompare(v) == .orderedSame })?.uuid ?? ""
            },
            set: { v in if !v.isEmpty { service.setDefaultFolder(v) } }
        )
    }

    private var autoupdateBinding: Binding<Bool> {
        Binding(
            get: { service.settings.autoupdateEnabled },
            set: { v in service.updateSettings { $0.autoupdateEnabled = v } }
        )
    }

    private var frequencyBinding: Binding<String> {
        Binding(
            get: { service.settings.updateCheckFrequency },
            set: { v in service.updateSettings { $0.updateCheckFrequency = v } }
        )
    }

    private var recentCountBinding: Binding<Int> {
        Binding(
            get: { service.settings.recentMemoriesCount },
            set: { v in service.updateSettings { $0.recentMemoriesCount = v } }
        )
    }
}
