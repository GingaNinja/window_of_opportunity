#!/usr/bin/env bash
# Runs a GUI example, screenshots its window, and exits.
#
# usage: scripts/capture-screenshot.sh <binary> <output.png> [settle-seconds]
#
# Captures just the app's window via its CGWindowID when we can find it,
# falling back to the full screen. The hosted macOS runner has a GUI session
# with Screen Recording pre-granted, so screencapture works headlessly.

set -euo pipefail

bin="${1:?usage: capture-screenshot.sh <binary> <output.png> [settle-seconds]}"
out="${2:?usage: capture-screenshot.sh <binary> <output.png> [settle-seconds]}"
settle="${3:-5}"

here="$(cd "$(dirname "$0")" && pwd)"

"$bin" &
pid=$!
trap 'kill "$pid" 2>/dev/null || true' EXIT

# Give the app time to create and lay out its window.
sleep "$settle"

# -x: no shutter sound.
wid=""
if swiftc -o /tmp/windowid "$here/windowid.swift" >/dev/null 2>&1; then
    wid="$(/tmp/windowid "$(basename "$bin")" || true)"
fi

if [ -n "$wid" ]; then
    echo "Capturing window id $wid"
    screencapture -x -l"$wid" "$out"
else
    echo "No window id found; capturing the full screen"
    screencapture -x "$out"
fi

test -s "$out"
echo "Saved screenshot to $out"
