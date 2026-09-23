param([int]$ParentId, [string]$Target, [string]$Payload, [string]$Stage, [string]$Drawing, [string]$Platform)
$ErrorActionPreference = 'Stop'
New-Item -ItemType File -Path (Join-Path $Stage 'ready') | Out-Null
Wait-Process -Id $ParentId -Timeout 60 -ErrorAction SilentlyContinue
if (Get-Process -Id $ParentId -ErrorAction SilentlyContinue) { throw 'Application did not exit; nothing installed.' }
$installer = Start-Process -FilePath $Payload -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/SP-', ('/DIR="' + $Target + '"')) -Wait -PassThru
if ($installer.ExitCode -ne 0) { throw ('Installation failed with code ' + $installer.ExitCode + '. The installer is retained in ' + $Stage) }
$executable = Join-Path $Target 'reshiki.exe'
if ($Drawing) { Start-Process -FilePath $executable -ArgumentList @('--open', ('"' + $Drawing + '"')) }
else { Start-Process -FilePath $executable }
