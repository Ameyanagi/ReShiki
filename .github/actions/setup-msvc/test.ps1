param(
    [string] $Artifacts = (Join-Path ([IO.Path]::GetTempPath()) ('reshiki-msvc-test-' + [Guid]::NewGuid().ToString('N')))
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
New-Item -ItemType Directory -Path $Artifacts -Force | Out-Null
$source = Join-Path $Artifacts 'probe.c'
[IO.File]::WriteAllText($source, "#include <windows.h>`nint main(void) { return sizeof(DWORD) == 4 ? 0 : 1; }`n")
$marker = Join-Path $Artifacts 'intervening-tool'
New-Item -ItemType Directory -Path $marker -Force | Out-Null
$originalDirectory = (Get-Location).Path
$originalGitHubEnv = $env:GITHUB_ENV
$env:GITHUB_ENV = Join-Path $Artifacts 'github-env.txt'

try {
    # Cross-target compilation makes stale PATH/SDK selection observable even
    # on an x64 developer machine without the ARM64 toolchain installed.
    foreach ($arch in @('x64', 'x86', 'x64')) {
        & (Join-Path $PSScriptRoot 'setup.ps1') -Arch $arch
        if ((Get-Location).Path -ne $originalDirectory) { throw 'Setup changed the current directory.' }
        if ($arch -eq 'x86' -and $marker -notin ($env:PATH -split ';')) { throw 'Setup lost an intervening PATH entry.' }
        $object = Join-Path $Artifacts "$arch.obj"
        $program = Join-Path $Artifacts "$arch.exe"
        & cl.exe /nologo /WX $source "/Fo$object" "/Fe$program" | Out-Host
        if ($LASTEXITCODE -ne 0) { throw "The $arch compiler or SDK failed." }
        $machine = [BitConverter]::ToUInt16([IO.File]::ReadAllBytes($object), 0)
        $expected = if ($arch -eq 'x64') { 0x8664 } else { 0x014c }
        if ($machine -ne $expected) { throw "Wrong COFF machine: $machine, expected $expected." }
        & $program
        if ($LASTEXITCODE -ne 0) { throw "The $arch program failed." }
        $env:PATH = "$marker;$env:PATH"
    }
    if ($marker -notin ($env:PATH -split ';')) { throw 'The final switch lost an intervening PATH entry.' }
    $exported = [IO.File]::ReadAllText($env:GITHUB_ENV)
    foreach ($name in @('PATH', 'INCLUDE', 'LIB', 'VSCMD_ARG_TGT_ARCH', 'RESHIKI_MSVC_PATH')) {
        if ($exported -notmatch "(?im)^$name<<reshiki_") { throw "GitHub environment is missing $name." }
    }
    $programFiles = ${env:ProgramFiles(x86)}
    $missingToolsRejected = $false
    try {
        ${env:ProgramFiles(x86)} = Join-Path $Artifacts 'missing-visual-studio'
        & (Join-Path $PSScriptRoot 'setup.ps1') -Arch x64
    }
    catch {
        if ($_.Exception.Message -notlike '*did not provide vswhere.exe*') { throw }
        $missingToolsRejected = $true
    }
    finally {
        ${env:ProgramFiles(x86)} = $programFiles
    }
    if (-not $missingToolsRejected) { throw 'Setup accepted missing Visual Studio tools.' }
    Write-Host 'PASS: x64 -> x86 -> x64; compiler, SDK, COFF machine, execution, PATH, working directory, environment export, and missing-tool rejection.'
}
finally {
    $env:GITHUB_ENV = $originalGitHubEnv
}
