import SwiftUI

struct GeniuzMenu: View {
    @ObservedObject var service: GeniuzService

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            Divider()

            if service.stationExists {
                statsHero
                Divider()
                recentMemoriesList
            } else {
                emptyState
            }

            Divider()
            connectionStatus

            if !service.cliOnPath {
                Divider()
                cliInstallSection
            }

            Divider()
            openDashboardRow

            Divider()
            quitRow
        }
        .frame(width: 360)
        .background(.regularMaterial)
    }

    // MARK: - Open Dashboard

    private var openDashboardRow: some View {
        Button(action: { service.openDashboard() }) {
            HStack(spacing: 8) {
                Image(systemName: "rectangle.grid.2x2")
                    .font(.system(size: 12))
                Text("Open Dashboard")
                    .font(.system(size: 12, weight: .medium))
                Spacer()
            }
            .foregroundColor(.primary)
            .padding(.horizontal, 16)
            .padding(.vertical, 8)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    // MARK: - Header

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
                Text("v\(Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "0.0.0")")
                    .font(.custom("HankenGrotesk-Regular", size: 11).weight(.semibold))
                    .foregroundColor(.secondary)
            }

            Spacer()

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

    // MARK: - Stats hero

    private var statsHero: some View {
        HStack(spacing: 18) {
            statTile(value: formattedCount(service.memoryCount), label: service.memoryCount == 1 ? "memory" : "memories")
            divider
            statTile(value: "+\(service.addedToday)", label: "today")
            divider
            statTile(value: "\(service.threadCount)", label: service.threadCount == 1 ? "thread" : "threads")
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .background(
            LinearGradient(
                colors: [
                    Color.accentColor.opacity(0.10),
                    Color.accentColor.opacity(0.04)
                ],
                startPoint: .topLeading, endPoint: .bottomTrailing
            )
        )
    }

    private func statTile(value: String, label: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(value)
                .font(.system(size: 20, weight: .semibold, design: .rounded))
                .foregroundColor(.primary)
            Text(label)
                .font(.system(size: 11))
                .foregroundColor(.secondary)
        }
    }

    private var divider: some View {
        Rectangle()
            .fill(Color.secondary.opacity(0.18))
            .frame(width: 1, height: 28)
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

            Text(m.gist)
                .font(.system(size: 12))
                .foregroundColor(m.isThreadFollowup ? .secondary : .primary)
                .lineLimit(2)
                .fixedSize(horizontal: false, vertical: true)
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
                Image(systemName: service.mcpInstalled ? "checkmark.circle.fill" : "xmark.circle")
                    .foregroundColor(service.mcpInstalled ? .green : .orange)
                    .font(.system(size: 13))
                Text(service.mcpInstalled ? "Claude Desktop connected" : "Claude Desktop not configured")
                    .font(.system(size: 12))
                    .foregroundColor(.primary)
                Spacer()
            }

            if !service.mcpInstalled {
                Button("Configure Claude Connection") {
                    service.configureClaudeConnection()
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.small)
                .tint(.orange)
            }

            if service.mcpInstalled && service.restartRequired {
                HStack(alignment: .top, spacing: 6) {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .foregroundColor(.yellow)
                        .font(.caption)
                    Text("Restart Claude Desktop to activate Geniuz.")
                        .font(.caption)
                        .foregroundColor(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
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

    // MARK: - CLI install section (kept from prior layout)

    private var cliInstallSection: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 6) {
                Image(systemName: "terminal")
                    .foregroundColor(.secondary)
                    .font(.caption)
                Text("Use Geniuz from Terminal")
                    .font(.caption)
            }

            if service.cliCopyConfirmation {
                HStack(alignment: .top, spacing: 6) {
                    Image(systemName: "checkmark.circle.fill")
                        .foregroundColor(.green)
                        .font(.caption)
                    Text("Command copied. Paste into Terminal and press Return.")
                        .font(.caption)
                        .foregroundColor(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                .padding(.top, 2)
            } else {
                Button("Copy Install Command") {
                    service.copyCliInstallCommand()
                }
                .buttonStyle(.bordered)
                .controlSize(.small)

                Text("Copies a one-line `sudo` command. Paste into Terminal to add `geniuz` to your PATH.")
                    .font(.caption2)
                    .foregroundColor(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.top, 2)
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
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
                Toggle("Check for updates automatically", isOn: autoupdateBinding)
                Picker("Update frequency", selection: frequencyBinding) {
                    Text("Daily").tag("daily")
                    Text("Weekly").tag("weekly")
                    Text("Manually only").tag("manual")
                }
                .disabled(!service.settings.autoupdateEnabled)
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
                LabeledContent("Folder") {
                    Text(service.dataDir)
                        .font(.system(.caption, design: .monospaced))
                        .textSelection(.enabled)
                        .multilineTextAlignment(.trailing)
                }
                Text("Override by setting GENIUZ_HOME in your shell profile, then restart Geniuz.")
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
