#!/bin/bash
# Runs CommandInstaller against scratch homes, with no app launched and nothing
# real touched:  desktop/scripts/test-command-installer.sh
set -euo pipefail
cd "$(dirname "$0")/.."
T=$(mktemp -d); trap 'rm -rf "$T"' EXIT
cat > "$T/main.swift" <<'SWIFT'
import Foundation
var failed = 0
func check(_ ok: Bool, _ what: String) { if ok { print("ok   \(what)") } else { print("FAIL \(what)"); failed += 1 } }
let fm = FileManager.default
let root = CommandLine.arguments[1]
func home(_ name: String) -> String { let h = "\(root)/\(name)"; try! fm.createDirectory(atPath: h, withIntermediateDirectories: true); return h }
let bundled = "\(root)/Geniuz.app/geniuz"
try! fm.createDirectory(atPath: "\(root)/Geniuz.app", withIntermediateDirectories: true)
fm.createFile(atPath: bundled, contents: Data("#!/bin/sh\necho geniuz test\n".utf8), attributes: [.posixPermissions: 0o755])
func read(_ p: String) -> String { (try? String(contentsOfFile: p, encoding: .utf8)) ?? "" }
func dest(_ p: String) -> String? { try? fm.destinationOfSymbolicLink(atPath: p) }

// 1. ~/.local/bin already on the PATH: a link is all, no file is written.
do {
    let h = home("onpath"); try! fm.createDirectory(atPath: "\(h)/.local/bin", withIntermediateDirectories: true)
    let i = CommandInstaller(home: h, bundled: bundled, shell: "/bin/zsh", pathDirs: ["/usr/bin", "\(h)/.local/bin"], systemDirs: [])
    check(!i.alreadyInstalled(), "on PATH: not installed before")
    check(i.install() == .linked(dir: "\(h)/.local/bin"), "on PATH: linked")
    check(dest("\(h)/.local/bin/geniuz") == bundled, "on PATH: link points at the app's geniuz")
    check(!fm.fileExists(atPath: "\(h)/.zshenv"), "on PATH: no startup file written")
    check(i.alreadyInstalled(), "on PATH: installed after")
}
// 2. A plain Mac with zsh and nothing set up: folder made, link made, one marked line added, once.
do {
    let h = home("fresh")
    let i = CommandInstaller(home: h, bundled: bundled, shell: "/bin/zsh", pathDirs: ["/usr/bin", "/bin"], systemDirs: [])
    check(i.install() == .linkedAndPathAdded(dir: "\(h)/.local/bin", file: "\(h)/.zshenv"), "fresh zsh: linked and PATH added")
    let text = read("\(h)/.zshenv")
    check(text.contains(CommandInstaller.marker) && text.contains(CommandInstaller.pathLine), "fresh zsh: marker and line in .zshenv")
    check(i.alreadyInstalled(), "fresh zsh: installed after (by the marker, PATH not yet reread)")
    check(i.install() == .linkedPathAlreadyAdded(dir: "\(h)/.local/bin", file: "\(h)/.zshenv"), "fresh zsh: second run adds nothing")
    check(read("\(h)/.zshenv") == text, "fresh zsh: file unchanged by the second run")
}
// 3. An existing .zshenv keeps every line, including one with no newline at its end.
do {
    let h = home("existing"); try! "export EDITOR=vim".write(toFile: "\(h)/.zshenv", atomically: true, encoding: .utf8)
    let i = CommandInstaller(home: h, bundled: bundled, shell: "/bin/zsh", pathDirs: nil, systemDirs: [])
    check(i.install() == .linkedAndPathAdded(dir: "\(h)/.local/bin", file: "\(h)/.zshenv"), "existing .zshenv: added (PATH unknown)")
    check(read("\(h)/.zshenv").hasPrefix("export EDITOR=vim\n\n" + CommandInstaller.marker), "existing .zshenv: old line kept, ours on its own lines")
}
// 4. bash writes .bash_profile.
do {
    let h = home("bash")
    let i = CommandInstaller(home: h, bundled: bundled, shell: "/bin/bash", pathDirs: ["/usr/bin"], systemDirs: [])
    check(i.install() == .linkedAndPathAdded(dir: "\(h)/.local/bin", file: "\(h)/.bash_profile"), "bash: .bash_profile")
}
// 5. A shell we don't know: nothing is touched.
do {
    let h = home("fish")
    let i = CommandInstaller(home: h, bundled: bundled, shell: "/opt/homebrew/bin/fish", pathDirs: ["/usr/bin"], systemDirs: [])
    check(i.install() == .notPossible, "fish: not possible")
    check(!fm.fileExists(atPath: "\(h)/.local"), "fish: nothing created")
}
// 6. A real file called geniuz is never replaced.
do {
    let h = home("realfile"); try! fm.createDirectory(atPath: "\(h)/.local/bin", withIntermediateDirectories: true)
    try! "mine".write(toFile: "\(h)/.local/bin/geniuz", atomically: true, encoding: .utf8)
    let i = CommandInstaller(home: h, bundled: bundled, shell: "/bin/zsh", pathDirs: ["\(h)/.local/bin"], systemDirs: [])
    check(i.install() == .notPossible, "real file: not possible")
    check(read("\(h)/.local/bin/geniuz") == "mine", "real file: left as it was")
    check(!fm.fileExists(atPath: "\(h)/.zshenv"), "real file: no startup file written")
}
// 7. A startup file that is itself a link (a dotfiles setup) is left alone.
do {
    let h = home("dotfiles"); try! "".write(toFile: "\(h)/real-zshenv", atomically: true, encoding: .utf8)
    try! fm.createSymbolicLink(atPath: "\(h)/.zshenv", withDestinationPath: "\(h)/real-zshenv")
    let i = CommandInstaller(home: h, bundled: bundled, shell: "/bin/zsh", pathDirs: ["/usr/bin"], systemDirs: [])
    check(i.install() == .notPossible, "linked .zshenv: not possible")
    check(read("\(h)/real-zshenv").isEmpty, "linked .zshenv: target untouched")
}
// 8. An old link is replaced (the app moved).
do {
    let h = home("oldlink"); try! fm.createDirectory(atPath: "\(h)/.local/bin", withIntermediateDirectories: true)
    try! fm.createSymbolicLink(atPath: "\(h)/.local/bin/geniuz", withDestinationPath: "/nowhere/geniuz")
    let i = CommandInstaller(home: h, bundled: bundled, shell: "/bin/zsh", pathDirs: ["\(h)/.local/bin"], systemDirs: [])
    check(!i.alreadyInstalled(), "old link: a dead link is not 'installed'")
    check(i.install() == .linked(dir: "\(h)/.local/bin") && dest("\(h)/.local/bin/geniuz") == bundled, "old link: replaced")
}
print(failed == 0 ? "ALL PASSED" : "\(failed) FAILED"); exit(failed == 0 ? 0 : 1)
SWIFT
swiftc -O -o "$T/t" Geniuz/CommandInstaller.swift "$T/main.swift"
"$T/t" "$T/homes"
# And for real: a new zsh with that scratch home finds and runs the command.
H="$T/homes/fresh"; out=$(env -i HOME="$H" PATH=/usr/bin:/bin /bin/zsh -c 'command -v geniuz && geniuz')
echo "$out" | grep -q "geniuz test" && echo "ok   a new zsh in the scratch home runs geniuz" || { echo "FAIL zsh did not find geniuz: $out"; exit 1; }
