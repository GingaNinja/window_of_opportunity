// Prints "<windowid> <x> <y> <w> <h>" for the named on-screen window
// (matched by owner process name). Used by scripts/capture-screenshot.sh:
// the id for `screencapture -l<id>`, the bounds for a `-R x,y,w,h` crop
// when window capture is refused.
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
    // layer 0 == normal windows; w/h > 0 skips minimised/placeholder entries
    guard name == owner, layer == 0,
        let id = window[kCGWindowNumber as String] as? Int,
        let bounds = window[kCGWindowBounds as String] as? [String: Any],
        let x = bounds["X"] as? Double,
        let y = bounds["Y"] as? Double,
        let w = bounds["Width"] as? Double,
        let h = bounds["Height"] as? Double,
        w > 0, h > 0
    else { continue }
    print("\(id) \(Int(x)) \(Int(y)) \(Int(w)) \(Int(h))")
    exit(0)
}
exit(1)
