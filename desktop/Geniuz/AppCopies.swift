import Foundation

/// Two copies of Geniuz in Applications, which is what Finder's "Keep Both" leaves:
/// the old one as `Geniuz.app`, the new one as `Geniuz 2.app`.
///
/// Everything outside the app points at `/Applications/Geniuz.app` by path: the
/// `geniuz` command's link, Claude's connection, the login item. So with two copies
/// the window says the new version while Terminal and the person's AI go on running
/// the old one (Jack, 14 Nov 2026: 5.1.3 in the menu bar, `geniuz --version` 5.1.2).
/// The cure is one copy, at that path. This decides when to offer it; the moving is
/// ApplicationsMover's, which already does it for an app run from its download.
enum AppCopies {
    static let home = "/Applications/Geniuz.app"

    /// Whether this copy should offer to take the place of the one at `home`.
    ///
    /// Only for a copy sitting beside it in Applications under another name, and only
    /// when the one at `home` is the same app and older. A build run from Xcode, or a
    /// copy kept somewhere else on purpose, is left alone.
    static func shouldReplaceHome(myPath: String, myVersion: String, myBundleID: String?,
                                  homeVersion: String?, homeBundleID: String?) -> Bool {
        guard myPath != home else { return false }
        guard (myPath as NSString).deletingLastPathComponent == (home as NSString).deletingLastPathComponent else { return false }
        guard let homeVersion, let myBundleID, homeBundleID == myBundleID else { return false }
        return isOlder(homeVersion, than: myVersion)
    }

    /// Dotted numbers compared part by part: 5.1.2 is older than 5.1.10.
    static func isOlder(_ a: String, than b: String) -> Bool {
        let parts = { (v: String) in v.split(separator: ".").map { Int($0.prefix { $0.isNumber }) ?? 0 } }
        let (x, y) = (parts(a), parts(b))
        for i in 0..<max(x.count, y.count) {
            let (l, r) = (i < x.count ? x[i] : 0, i < y.count ? y[i] : 0)
            if l != r { return l < r }
        }
        return false
    }

    /// The version and bundle ID of the app at `path`, read from its Info.plist.
    static func identity(ofAppAt path: String) -> (version: String?, bundleID: String?) {
        guard let plist = NSDictionary(contentsOfFile: "\(path)/Contents/Info.plist") else { return (nil, nil) }
        return (plist["CFBundleShortVersionString"] as? String, plist["CFBundleIdentifier"] as? String)
    }
}
