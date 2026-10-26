# Embedded before checkout; setup-node does not change JavaScript actions' Node.
$ErrorActionPreference = 'Stop'
$workers = @(Get-Process -Name Runner.Worker -ErrorAction Stop)
if ($workers.Count -ne 1) { throw 'Expected exactly one live Runner.Worker.' }
$root = Split-Path (Split-Path $workers[0].Path -Parent) -Parent
$name = if ($IsWindows) { 'node.exe' } else { 'node' }
$node = Join-Path $root "externals/node24/bin/$name"
$version = & $node --version
if ($LASTEXITCODE -ne 0 -or $version -cnotmatch '^v24\.[0-9]+\.[0-9]+$') {
    throw 'Could not read the active runner Node 24 version.'
}
Write-Host "Actions runtime before bootstrap: $version ($node)"
if ([version] $version.Substring(1) -lt [version] '24.22.1') {
    # Official signed SHASUMS256.txt provenance is in actions_node_bootstrap.json.
    $assets = @{
        'macOS-ARM64' = @('node-v24.22.1-darwin-arm64.tar.gz', '89c682593d76cd44233fa53b1563264e7de9cbb189cad169135c5b4fadfbc0a7')
        'macOS-X64' = @('node-v24.22.1-darwin-x64.tar.gz', 'd132557e7ee2b85af08b0c8c34338e65c6430f2238d01f727728ef42dc83b035')
        'Linux-ARM64' = @('node-v24.22.1-linux-arm64.tar.gz', '33c37be234ba8bee041961047b19160f4d7d0f164e8e246f78bdd10feff148b3')
        'Linux-X64' = @('node-v24.22.1-linux-x64.tar.gz', 'eb6728e77c576a35162c122c45ccadf224d89ff00abc8dae46cc2897c6002d7c')
        'Windows-ARM64' = @('win-arm64/node.exe', '8bea91dc437fe01e7d92ef0efc6c9eaf654369aaba9b2d3e9841552ea7b95a00')
        'Windows-X64' = @('win-x64/node.exe', 'd702daa815eabbff763c7d3808e235cf031cf2ca70211b56847bc72f51e240e5')
    }
    $asset = $assets["$env:RUNNER_OS-$env:RUNNER_ARCH"]
    if ($null -eq $asset) { throw 'Unsupported runner OS/architecture.' }
    $temporary = Join-Path $env:RUNNER_TEMP ('reshiki-node-' + [Guid]::NewGuid())
    New-Item -ItemType Directory -Path $temporary | Out-Null
    try {
        $download = Join-Path $temporary 'node-download'
        Invoke-WebRequest -Uri "https://nodejs.org/dist/v24.22.1/$($asset[0])" -OutFile $download
        if ((Get-FileHash -LiteralPath $download -Algorithm SHA256).Hash -ine $asset[1]) {
            throw 'Official Node artifact SHA256 mismatch.'
        }
        if ($IsWindows) {
            $candidate = $download # The pinned Windows asset is the raw node.exe.
        } else {
            & tar -xzf $download -C $temporary
            if ($LASTEXITCODE -ne 0) { throw 'Official Node archive extraction failed.' }
            $candidate = Join-Path $temporary ($asset[0].Replace('.tar.gz', '') + '/bin/node')
        }
        Copy-Item -LiteralPath $candidate -Destination $node -Force
    } finally {
        Remove-Item -LiteralPath $temporary -Recurse -Force
    }
    $version = & $node --version
    if ($LASTEXITCODE -ne 0 -or $version -cne 'v24.22.1') {
        throw 'The installed Actions Node did not report v24.22.1.'
    }
}
Write-Host "Verified Actions runtime: $version ($node)"
