$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$source = Join-Path $PSScriptRoot '../scripts/check_actions_runtime.ps1'
$tokens = $null
$errors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile($source, [ref] $tokens, [ref] $errors)
if ($errors.Count -ne 0) { throw "Guard parse failed: $errors" }
# Execute the actual function definitions, without running host discovery on a
# developer machine. There is no test/bypass switch in the production guard.
$definitions = $ast.FindAll({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] }, $true)
. ([ScriptBlock]::Create(($definitions | ForEach-Object { $_.Extent.Text }) -join "`n"))

function Must-Reject {
    param([scriptblock] $Action)
    $rejected = $false
    try { & $Action | Out-Null } catch { $rejected = $true }
    if (-not $rejected) { throw 'Accepted an unsafe or ambiguous runtime.' }
}

$temporary = Join-Path ([IO.Path]::GetTempPath()) ('reshiki-actions-test-' + [Guid]::NewGuid().ToString('N'))
try {
    $bin = Join-Path $temporary 'bin'
    $external = Join-Path $temporary 'externals/node24/bin'
    New-Item -ItemType Directory -Path $bin, $external -Force | Out-Null
    $workerName = if ($IsWindows) { 'Runner.Worker.exe' } else { 'Runner.Worker' }
    $nodeName = if ($IsWindows) { 'node.exe' } else { 'node' }
    $worker = Join-Path $bin $workerName
    $node = [IO.Path]::GetFullPath((Join-Path $external $nodeName))
    New-Item -ItemType File -Path $worker, $node | Out-Null
    if ((Get-ActionsNodePath -WorkerPaths @($worker)) -cne $node) { throw 'Wrong active-runner Node path.' }
    Must-Reject { Get-ActionsNodePath -WorkerPaths @() }
    Must-Reject { Get-ActionsNodePath -WorkerPaths @($worker, $worker) }
    Must-Reject { Get-ActionsNodePath -WorkerPaths @('') }
    Must-Reject { Get-ActionsNodePath -WorkerPaths @('bin/Runner.Worker') }
    Must-Reject { Get-ActionsNodePath -WorkerPaths @((Join-Path $temporary $workerName)) }
    Must-Reject { Get-ActionsNodePath -WorkerPaths @((Join-Path $bin 'unrelated-process')) }
    Remove-Item -LiteralPath $node
    Must-Reject { Get-ActionsNodePath -WorkerPaths @($worker) }

    $runtime = [pscustomobject] @{ execPath = $node; versions = [pscustomobject] @{ node = '24.22.1'; brotli = '1.2.0' } }
    # Security backport with the original library label must be accepted.
    Assert-ActionsNodeRuntime -Runtime $runtime -NodePath $node
    $runtime.versions.node = '24.23.0'
    $runtime.versions.brotli = '1.2.1'
    Assert-ActionsNodeRuntime -Runtime $runtime -NodePath $node
    foreach ($unsafe in @('24.21.0', '24.22.0', '22.23.4', '26.12.1', '24.22.1-rc.1', '24.22.1garbage', '')) {
        $runtime.versions.node = $unsafe
        Must-Reject { Assert-ActionsNodeRuntime -Runtime $runtime -NodePath $node }
    }
    $runtime.versions.node = '24.22.1'
    $runtime.versions.brotli = ''
    Must-Reject { Assert-ActionsNodeRuntime -Runtime $runtime -NodePath $node }
    $runtime.versions.brotli = '1.2.0'
    $runtime.execPath = Join-Path $temporary $nodeName
    Must-Reject { Assert-ActionsNodeRuntime -Runtime $runtime -NodePath $node }
    $runtime.execPath = $node
    if ($IsWindows) {
        Assert-ActionsNodeRuntime -Runtime $runtime -NodePath $node.ToUpperInvariant()
    } else {
        Must-Reject { Assert-ActionsNodeRuntime -Runtime $runtime -NodePath $node.ToUpperInvariant() }
    }
    Write-Host 'PASS: real guard path/version functions reject missing, ambiguous, wrong-path, old and malformed runtimes; accept the official backport.'
}
finally {
    Remove-Item -LiteralPath $temporary -Recurse -Force
}
