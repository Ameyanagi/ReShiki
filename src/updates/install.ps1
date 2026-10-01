param([int]$ParentId, [string]$Target, [string]$Payload, [string]$Stage, [string]$Drawing, [string]$Platform)
$ErrorActionPreference = 'Stop'
$drawings = @()
if ($Drawing) { $drawings += $Drawing }
$drawings += $args
function Quote-Argument([string]$Value) {
    '"' + ([regex]::Replace($Value, '(\\*)"', '$1$1\"') -replace '(\\+)$', '$1$1') + '"'
}
function Reopen-Arguments([string[]]$Paths) {
    @($Paths | ForEach-Object { '--open'; Quote-Argument $_ }) -join ' '
}
New-Item -ItemType File -Path (Join-Path $Stage 'ready') | Out-Null
Wait-Process -Id $ParentId -Timeout 60 -ErrorAction SilentlyContinue
if (Get-Process -Id $ParentId -ErrorAction SilentlyContinue) { throw 'Application did not exit; nothing installed.' }
$installer = Start-Process -FilePath $Payload -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/SP-', ('/DIR="' + $Target + '"')) -Wait -PassThru
if ($installer.ExitCode -ne 0) { throw ('Installation failed with code ' + $installer.ExitCode + '. The installer is retained in ' + $Stage) }
$executable = Join-Path $Target 'reshiki.exe'
$arguments = Reopen-Arguments $drawings
if ($arguments) { Start-Process -FilePath $executable -ArgumentList $arguments }
else { Start-Process -FilePath $executable }
