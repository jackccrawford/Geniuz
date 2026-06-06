import SwiftUI
import AppKit
import CoreText

@main
struct GeniuzApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) var appDelegate

    init() {
        Self.registerCustomFonts()
    }

    /// Register the bundled brand fonts so SwiftUI's Font.custom can find them.
    /// Newsreader carries the wordmark; Hanken Grotesk carries supporting text.
    /// Both ship as variable fonts in Geniuz/Fonts/.
    private static func registerCustomFonts() {
        let filenames = ["Newsreader-VariableFont", "HankenGrotesk-VariableFont"]
        for name in filenames {
            guard let url = Bundle.main.url(forResource: name, withExtension: "ttf") else { continue }
            CTFontManagerRegisterFontsForURL(url as CFURL, .process, nil)
        }
    }

    // Menu-bar-only app: no Scenes. The status item is created in
    // AppDelegate.applicationDidFinishLaunching. Returning an empty
    // `Settings` scene would add an empty "Preferences..." menu item;
    // returning no scenes at all gives us the clean menu-bar-only UX.
    var body: some Scene {
        Settings {
            EmptyView()
        }
        .commands {
            // Remove default "Preferences..." from the app menu; we have
            // no settings surface yet. Re-adding when real settings exist.
            CommandGroup(replacing: .appSettings) {}
            // Remove Help menu entirely; "Help isn't available for Geniuz"
            // is worse than no Help menu. Re-add when a Help Book exists.
            CommandGroup(replacing: .help) {}
        }
    }
}

class AppDelegate: NSObject, NSApplicationDelegate {
    var statusItem: NSStatusItem!
    var popover: NSPopover!
    var service: GeniuzService!
    var globalEventMonitor: Any?

    func applicationDidFinishLaunching(_ notification: Notification) {
        // Menubar-only: no dock icon. The dashboard is launched as a
        // separate process by menu action — it appears in the dock while
        // open, leaving the menubar item as the always-on Geniuz presence.
        NSApp.setActivationPolicy(.accessory)

        service = GeniuzService()

        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        if let button = statusItem.button {
            button.image = NSImage(named: "MenuBarIcon")
            button.image?.size = NSSize(width: 18, height: 18)
            button.image?.isTemplate = true
            button.action = #selector(togglePopover)
            button.target = self
            button.toolTip = service.tooltipText()
        }

        popover = NSPopover()
        popover.contentSize = NSSize(width: 280, height: 320)
        popover.behavior = .transient
        popover.contentViewController = NSHostingController(rootView: GeniuzMenu(service: service))

        // Refresh the tooltip whenever the service publishes new state so
        // hover always reflects the latest count + most-recent gist.
        service.onStateChange = { [weak self] in
            self?.statusItem.button?.toolTip = self?.service.tooltipText()
        }

        // NSPopover.transient is supposed to close when the user interacts
        // outside the popover, but it has a long-standing gap with system
        // status bar items: clicking another app's menu bar icon, or the
        // dock, does not register as "outside" and the popover stays open.
        // The global event monitor fires for mouse-down events in any
        // *other* application, catching exactly what .transient misses.
        // Clicks on Geniuz's own status item still go through togglePopover.
        globalEventMonitor = NSEvent.addGlobalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown]) { [weak self] _ in
            guard let self = self, self.popover.isShown else { return }
            self.popover.performClose(nil)
        }
    }

    @objc func togglePopover() {
        guard let button = statusItem.button else { return }
        if popover.isShown {
            popover.performClose(nil)
        } else {
            service.refresh()
            popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)
        }
    }
}
