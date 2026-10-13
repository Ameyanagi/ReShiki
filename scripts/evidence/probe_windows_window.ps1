param(
    [Parameter(Mandatory = $true)]
    [int]$EditorProcessId,
    [string]$SourceCommit = ''
)

# Run in the same interactive Windows session as the editor. SSH can use a
# different window station and report different monitors or no editor HWND.
# This probe reads window/display state; it never moves or resizes a window.
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class ReShikiWindowProbe {
    [StructLayout(LayoutKind.Sequential)]
    public struct Rect { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)]
    public struct MonitorInfo {
        public uint Size;
        public Rect Monitor, Work;
        public uint Flags;
    }
    [DllImport("user32.dll")]
    public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")]
    public static extern IntPtr MonitorFromWindow(IntPtr window, uint flags);
    [DllImport("user32.dll")]
    public static extern bool GetMonitorInfo(IntPtr monitor, ref MonitorInfo info);
    [DllImport("user32.dll")]
    public static extern uint GetDpiForWindow(IntPtr window);
    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")]
    public static extern bool GetClientRect(IntPtr window, out Rect rect);
}
'@

$editor = Get-Process -Id $EditorProcessId
if ($editor.ProcessName -ne 'reshiki') {
    throw 'Select the ReShiki editor process.'
}
$window = $editor.MainWindowHandle
if ($window -eq 0) {
    throw 'No editor window is available in this session. Run this probe in the interactive editor session.'
}

# Use physical coordinates for this probe thread without changing the editor's
# process or its DPI awareness. Restore the original thread context on exit.
$previous = [ReShikiWindowProbe]::SetThreadDpiAwarenessContext([IntPtr](-4))
if ($previous -eq [IntPtr]::Zero) {
    throw 'Could not enable physical-coordinate window queries.'
}
try {
    $outer = New-Object ReShikiWindowProbe+Rect
    $client = New-Object ReShikiWindowProbe+Rect
    $monitor = New-Object ReShikiWindowProbe+MonitorInfo
    $monitor.Size = [Runtime.InteropServices.Marshal]::SizeOf($monitor)
    $monitorHandle = [ReShikiWindowProbe]::MonitorFromWindow($window, 2)
    if (-not [ReShikiWindowProbe]::GetWindowRect($window, [ref]$outer) -or
        -not [ReShikiWindowProbe]::GetClientRect($window, [ref]$client) -or
        -not [ReShikiWindowProbe]::GetMonitorInfo($monitorHandle, [ref]$monitor)) {
        throw 'Could not read editor window or monitor bounds.'
    }
    $dpi = [ReShikiWindowProbe]::GetDpiForWindow($window)
    if ($dpi -eq 0) { throw 'Could not read editor window DPI.' }
    $scale = $dpi / 96.0
    $work = $monitor.Work
    [pscustomobject]@{
        Timestamp = (Get-Date).ToString('o')
        SourceCommit = $SourceCommit
        WindowsVersion = [Environment]::OSVersion.Version.ToString()
        ProcessId = $editor.Id
        SessionId = $editor.SessionId
        Executable = $editor.Path
        ExecutableSha256 = (Get-FileHash $editor.Path -Algorithm SHA256).Hash.ToLowerInvariant()
        Dpi = $dpi
        Scale = $scale
        OuterPhysical = $outer
        ClientPhysical = $client
        ClientLogical = @(
            (($client.Right - $client.Left) / $scale),
            (($client.Bottom - $client.Top) / $scale)
        )
        MonitorPhysical = $monitor.Monitor
        WorkPhysical = $work
        FitsWorkArea = $outer.Left -ge $work.Left -and
            $outer.Top -ge $work.Top -and
            $outer.Right -le $work.Right -and
            $outer.Bottom -le $work.Bottom
    } | ConvertTo-Json -Depth 4
} finally {
    [void][ReShikiWindowProbe]::SetThreadDpiAwarenessContext($previous)
}
