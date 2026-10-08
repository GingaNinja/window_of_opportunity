# Runs a GUI example, screenshots its window, and exits.
#
# usage: scripts/capture-screenshot.ps1 -Exe target\release\examples\simples.exe -Out dist\screenshot.png
#
# Captures the window rect (raised to the front first), falling back to the
# full screen. The process is made DPI-aware up front so window rects and
# screen copies agree on physical pixels. Run under Windows PowerShell 5.1
# (System.Windows.Forms is .NET Framework there, no quirks).

param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [Parameter(Mandatory = $true)][string]$Out,
    [int]$SettleSeconds = 5
)

$ErrorActionPreference = "Stop"

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
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
}
"@

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
        [void][Win32Shot]::GetWindowRect($handle, [ref]$rect)
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
