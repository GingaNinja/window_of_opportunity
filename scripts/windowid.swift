// Prints the CGWindowID of the named on-screen window (matched by owner
// process name). Used by scripts/capture-screenshot.sh so we can screenshot
// just the app's window with `screencapture -l<id>`.
import CoreGraphics
import Foundation

guard CommandLine.arguments.count > 1 else { exit(2) }
let owner = CommandLine.arguments[1]

let options = CGWindowListOption(arrayLiteral: .optionOnScreenOnly, .excludeDesktopElements)
guard let windows = CGWindowListCopyWindowInfo(options, kCGNullWindowID) as? [[String: Any]] else {
    exit(1)
}

for window in windows {
    let name = window[kCGWindowOwnerName as String] as? String
    let layer = window[kCGWindowLayer as String] as? Int ?? 0
    let bounds = window[kCGWindowBounds as String] as? [String: Any]
    let width = bounds?["Width"] as? Double ?? 0
    // layer 0 == normal windows; width > 0 skips minimised/placeholder entries
    if name == owner, layer == 0, width > 0,
        let id = window[kCGWindowNumber as String] as? Int {
        print(id)
        exit(0)
    }
}
exit(1)
