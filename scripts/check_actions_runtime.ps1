# This exact body is embedded before checkout in every workflow that uses
# JavaScript actions. tests/test_ci_runtime_policy.py checks that it stays exact.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Get-ActionsNodePath {
    param([string[]] $WorkerPaths)
    if (@($WorkerPaths).Count -ne 1 -or [string]::IsNullOrWhiteSpace($WorkerPaths[0])) {
        throw 'Expected exactly one live Runner.Worker executable; refusing to use Node from PATH.'
    }
    $worker = $WorkerPaths[0]
    $workerName = if ($IsWindows) { 'Runner.Worker.exe' } else { 'Runner.Worker' }
    if (-not [IO.Path]::IsPathRooted($worker) -or (Split-Path $worker -Leaf) -cne $workerName) {
        throw "Unexpected Runner.Worker executable: $worker"
    }
    $bin = Split-Path $worker -Parent
    if ((Split-Path $bin -Leaf) -cne 'bin') {
        throw "Runner.Worker is not in its runner's bin directory: $worker"
    }
    $root = Split-Path $bin -Parent
    $name = if ($IsWindows) { 'node.exe' } else { 'node' }
    $node = [IO.Path]::GetFullPath((Join-Path $root "externals/node24/bin/$name"))
    if (-not (Test-Path -LiteralPath $node -PathType Leaf)) {
        throw "The active runner has no embedded Node 24: $node"
    }
    return $node
}

function Assert-ActionsNodeRuntime {
    param([object] $Runtime, [string] $NodePath)
    if ($Runtime.versions.node -cnotmatch '^24\.[0-9]+\.[0-9]+$' -or
        [version] $Runtime.versions.node -lt [version] '24.22.1') {
        throw "JavaScript actions require the runner's Node >=24.22.1 (24.x); found $($Runtime.versions.node). Update the runner; setup-node cannot fix this runtime."
    }
    if ($Runtime.versions.brotli -cnotmatch '^[0-9]+\.[0-9]+\.[0-9]+$') {
        throw 'The embedded Node runtime did not report its Brotli version.'
    }
    $comparison = if ($IsWindows) { [StringComparison]::OrdinalIgnoreCase } else { [StringComparison]::Ordinal }
    if (-not [string]::Equals([IO.Path]::GetFullPath($Runtime.execPath), $NodePath, $comparison)) {
        throw "The inspected runtime is not the active runner's embedded Node: $($Runtime.execPath)"
    }
    # Official Node 24.22.1 backports CVE-2026-111497 while retaining the
    # Brotli 1.2.0 version label. The Node security-release floor is authoritative:
    # https://github.com/nodejs/node/compare/v24.21.0...v24.22.1
    Write-Host "Verified Actions runtime: Node $($Runtime.versions.node), Brotli $($Runtime.versions.brotli), $NodePath"
}

$workers = @(Get-Process -Name Runner.Worker -ErrorAction Stop | ForEach-Object { $_.Path })
$node = Get-ActionsNodePath -WorkerPaths $workers
$reported = & $node -p 'JSON.stringify({execPath:process.execPath,versions:process.versions})'
if ($LASTEXITCODE -ne 0) { throw 'The active runner could not report its embedded Node version.' }
Assert-ActionsNodeRuntime -Runtime ($reported | ConvertFrom-Json) -NodePath $node
