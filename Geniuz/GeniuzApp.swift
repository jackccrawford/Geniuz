//
//  GeniuzApp.swift
//  Geniuz
//
//  Created by Jack C Crawford 2 on 3/30/26.
//

import SwiftUI
import CoreData

@main
struct GeniuzApp: App {
    let persistenceController = PersistenceController.shared

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environment(\.managedObjectContext, persistenceController.container.viewContext)
        }
    }
}
