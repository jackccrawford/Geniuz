import SwiftUI

@main
struct GeniuzApp: App {
    @StateObject private var service = GeniuzService()

    var body: some Scene {
        MenuBarExtra {
            GeniuzMenu(service: service)
        } label: {
            Image(systemName: "brain.filled.head.profile")
                .symbolRenderingMode(.hierarchical)
        }
        .menuBarExtraStyle(.window)
    }
}
