# Run in an interactive Windows desktop session after building with --release.
# Measures process CPU for a fixed pointer/zoom workload, not frame latency.
param(
    [Parameter(Mandatory = $true)][string]$Executable,
    [string]$OutputDirectory = 'artifacts/windows-renderer-benchmark'
)
$ErrorActionPreference = 'Stop'
$Executable = (Resolve-Path $Executable).Path
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$root = (Resolve-Path $OutputDirectory).Path
$previousBackend = $env:ICED_BACKEND
$previousDataDirectory = $env:RESHIKI_DATA_DIR
$runId = [Guid]::NewGuid().ToString('N')
Start-Transcript -Path "$root/desktop-benchmark.log" -Force
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class GpuBenchWin32 {
    private delegate bool EnumWindowProc(IntPtr hwnd, IntPtr parameter);
    [DllImport("user32.dll")] private static extern bool EnumWindows(EnumWindowProc callback, IntPtr parameter);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);
    [DllImport("user32.dll")] private static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] private static extern int GetWindowText(IntPtr hwnd, StringBuilder text, int count);
    public static string Title(IntPtr hwnd) {
        var text = new StringBuilder(512);
        GetWindowText(hwnd, text, text.Capacity);
        return text.ToString();
    }
    public static IntPtr EditorWindow(int processId) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((hwnd, parameter) => {
            uint owner;
            GetWindowThreadProcessId(hwnd, out owner);
            if (owner == processId && IsWindowVisible(hwnd) && Title(hwnd).StartsWith("Shortcut examples")) {
                found = hwnd;
                return false;
            }
            return true;
        }, IntPtr.Zero);
        return found;
    }
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hwnd);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out RECT rect);
    [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr hwnd, int x, int y, int width, int height, bool repaint);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, int data, UIntPtr extra);
    [DllImport("user32.dll")] public static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extra);
}
'@
[GpuBenchWin32]::SetProcessDPIAware() | Out-Null
$foreground = [GpuBenchWin32]::GetForegroundWindow()
$pointer = [System.Windows.Forms.Cursor]::Position
$results = @()
$cores = (Get-CimInstance Win32_ComputerSystem).NumberOfLogicalProcessors
try {
    & $Executable --graphics-info | Set-Content "$root/desktop-graphics-info.json"
    $modes = @('tiny-skia', 'wgpu', 'auto', 'wgpu', 'tiny-skia')
    for ($trial = 0; $trial -lt $modes.Count; $trial++) {
        $mode = $modes[$trial]
        if ($mode -eq 'auto') { Remove-Item Env:ICED_BACKEND -ErrorAction SilentlyContinue }
        else { $env:ICED_BACKEND = $mode }
        $env:RESHIKI_DATA_DIR = "$root/data-$runId-$trial"
        New-Item -ItemType Directory -Path $env:RESHIKI_DATA_DIR -Force | Out-Null
        # Keep release-check network activity out of the rendering comparison.
        Set-Content -Path "$env:RESHIKI_DATA_DIR/update-preferences.json" -Value 'false' -Encoding ASCII
        $proc = Start-Process -FilePath $Executable -ArgumentList '--shortcut-examples' -WorkingDirectory $root -PassThru -RedirectStandardError "$root/stderr-$trial.log"
        try {
            $hwnd = [IntPtr]::Zero
            for ($i = 0; $i -lt 150; $i++) {
                Start-Sleep -Milliseconds 200
                $proc.Refresh()
                if ($proc.HasExited) { throw "Renderer $mode exited: $(Get-Content "$root/stderr-$trial.log" -Raw)" }
                # WGPU creates temporary graphics windows before the editor.
                $hwnd = [GpuBenchWin32]::EditorWindow($proc.Id)
                if ($hwnd -ne [IntPtr]::Zero) { break }
            }
            if ($hwnd -eq [IntPtr]::Zero) { throw 'No benchmark editor window' }
            [GpuBenchWin32]::MoveWindow($hwnd, 30, 30, 1280, 820, $true) | Out-Null
            (New-Object -ComObject WScript.Shell).AppActivate($proc.Id) | Out-Null
            [GpuBenchWin32]::SetForegroundWindow($hwnd) | Out-Null
            [GpuBenchWin32]::SetCursorPos(40, 40) | Out-Null
            Start-Sleep -Seconds 8
            if ([GpuBenchWin32]::GetForegroundWindow() -ne $hwnd) {
                throw 'The benchmark editor did not receive focus'
            }
            $rect = New-Object GpuBenchWin32+RECT
            [GpuBenchWin32]::GetWindowRect($hwnd, [ref]$rect) | Out-Null
            $proc.Refresh()
            $cpuBefore = $proc.TotalProcessorTime.TotalSeconds
            $timer = [Diagnostics.Stopwatch]::StartNew()
            Start-Sleep -Seconds 8
            $timer.Stop()
            $proc.Refresh()
            $idleCpu = $proc.TotalProcessorTime.TotalSeconds - $cpuBefore
            $idleWall = $timer.Elapsed.TotalSeconds
            $cpuBefore = $proc.TotalProcessorTime.TotalSeconds
            # ReShiki uses Ctrl+wheel for zoom; plain wheel movement pans.
            [GpuBenchWin32]::keybd_event(0x11, 0, 0, [UIntPtr]::Zero)
            $timer.Restart()
            for ($step = 0; $step -lt 300; $step++) {
                if (($step % 30) -eq 0 -and [GpuBenchWin32]::GetForegroundWindow() -ne $hwnd) {
                    throw 'Focus changed during the benchmark'
                }
                $x = $rect.Left + 350 + (($step * 31) % 400)
                $y = $rect.Top + 280 + (($step * 17) % 250)
                [GpuBenchWin32]::SetCursorPos($x, $y) | Out-Null
                if (($step % 12) -eq 0) {
                    $wheel = if (($step % 24) -eq 0) { 120 } else { -120 }
                    [GpuBenchWin32]::mouse_event(0x0800, 0, 0, $wheel, [UIntPtr]::Zero)
                }
                Start-Sleep -Milliseconds 33
            }
            $timer.Stop()
            $proc.Refresh()
            $activeCpu = $proc.TotalProcessorTime.TotalSeconds - $cpuBefore
            $activeWall = $timer.Elapsed.TotalSeconds
            [GpuBenchWin32]::keybd_event(0x11, 0, 2, [UIntPtr]::Zero)
            if (![GpuBenchWin32]::Title($hwnd).StartsWith('Shortcut examples')) {
                throw 'The benchmark editor window changed'
            }
            $results += [pscustomobject]@{
                trial = $trial; backend = $mode; process_id = $proc.Id; session_id = $proc.SessionId
                title = [GpuBenchWin32]::Title($hwnd); logical_processors = $cores
                idle_wall_seconds = $idleWall; idle_cpu_seconds = $idleCpu
                idle_cpu_percent = 100 * $idleCpu / $idleWall / $cores
                active_wall_seconds = $activeWall; active_cpu_seconds = $activeCpu
                active_cpu_percent = 100 * $activeCpu / $activeWall / $cores
                working_set_mb = $proc.WorkingSet64 / 1MB
                width = $rect.Right - $rect.Left; height = $rect.Bottom - $rect.Top
                dpi = [GpuBenchWin32]::GetDpiForWindow($hwnd)
                foreground_matches = ([GpuBenchWin32]::GetForegroundWindow() -eq $hwnd)
            }
            $results | ConvertTo-Json -Depth 5 | Set-Content "$root/benchmark.json"
            $results[-1] | Format-List
            Start-Sleep -Seconds 1
            $bitmap = New-Object System.Drawing.Bitmap(($rect.Right-$rect.Left), ($rect.Bottom-$rect.Top))
            $graphics = [Drawing.Graphics]::FromImage($bitmap)
            $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bitmap.Size)
            $bitmap.Save("$root/desktop-$trial-$mode.png", [Drawing.Imaging.ImageFormat]::Png)
            $graphics.Dispose()
            $bitmap.Dispose()
        } finally {
            [GpuBenchWin32]::keybd_event(0x11, 0, 2, [UIntPtr]::Zero)
            if (!$proc.HasExited) {
                $proc.CloseMainWindow() | Out-Null
                if (!$proc.WaitForExit(3000)) { $proc.Kill() }
            }
        }
    }
    'complete' | Set-Content "$root/benchmark-status.txt"
} catch {
    $_ | Out-String | Set-Content "$root/benchmark-error.txt"
    throw
} finally {
    [GpuBenchWin32]::SetCursorPos($pointer.X, $pointer.Y) | Out-Null
    [GpuBenchWin32]::SetForegroundWindow($foreground) | Out-Null
    $env:ICED_BACKEND = $previousBackend
    $env:RESHIKI_DATA_DIR = $previousDataDirectory
    Stop-Transcript
}
