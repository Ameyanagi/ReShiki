param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('x86', 'x64', 'arm64')]
    [string] $Arch
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

if ($env:OS -ne 'Windows_NT') {
    throw 'MSVC setup requires Windows.'
}

$before = @{}
Get-ChildItem Env: | ForEach-Object { $before[$_.Name] = $_.Value }
$previousPaths = @($env:RESHIKI_MSVC_PATH -split ';' | Where-Object { $_ })
$preservedPaths = @($env:PATH -split ';' | Where-Object { $_ -and $_ -notin $previousPaths })
$hostArch = if ($Arch -eq 'arm64' -and ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64' -or $env:PROCESSOR_ARCHITEW6432 -eq 'ARM64')) { 'arm64' } else { 'x64' }

$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
if (-not (Test-Path -LiteralPath $vswhere -PathType Leaf)) {
    throw 'Visual Studio Installer did not provide vswhere.exe.'
}
$installation = & $vswhere -latest -products '*' -property installationPath
if ($LASTEXITCODE -ne 0 -or -not $installation -or @($installation).Count -ne 1) {
    throw 'No installed Visual Studio instance was found.'
}
$module = Join-Path $installation 'Common7/Tools/Microsoft.VisualStudio.DevShell.dll'
if (-not (Test-Path -LiteralPath $module -PathType Leaf)) {
    throw "Visual Studio Developer Shell is missing: $module"
}
Import-Module $module

# The official cleanup removes the previous SDK/compiler environment. Preserve
# tools added by intervening steps instead of restoring a stale initial PATH.
if ($env:VSCMD_VER) {
    Enter-VsDevShell -VsInstallPath $installation -SkipAutomaticLocation -DevCmdArguments '-clean_env -no_logo'
}
$env:PATH = $preservedPaths -join ';'
Enter-VsDevShell -VsInstallPath $installation -SkipAutomaticLocation -DevCmdArguments "-no_logo -arch=$Arch -host_arch=$hostArch"

if ($env:VSCMD_ARG_TGT_ARCH -ne $Arch -or $env:VSCMD_ARG_HOST_ARCH -ne $hostArch) {
    throw "Visual Studio did not select the requested $hostArch-hosted $Arch tools."
}
$compiler = (Get-Command cl.exe -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
$expected = Join-Path $env:VCToolsInstallDir "bin/Host$hostArch/$Arch/cl.exe"
if ([IO.Path]::GetFullPath($compiler) -ne [IO.Path]::GetFullPath($expected)) {
    throw "Unexpected compiler on PATH: $compiler (expected $expected)"
}
$env:RESHIKI_MSVC_PATH = @($env:PATH -split ';' | Where-Object { $_ -and $_ -notin $preservedPaths }) -join ';'

if ($env:GITHUB_ENV) {
    $after = @{}
    Get-ChildItem Env: | ForEach-Object { $after[$_.Name] = $_.Value }
    $changes = [Collections.Generic.List[string]]::new()
    foreach ($name in @($before.Keys + $after.Keys | Sort-Object -Unique)) {
        if ($before[$name] -eq $after[$name]) { continue }
        if ($name -match '^(GITHUB_|RUNNER_|NODE_OPTIONS$)') {
            throw "Developer Shell unexpectedly changed a protected variable: $name"
        }
        $delimiter = 'reshiki_' + [Guid]::NewGuid().ToString('N')
        $changes.Add("$name<<$delimiter")
        $changes.Add([string] $after[$name])
        $changes.Add($delimiter)
    }
    [IO.File]::AppendAllLines($env:GITHUB_ENV, $changes, [Text.UTF8Encoding]::new($false))
}
Write-Host "MSVC target $Arch, host ${hostArch}: $compiler"
