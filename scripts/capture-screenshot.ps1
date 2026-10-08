# Runs a GUI example, screenshots its window, and exits.
#
# usage: scripts/capture-screenshot.ps1 -Exe target\release\examples\simples.exe -Out dist\screenshot.png
#
# Captures the window rect when we can find it, falling back to the full
# screen. Run under Windows PowerShell 5.1 (System.Windows.Forms is .NET
# Framework there, no quirks).

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
}
"@

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
        # Raise it so nothing overlaps it, then re-read the (possibly moved) rect.
        [void][Win32Shot]::SetForegroundWindow($handle)
        Start-Sleep -Milliseconds 500
        [void][Win32Shot]::GetWindowRect($handle, [ref]$rect)
        $origin = [System.Drawing.Point]::new($rect.Left, $rect.Top)
        $size = [System.Drawing.Size]::new($rect.Right - $rect.Left, $rect.Bottom - $rect.Top)
        Write-Host "Capturing window rect $($rect.Left),$($rect.Top) $($size.Width)x$($size.Height)"
    } else {
        Write-Host "No main window found; capturing the full screen"
    }

    $bitmap = [System.Drawing.Bitmap]::new($size.Width, $size.Height)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen($origin, [System.Drawing.Point]::Empty, $size)
        $bitmap.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
    } finally {
        $graphics.Dispose()
        $bitmap.Dispose()
    }
} finally {
    if (-not $proc.HasExited) { Stop-Process -Id $proc.Id -Force }
}

Write-Host "Saved screenshot to $Out"
