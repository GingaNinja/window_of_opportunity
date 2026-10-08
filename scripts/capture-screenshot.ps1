# Runs a GUI example, screenshots its window, and exits.
#
# usage: scripts/capture-screenshot.ps1 -Exe target\release\examples\simples.exe -Out dist\screenshot.png
#
# Fits the window inside the work area, raises it, and captures its visible
# frame (falling back to the full screen). The process is made DPI-aware up
# front so window rects and screen copies agree on physical pixels. Run
# under Windows PowerShell 5.1 (System.Windows.Forms is .NET Framework
# there, no quirks).

param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [Parameter(Mandatory = $true)][string]$Out,
    [int]$SettleSeconds = 5
)

$ErrorActionPreference = "Stop"

# Bitmap.Save throws a cryptic "generic error occurred in GDI+" when the
# output directory doesn't exist.
$outDir = Split-Path -Parent $Out
if ($outDir) { [void][System.IO.Directory]::CreateDirectory($outDir) }

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
if ("Win32Shot" -as [type]) {} else {
Add-Type @"
using System;
using System.Runtime.InteropServices;

public static class Win32Shot {
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }

    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);

    [DllImport("user32.dll")]
    public static extern bool SetForegroundWindow(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern bool SetProcessDPIAware();

    [DllImport("user32.dll")]
    public static extern bool SetProcessDpiAwarenessContext(IntPtr value);

    [DllImport("user32.dll")]
    public static extern IntPtr GetForegroundWindow();

    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);

    [DllImport("kernel32.dll")]
    public static extern uint GetCurrentThreadId();

    [DllImport("user32.dll")]
    public static extern bool AttachThreadInput(uint idAttach, uint idAttachTo, bool fAttach);

    [DllImport("user32.dll")]
    public static extern bool BringWindowToTop(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);

    [DllImport("dwmapi.dll")]
    public static extern int DwmGetWindowAttribute(IntPtr hWnd, int attr, out RECT rect, int size);

    [DllImport("user32.dll")]
    public static extern bool SetWindowPos(IntPtr hWnd, IntPtr insertAfter, int x, int y, int cx, int cy, uint flags);
}
"@
}

# Make every coordinate in this script physical pixels. Without this, a
# DPI-scaled desktop (125%/150%) virtualises coordinates for the unaware
# process and the capture lands in the wrong place (window cut off at the
# bottom-right). Must run before any window/graphics API is used.
if (-not [Win32Shot]::SetProcessDpiAwarenessContext([IntPtr]::new(-4))) {
    [void][Win32Shot]::SetProcessDPIAware() # pre-1703: system-wide fallback
}

# A uniform image means the capture gave us nothing useful — treat it as a
# failed capture and try another.
function Test-UniformImage([System.Drawing.Bitmap]$bmp) {
    if ($bmp.Width -lt 12 -or $bmp.Height -lt 12) { return $false }
    # (parenthesised: the comma operator binds tighter than '-', so the
    # sample coordinates are computed into variables first)
    $right = $bmp.Width - 6
    $bottom = $bmp.Height - 6
    $midX = [int]($bmp.Width / 2)
    $midY = [int]($bmp.Height / 2)
    $corner = $bmp.GetPixel(0, 0).ToArgb()
    foreach ($p in @(@(5, 5), @(5, $bottom), @($right, 5), @($right, $bottom), @($midX, $midY))) {
        if ($bmp.GetPixel($p[0], $p[1]).ToArgb() -ne $corner) { return $false }
    }
    return $true
}

# The window's VISIBLE frame. GetWindowRect includes the invisible resize
# borders - those pixels show whatever is behind the window (the strips in
# earlier captures), while DWM's extended frame bounds is exact at any DPI,
# so no fixed edge trimming is needed. Falls back to GetWindowRect where DWM
# is unavailable.
function Get-VisibleRect([IntPtr]$handle) {
    $rect = New-Object Win32Shot+RECT
    if ([Win32Shot]::DwmGetWindowAttribute($handle, 9, [ref]$rect, 16) -ne 0) { # DWMWA_EXTENDED_FRAME_BOUNDS
        [void][Win32Shot]::GetWindowRect($handle, [ref]$rect)
    }
    return $rect
}

$proc = Start-Process -FilePath $Exe -PassThru
try {
    # Give the app time to create and lay out its window.
    Start-Sleep -Seconds $SettleSeconds
    $proc.Refresh()
    if ($proc.HasExited) {
        throw "$Exe exited early (code $($proc.ExitCode)) - refusing to screenshot a dead app"
    }

    # Default to the whole screen, narrowed to the window rect if we find one.
    $screen = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
    $origin = $screen.Location
    $size = $screen.Size

    $rect = New-Object Win32Shot+RECT
    $handle = $proc.MainWindowHandle
    if ($handle -ne [IntPtr]::Zero -and [Win32Shot]::GetWindowRect($handle, [ref]$rect)) {
        # Fit the window inside the work area first: on small displays (the
        # CI runner is 1024x768) the window overlaps the taskbar and the
        # capture would include it. The example is resizable and re-lays
        # itself out via on_resize.
        $work = [System.Windows.Forms.Screen]::PrimaryScreen.WorkingArea
        $margin = 8
        [void][Win32Shot]::GetWindowRect($handle, [ref]$rect)
        $curW = $rect.Right - $rect.Left
        $curH = $rect.Bottom - $rect.Top
        $fitW = [Math]::Min($curW, $work.Width - 2 * $margin)
        $fitH = [Math]::Min($curH, $work.Height - 2 * $margin)
        if ($fitW -ne $curW -or $fitH -ne $curH) {
            Write-Host "Window ${curW}x${curH} doesn't fit the work area; resizing to ${fitW}x${fitH}"
        }
        [void][Win32Shot]::SetWindowPos($handle, [IntPtr]::Zero, # SWP_NOZORDER | SWP_NOACTIVATE
            $work.X + $margin, $work.Y + $margin, $fitW, $fitH, 0x14)
        Start-Sleep -Milliseconds 500 # let the app re-render at the new size

        # Raise it so nothing overlaps the capture. SetForegroundWindow alone
        # is subject to the foreground lock when the caller isn't the
        # foreground process (it silently fails), so borrow the foreground
        # thread's input access first — the classic reliable steal.
        [void][Win32Shot]::ShowWindow($handle, 9) # SW_RESTORE: no-op unless minimised
        if ([Win32Shot]::GetForegroundWindow() -ne $handle) {
            $ignoredPid = [uint32]0
            $fgThread = [Win32Shot]::GetWindowThreadProcessId([Win32Shot]::GetForegroundWindow(), [ref]$ignoredPid)
            $ourThread = [Win32Shot]::GetCurrentThreadId()
            [void][Win32Shot]::AttachThreadInput($ourThread, $fgThread, $true)
            [void][Win32Shot]::SetForegroundWindow($handle)
            [void][Win32Shot]::BringWindowToTop($handle)
            [void][Win32Shot]::AttachThreadInput($ourThread, $fgThread, $false)
        }
        Start-Sleep -Milliseconds 500
        $rect = Get-VisibleRect $handle
        $origin = [System.Drawing.Point]::new($rect.Left, $rect.Top)
        $size = [System.Drawing.Size]::new($rect.Right - $rect.Left, $rect.Bottom - $rect.Top)
        Write-Host "Capturing window rect $($rect.Left),$($rect.Top) $($size.Width)x$($size.Height)"
    } else {
        Write-Host "No main window found; capturing the full screen"
    }

    # Capture the window rect when there is one, else the full screen. With
    # the process DPI-aware (above), GetWindowRect and CopyFromScreen agree
    # on physical pixels, so the framing is exact on scaled displays too.
    $bitmap = $null
    if ($handle -ne [IntPtr]::Zero) {
        $bitmap = [System.Drawing.Bitmap]::new($size.Width, $size.Height)
        $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
        try {
            $graphics.CopyFromScreen($origin, [System.Drawing.Point]::Empty, $size)
        } finally {
            $graphics.Dispose()
        }
        if (Test-UniformImage $bitmap) {
            Write-Host "Window capture came back blank; falling back to the full screen"
            $bitmap.Dispose()
            $bitmap = $null
        }
    }

    if ($null -eq $bitmap) {
        $bitmap = [System.Drawing.Bitmap]::new($screen.Width, $screen.Height)
        $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
        try {
            $graphics.CopyFromScreen($screen.Location, [System.Drawing.Point]::Empty, $screen.Size)
        } finally {
            $graphics.Dispose()
        }
    }

    $bitmap.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
    $bitmap.Dispose()
} finally {
    if (-not $proc.HasExited) { Stop-Process -Id $proc.Id -Force }
}

Write-Host "Saved screenshot to $Out"
