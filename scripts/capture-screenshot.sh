#!/usr/bin/env bash
# Runs a GUI example, screenshots its window, and exits.
#
# usage: scripts/capture-screenshot.sh <binary> <output.png> [settle-seconds]
#
# Captures the app's window (via its CGWindowID) when possible, falling back
# to a window-bounds crop and then the full screen. If the example exits
# early, this script FAILS rather than screenshotting whatever is behind it.
# The hosted macOS runner has a GUI session with Screen Recording
# pre-granted; local runs need it granted to the terminal.

set -euo pipefail

bin="${1:?usage: capture-screenshot.sh <binary> <output.png> [settle-seconds]}"
out="${2:?usage: capture-screenshot.sh <binary> <output.png> [settle-seconds]}"
settle="${3:-5}"

here="$(cd "$(dirname "$0")" && pwd)"

"$bin" &
pid=$!
trap 'kill "$pid" 2>/dev/null || true' EXIT

# A screenshot of a dead app is a screenshot of the desktop behind it —
# never commit one.
fail_if_dead() {
    if ! kill -0 "$pid" 2>/dev/null; then
        status=0
        wait "$pid" || status=$?
        echo "error: $bin exited early (status $status) - refusing to screenshot" >&2
        exit 1
    fi
}

wid="" wx="" wy="" ww="" wh=""
if swiftc -o /tmp/windowid "$here/windowid.swift" >/dev/null 2>&1; then
    # Wait for the window to appear (cap ~30s).
    for _ in $(seq 1 60); do
        fail_if_dead
        read -r wid wx wy ww wh <<< "$(/tmp/windowid "$(basename "$bin")" || true)"
        [ -n "$wid" ] && break
        sleep 0.5
    done
else
    echo "warning: could not build windowid helper; will capture the full screen" >&2
fi

fail_if_dead
sleep "$settle"
fail_if_dead

# -x: no shutter sound.
captured=""
if [ -n "$wid" ] && screencapture -x -l"$wid" "$out" 2>/dev/null; then
    echo "Captured window id $wid"
    captured=1
fi

if [ -z "$captured" ] && [ -n "$wid" ]; then
    # -l can fail with "could not create image from window" (occlusion,
    # permissions). Raise the app and retry once, then crop its region.
    osascript -e "tell application \"System Events\" to set frontmost of (first process whose unix id is $pid) to true" \
        >/dev/null 2>&1 || true
    sleep 1
    if screencapture -x -l"$wid" "$out" 2>/dev/null; then
        echo "Captured window id $wid after activating the app"
        captured=1
    elif screencapture -x -R"$wx,$wy,$ww,$wh" "$out" 2>/dev/null; then
        echo "Captured window region $wx,$wy,$ww,$wh"
        captured=1
    fi
fi

if [ -z "$captured" ]; then
    echo "warning: window capture failed - falling back to the full screen" >&2
    echo "hint: local runs need Screen Recording permission for the terminal:" >&2
    echo "      System Settings > Privacy & Security > Screen Recording" >&2
    screencapture -x "$out"
fi

fail_if_dead
test -s "$out"
echo "Saved screenshot to $out"
