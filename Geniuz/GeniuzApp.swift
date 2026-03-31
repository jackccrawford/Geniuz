import SwiftUI

@main
struct GeniuzApp: App {
    @StateObject private var service = GeniuzService()

    var body: some Scene {
        MenuBarExtra("Geniuz", systemImage: "brain.head.profile") {
            GeniuzMenu(service: service)
        }
        .menuBarExtraStyle(.window)
    }
}
