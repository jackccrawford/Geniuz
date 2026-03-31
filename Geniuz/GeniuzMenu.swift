import SwiftUI

struct GeniuzMenu: View {
    @ObservedObject var service: GeniuzService

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            // Header
            HStack {
                Text("Geniuz")
                    .font(.headline)
                Spacer()
                Text("v\(Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "0.1.0")")
                    .font(.caption)
                    .foregroundColor(.secondary)
            }
            .padding(.horizontal, 16)
            .padding(.top, 12)
            .padding(.bottom, 8)

            Divider()

            // Status
            VStack(alignment: .leading, spacing: 6) {
                if service.stationExists {
                    HStack(spacing: 6) {
                        Image(systemName: "circle.fill")
                            .font(.system(size: 6))
                            .foregroundColor(.green)
                        Text("\(service.memoryCount) memories")
                            .font(.system(.body, design: .rounded))
                    }

                    if service.embeddingCount < service.memoryCount {
                        HStack(spacing: 6) {
                            Image(systemName: "circle.fill")
                                .font(.system(size: 6))
                                .foregroundColor(.yellow)
                            Text("\(service.embeddingCount)/\(service.memoryCount) embedded")
                                .font(.caption)
                                .foregroundColor(.secondary)
                        }
                    } else if service.memoryCount > 0 {
                        HStack(spacing: 6) {
                            Image(systemName: "circle.fill")
                                .font(.system(size: 6))
                                .foregroundColor(.green)
                            Text("Semantic search ready")
                                .font(.caption)
                                .foregroundColor(.secondary)
                        }
                    }

                    if let gist = service.lastSignalGist {
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Last memory")
                                .font(.caption2)
                                .foregroundColor(.secondary)
                                .textCase(.uppercase)
                            Text(gist)
                                .font(.caption)
                                .lineLimit(2)
                            if let date = service.lastSignalDate {
                                Text(date)
                                    .font(.caption2)
                                    .foregroundColor(.secondary)
                            }
                        }
                        .padding(.top, 4)
                    }
                } else {
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
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 10)

            Divider()

            // MCP Status
            VStack(alignment: .leading, spacing: 6) {
                HStack(spacing: 6) {
                    Image(systemName: service.mcpInstalled ? "checkmark.circle.fill" : "xmark.circle")
                        .foregroundColor(service.mcpInstalled ? .green : .orange)
                    Text(service.mcpInstalled ? "Claude Desktop connected" : "Claude Desktop not configured")
                        .font(.caption)
                }

                if !service.mcpInstalled {
                    Button("Connect to Claude Desktop") {
                        service.installMcp()
                    }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.small)
                    .tint(.orange)
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 8)

            Divider()

            // Actions
            Button(action: { service.refresh() }) {
                Label("Refresh", systemImage: "arrow.clockwise")
            }
            .buttonStyle(.plain)
            .padding(.horizontal, 16)
            .padding(.vertical, 6)

            if service.mcpInstalled {
                Button(action: { service.uninstallMcp() }) {
                    Label("Disconnect from Claude Desktop", systemImage: "minus.circle")
                }
                .buttonStyle(.plain)
                .foregroundColor(.secondary)
                .padding(.horizontal, 16)
                .padding(.vertical, 6)
            }

            Divider()

            Button(action: { NSApplication.shared.terminate(nil) }) {
                Label("Quit Geniuz", systemImage: "power")
            }
            .buttonStyle(.plain)
            .padding(.horizontal, 16)
            .padding(.vertical, 6)
            .padding(.bottom, 4)
        }
        .frame(width: 280)
    }
}
