// Lists the on-screen window ids of one process owner, for evidence capture.
//
// The update self-test captures the app window itself (only that window, no
// desktop) with `screencapture -l <id>`; this prints `<id>\t<pid>\t<owner>\t<title>`
// for the owner named in argv[1] (its process name, as macOS shows it).
//
//   swift -module-cache-path <cache> window_ids.swift QUSOR
//
// Swift needs a writable module cache; NSH app builds already need the Xcode
// command line tools, so this adds no new prerequisite.
import CoreGraphics
import Foundation

let wanted = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : ""
let options: CGWindowListOption = [.optionOnScreenOnly, .excludeDesktopElements]
guard let windows = CGWindowListCopyWindowInfo(options, kCGNullWindowID) as? [[String: Any]] else {
    exit(1)
}
for window in windows {
    let owner = window[kCGWindowOwnerName as String] as? String ?? ""
    let layer = window[kCGWindowLayer as String] as? Int ?? 0
    guard layer == 0, owner == wanted else { continue }
    let pid = window[kCGWindowOwnerPID as String] as? Int ?? 0
    let number = window[kCGWindowNumber as String] as? Int ?? 0
    let title = window[kCGWindowName as String] as? String ?? ""
    print("\(number)\t\(pid)\t\(owner)\t\(title)")
}
