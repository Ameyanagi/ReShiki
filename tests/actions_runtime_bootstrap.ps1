param([string] $NodeExecutable = '', [string] $OfficialArchive = '', [string] $OriginalExecutable = '')
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$source = Join-Path $PSScriptRoot '../scripts/check_actions_runtime.ps1'
$tokens = $null; $errors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile($source, [ref] $tokens, [ref] $errors)
if ($errors.Count -ne 0) { throw "Bootstrap parse failed: $errors" }
$definitions = $ast.FindAll({ param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst] }, $true)
. ([ScriptBlock]::Create(($definitions | ForEach-Object { $_.Extent.Text }) -join "`n"))
if (-not $NodeExecutable) {
    # Hosted tests read/copy the already guarded runtime; they never patch it.
    $workers = @(Get-Process -Name Runner.Worker -ErrorAction Stop | ForEach-Object { $_.Path })
    $NodeExecutable = Get-ActionsNodePath -WorkerPaths $workers
}

function Must-Reject {
    param([scriptblock] $Action)
    $rejected = $false
    try { & $Action | Out-Null } catch { $rejected = $true }
    if (-not $rejected) { throw 'Bootstrap accepted an unsafe input.' }
}

function Write-TarFixture {
    param([string] $Path, [object[]] $Entries)
    $file = [IO.File]::Create($Path)
    $gzip = [IO.Compression.GZipStream]::new($file, [IO.Compression.CompressionMode]::Compress)
    $tar = [System.Formats.Tar.TarWriter]::new($gzip)
    try {
        foreach ($item in $Entries) {
            $entry = [System.Formats.Tar.PaxTarEntry]::new($item.Type, $item.Name)
            if ($item.Type -in @([System.Formats.Tar.TarEntryType]::SymbolicLink, [System.Formats.Tar.TarEntryType]::HardLink)) { $entry.LinkName = 'elsewhere' }
            else { $entry.DataStream = [IO.MemoryStream]::new([byte[]] @(1, 2, 3, 4)) }
            $tar.WriteEntry($entry)
        }
    } finally { $tar.Dispose(); $gzip.Dispose(); $file.Dispose() }
}

# All mutations stay under this owned fixture. No production discovery/top-level
# guard is executed, and no developer or hosted runner installation is modified.
$temporary = Join-Path $PSScriptRoot ('reshiki-bootstrap-test-' + [Guid]::NewGuid().ToString('N'))
$originalEnv = @{}
$names = @('RESHIKI_RUNNER_ENVIRONMENT', 'RESHIKI_RUNNER_OS', 'RESHIKI_RUNNER_ARCH', 'RUNNER_TEMP',
    'NODE_OPTIONS', 'NODE_PATH', 'NODE_EXTRA_CA_CERTS', 'NODE_ICU_DATA', 'LD_PRELOAD', 'LD_LIBRARY_PATH',
    'DYLD_INSERT_LIBRARIES', 'DYLD_LIBRARY_PATH', 'OPENSSL_CONF', 'OPENSSL_MODULES',
    'ACTIONS_RUNNER_FORCED_INTERNAL_NODE_VERSION', 'ACTIONS_RUNNER_FORCE_ACTIONS_NODE_VERSION',
    'ACTIONS_ALLOW_USE_UNSECURE_NODE_VERSION')
foreach ($name in $names) { $originalEnv[$name] = [Environment]::GetEnvironmentVariable($name) }
try {
    foreach ($name in $names) { [Environment]::SetEnvironmentVariable($name, $null) }
    $bin = Join-Path $temporary 'bin'
    $nodeBin = Join-Path $temporary 'externals/node24/bin'
    [IO.Directory]::CreateDirectory($bin) | Out-Null
    [IO.Directory]::CreateDirectory($nodeBin) | Out-Null
    $workerName = if ($IsWindows) { 'Runner.Worker.exe' } else { 'Runner.Worker' }
    $nodeName = if ($IsWindows) { 'node.exe' } else { 'node' }
    $script:FixtureWorker = Join-Path $bin $workerName
    $node = Join-Path $nodeBin $nodeName
    [IO.File]::WriteAllText($script:FixtureWorker, 'owned fake runner fixture')
    [IO.File]::WriteAllText($node, 'old target bytes; never executed')
    $oldHash = (Get-FileHash $node).Hash.ToLowerInvariant()
    # Current hosted runtime and pinned installation candidate are separate
    # fixtures. A future provider patch must pass no-op coverage without being
    # mistaken for the installer's intentionally exact 24.22.1 candidate.
    $current = Join-Path $temporary $(if ($IsWindows) { 'current-node.exe' } else { 'current-node' })
    [IO.File]::Copy($NodeExecutable, $current)
    if (-not $IsWindows) { [IO.File]::SetUnixFileMode($current, [IO.File]::GetUnixFileMode($NodeExecutable)) }
    $currentRuntime = Read-NodeRuntime $current
    Assert-ActionsNodeRuntime $currentRuntime $current
    $currentHash = (Get-FileHash $current).Hash.ToLowerInvariant()
    $asset = Get-NodeBootstrapAsset -Platform $currentRuntime.platform -Architecture $currentRuntime.arch
    $candidateArchive = $OfficialArchive
    if (-not $candidateArchive) {
        $candidateArchive = Join-Path $temporary 'pinned-candidate-download'
        Save-NodeArtifact $asset $candidateArchive
    }
    $staged = Join-Path $temporary $nodeName
    Expand-NodeExecutable $asset $candidateArchive $staged
    $runtime = Read-NodeRuntime -NodePath $staged
    Assert-ActionsNodeRuntime -Runtime $runtime -NodePath $staged
    if ($runtime.versions.node -cne '24.22.1' -or $runtime.platform -cne $currentRuntime.platform -or
        $runtime.arch -cne $currentRuntime.arch) { throw 'Incorrect pinned installation candidate.' }
    $stagedHash = (Get-FileHash $staged).Hash.ToLowerInvariant()
    $env:RESHIKI_RUNNER_ENVIRONMENT = 'github-hosted'
    $env:RESHIKI_RUNNER_OS = if ($IsWindows) { 'Windows' } elseif ($IsMacOS) { 'macOS' } else { 'Linux' }
    $env:RESHIKI_RUNNER_ARCH = if ($runtime.arch -ceq 'arm64') { 'ARM64' } else { 'X64' }
    $env:RUNNER_TEMP = $temporary
    Assert-HostedNodeContext $runtime $staged $env:RESHIKI_RUNNER_ENVIRONMENT $env:RESHIKI_RUNNER_OS $env:RESHIKI_RUNNER_ARCH
    Must-Reject { Assert-HostedNodeContext $runtime $staged 'self-hosted' $env:RESHIKI_RUNNER_OS $env:RESHIKI_RUNNER_ARCH }
    Must-Reject { Assert-HostedNodeContext $runtime $staged '' $env:RESHIKI_RUNNER_OS $env:RESHIKI_RUNNER_ARCH }
    Must-Reject { Assert-HostedNodeContext $runtime $staged 'github-hosted' 'wrong' $env:RESHIKI_RUNNER_ARCH }
    Must-Reject { Assert-HostedNodeContext $runtime $staged 'github-hosted' $env:RESHIKI_RUNNER_OS 'X86' }
    $wrongArch = if ($runtime.arch -ceq 'arm64') { 'X64' } else { 'ARM64' }
    Must-Reject { Assert-HostedNodeContext $runtime $staged 'github-hosted' $env:RESHIKI_RUNNER_OS $wrongArch }
    Must-Reject { Assert-HostedNodeContext $runtime $node 'github-hosted' $env:RESHIKI_RUNNER_OS $env:RESHIKI_RUNNER_ARCH }
    foreach ($platform in @('linux', 'darwin', 'win32')) {
        foreach ($architecture in @('x64', 'arm64')) {
            $selected = Get-NodeBootstrapAsset $platform $architecture
            if ($selected.SHA256 -cnotmatch '^[0-9a-f]{64}$') { throw 'Missing artifact pin.' }
        }
    }
    Must-Reject { Get-NodeBootstrapAsset 'linux' 'ia32' }
    Must-Reject { Get-NodeBootstrapAsset 'freebsd' 'x64' }
    Assert-NodeArtifactHash $staged $stagedHash
    Must-Reject { Assert-NodeArtifactHash $staged ('0' * 64) }
    Must-Reject { Assert-NodeArtifactHash $staged 'not-a-hash' }
    foreach ($name in $names | Where-Object { $_ -match '^(NODE_|LD_|DYLD_|OPENSSL_|ACTIONS_)' }) {
        [Environment]::SetEnvironmentVariable($name, 'unsafe')
        Must-Reject { Assert-NodeEnvironment }
        [Environment]::SetEnvironmentVariable($name, $null)
    }
    foreach ($name in @('ACTIONS_RUNNER_FORCED_INTERNAL_NODE_VERSION', 'ACTIONS_RUNNER_FORCE_ACTIONS_NODE_VERSION')) {
        [Environment]::SetEnvironmentVariable($name, 'node20'); Must-Reject { Assert-NodeEnvironment }
        [Environment]::SetEnvironmentVariable($name, 'node24'); Assert-NodeEnvironment
        [Environment]::SetEnvironmentVariable($name, $null)
    }
    # Exercise the real response consumer with bounded offline HTTP messages.
    foreach ($status in @(302, 404, 500)) {
        $response = [Net.Http.HttpResponseMessage]::new($status)
        $response.Content = [Net.Http.ByteArrayContent]::new([byte[]] @(1))
        Must-Reject { Receive-NodeArtifact $response (Join-Path $temporary 'bad-response') ([Threading.CancellationToken]::None) }
        $response.Dispose()
    }
    foreach ($kind in @('encoding', 'oversize', 'empty', 'partial', 'cancelled')) {
        $response = [Net.Http.HttpResponseMessage]::new(200)
        $response.Content = [Net.Http.ByteArrayContent]::new([byte[]] @(1, 2, 3))
        $cancel = [Threading.CancellationTokenSource]::new()
        switch ($kind) {
            'encoding' { $response.Content.Headers.ContentEncoding.Add('br') }
            'oversize' { $response.Content.Headers.ContentLength = 134217729 }
            'empty' { $response.Content = [Net.Http.ByteArrayContent]::new([byte[]] @()) }
            'partial' { $response.Content.Headers.ContentLength = 7 }
            'cancelled' { $cancel.Cancel() }
        }
        Must-Reject { Receive-NodeArtifact $response (Join-Path $temporary $kind) $cancel.Token }
        $cancel.Dispose(); $response.Dispose()
    }
    $response = [Net.Http.HttpResponseMessage]::new(200)
    $response.Content = [Net.Http.ByteArrayContent]::new([byte[]] @(1, 2, 3))
    Receive-NodeArtifact $response (Join-Path $temporary 'valid-response') ([Threading.CancellationToken]::None)
    $response.Dispose()
    if (([IO.File]::ReadAllBytes((Join-Path $temporary 'valid-response'))).Length -ne 3) { throw 'Response body changed.' }
    if (-not $IsWindows) {
        $prefix = $asset.Name.Substring(0, $asset.Name.Length - 7)
        $regular = [System.Formats.Tar.TarEntryType]::RegularFile
        $link = [System.Formats.Tar.TarEntryType]::SymbolicLink
        $good = @{ Name = "$prefix/bin/node"; Type = $regular }
        $cases = @(
            @($good, $good),
            @(@{ Name = "$prefix/bin/node"; Type = $link }),
            @(@{ Name = "$prefix/bin/node"; Type = [System.Formats.Tar.TarEntryType]::HardLink }),
            @(@{ Name = "$prefix/../escape"; Type = $regular }),
            @(@{ Name = '/absolute/node'; Type = $regular }),
            @(@{ Name = "$prefix/other"; Type = $regular })
        )
        $index = 0
        foreach ($entries in $cases) {
            $archive = Join-Path $temporary "bad-$index.tar.gz"
            Write-TarFixture $archive $entries
            $synthetic = [pscustomobject] @{ Name = $asset.Name; Platform = $asset.Platform; SHA256 = (Get-FileHash $archive).Hash.ToLowerInvariant() }
            Must-Reject { Expand-NodeExecutable $synthetic $archive (Join-Path $temporary "bad-$index-node") }
            $index++
        }
        $archive = Join-Path $temporary 'good.tar.gz'
        Write-TarFixture $archive @($good)
        $synthetic = [pscustomobject] @{ Name = $asset.Name; Platform = $asset.Platform; SHA256 = (Get-FileHash $archive).Hash.ToLowerInvariant() }
        Expand-NodeExecutable $synthetic $archive (Join-Path $temporary 'extracted-node')
        $linked = Join-Path $temporary 'linked'
        [IO.File]::CreateSymbolicLink($linked, $node) | Out-Null
        Must-Reject { Assert-NoLinkedPath $linked }
        [IO.File]::Delete($linked)
    }
    if ($OfficialArchive) {
        # Optional local integration of the real authenticated archive, with the
        # production hash and selective extractor, before executing its member.
        $extracted = Join-Path $temporary 'official-node'
        Expand-NodeExecutable $asset $OfficialArchive $extracted
        Assert-ActionsNodeRuntime (Read-NodeRuntime $extracted) $extracted
        if ((Get-FileHash $extracted).Hash.ToLowerInvariant() -cne $stagedHash) { throw 'Official member differs from the verified local runtime.' }
    }
    # Substitute process discovery ONLY in this test script, for our owned fake
    # worker and synthetic active-process controls. Production has no bypass.
    $script:ActiveNodes = @()
    function Get-Process {
        param([string] $Name, $ErrorAction)
        if ($Name -ceq 'Runner.Worker') { return [pscustomobject] @{ Path = $script:FixtureWorker } }
        if ($Name -ceq 'node') { return $script:ActiveNodes }
        throw 'Unexpected process query in fixture.'
    }
    $script:ActiveNodes = @([pscustomobject] @{ Path = $node })
    Must-Reject { Install-NodeExecutable $script:FixtureWorker $node $oldHash $staged $asset $stagedHash }
    $script:ActiveNodes = @([pscustomobject] @{ Path = '' })
    Must-Reject { Assert-NoActiveNodeTarget $node }
    $script:ActiveNodes = @()
    Must-Reject { Install-NodeExecutable $script:FixtureWorker $node ('0' * 64) $staged $asset $stagedHash }
    Must-Reject { Install-NodeExecutable (Join-Path $bin 'other-worker') $node $oldHash $staged $asset $stagedHash }
    $badAsset = [pscustomobject] @{ Platform = $asset.Platform; Architecture = 'wrong' }
    Must-Reject { Install-NodeExecutable $script:FixtureWorker $node $oldHash $staged $badAsset $stagedHash }
    if ((Get-FileHash $node).Hash.ToLowerInvariant() -cne $oldHash) { throw 'Rejected install modified the target.' }
    if (-not $IsWindows) {
        $mode = [IO.File]::GetUnixFileMode($nodeBin)
        try {
            [IO.File]::SetUnixFileMode($nodeBin, [IO.UnixFileMode]::UserRead -bor [IO.UnixFileMode]::UserExecute)
            Must-Reject { Install-NodeExecutable $script:FixtureWorker $node $oldHash $staged $asset $stagedHash }
        } finally { [IO.File]::SetUnixFileMode($nodeBin, $mode) }
    } else {
        $locked = [IO.File]::Open($node, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::None)
        try { Must-Reject { Install-NodeExecutable $script:FixtureWorker $node $oldHash $staged $asset $stagedHash } }
        finally { $locked.Dispose() }
    }
    Install-NodeExecutable $script:FixtureWorker $node $oldHash $staged $asset $stagedHash
    $installed = Read-NodeRuntime $node
    Assert-ActionsNodeRuntime $installed $node
    if ($OriginalExecutable) {
        # Local-only full integration: the signed/hash-verified old runtime is
        # copied into this fixture and runs only Read-NodeRuntime's fixed JSON.
        [IO.File]::Copy($OriginalExecutable, $node, $true)
        if (-not $IsWindows) { [IO.File]::SetUnixFileMode($node, [IO.File]::GetUnixFileMode($OriginalExecutable)) }
        $old = Read-NodeRuntime $node
        if ($old.versions.node -cne '24.21.0') { throw 'Expected the original official 24.21.0 fixture.' }
        Initialize-ActionsNode $script:FixtureWorker $node $old
        $installed = Read-NodeRuntime $node
        if ($installed.versions.node -cne '24.22.1') { throw 'Full initializer did not replace the old runtime.' }
    }
    function Save-NodeArtifact { throw 'A no-op must never download.' }
    [IO.File]::Copy($current, $node, $true)
    if (-not $IsWindows) { [IO.File]::SetUnixFileMode($node, [IO.File]::GetUnixFileMode($current)) }
    $installed = Read-NodeRuntime $node
    Initialize-ActionsNode $script:FixtureWorker $node $installed
    Initialize-ActionsNode $script:FixtureWorker $node (Read-NodeRuntime $node)
    foreach ($futureVersion in @('24.22.2', '24.23.0')) {
        $future = $installed | ConvertTo-Json -Depth 5 | ConvertFrom-Json
        $future.versions.node = $futureVersion
        Initialize-ActionsNode $script:FixtureWorker $node $future
        Assert-NodeArtifactHash $node $currentHash
    }
    foreach ($unsupported in @('24.20.0', '24.22.0', '22.23.4', '26.12.1', '24.22.1-rc.1', 'malformed')) {
        $bad = $installed | ConvertTo-Json -Depth 5 | ConvertFrom-Json
        $bad.versions.node = $unsupported
        Must-Reject { Initialize-ActionsNode $script:FixtureWorker $node $bad }
    }
    # Independent later launch stands in for the post-action process launch.
    Assert-ActionsNodeRuntime (Read-NodeRuntime $node) $node
    Assert-NodeArtifactHash $node $currentHash
    if (@(Get-ChildItem $nodeBin -Filter '.reshiki-node-*').Count -ne 0) { throw 'Sibling staging leaked.' }
    Write-Host 'PASS: real bootstrap functions, offline failure controls, owned sandbox install, idempotent no-network no-op and later post-style launch.'
} finally {
    foreach ($name in $names) { [Environment]::SetEnvironmentVariable($name, $originalEnv[$name]) }
    if (Test-Path $temporary) { Remove-Item -LiteralPath $temporary -Recurse -Force }
}
