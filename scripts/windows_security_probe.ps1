# Invoked by windows_security_evidence.py. No policy changes, downloads or app execution.
param(
    [Parameter(Mandatory = $true)][ValidateSet('snapshot', 'scan', 'events')][string]$Mode,
    [Parameter(Mandatory = $true)][string]$Target,
    [string]$Since
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
$logName = 'Microsoft-Windows-Windows Defender/Operational'

function Capture([scriptblock]$Query) {
    try { @{ ok = $true; data = (& $Query) } }
    catch { @{ ok = $false; error = $_.ToString(); error_id = $_.FullyQualifiedErrorId } }
}

if ($Mode -eq 'scan') {
    # Defender retains its normal remediation and cloud/sample-submission policy.
    # A successful cmdlet return is NOT a scan verdict; the caller correlates events.
    Start-MpScan -ScanType CustomScan -ScanPath $Target
    exit 0
}

if ($Mode -eq 'events') {
    $result = Capture {
        # FilterHashtable's DateTime conversion can reinterpret UTC as local time
        # in Windows PowerShell 5.1. Compare the event's UTC SystemTime directly.
        $start = [DateTimeOffset]::Parse($Since).UtcDateTime.ToString("yyyy-MM-ddTHH:mm:ss.fffffff'Z'")
        try {
            $events = @(Get-WinEvent -LogName $logName -FilterXPath "*[System[TimeCreated[@SystemTime >= '$start']]]" -MaxEvents 4097)
        }
        catch {
            if ($_.FullyQualifiedErrorId -notlike 'NoMatchingEventsFound*') { throw }
            $events = @()
        }
        @{
            records = @($events | Select-Object -First 4096 | ForEach-Object { $_.ToXml() })
            truncated = $events.Count -gt 4096
        }
    }
}
else {
    $result = @{
        captured_utc = [DateTimeOffset]::UtcNow.ToString('o')
        host = Capture {
            $os = Get-CimInstance Win32_OperatingSystem
            $computer = Get-CimInstance Win32_ComputerSystem
            $version = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
            @{
                caption = $os.Caption; version = $os.Version; build = $os.BuildNumber
                ubr = $version.UBR; display_version = $version.DisplayVersion
                os_architecture = $os.OSArchitecture; product_type = $os.ProductType
                system_type = $computer.SystemType; hypervisor_present = $computer.HypervisorPresent
                process_architecture = $env:PROCESSOR_ARCHITECTURE
                native_architecture = $env:PROCESSOR_ARCHITEW6432
                elevated = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
                github_actions = $env:GITHUB_ACTIONS; runner_image = $env:ImageOS
                runner_image_version = $env:ImageVersion; powershell = $PSVersionTable.PSVersion.ToString()
            }
        }
        status = Capture {
            Get-MpComputerStatus | Select-Object AMProductVersion, AMEngineVersion, AMRunningMode,
                AMServiceEnabled, AntivirusEnabled, RealTimeProtectionEnabled, BehaviorMonitorEnabled,
                OnAccessProtectionEnabled, IoavProtectionEnabled, AntivirusSignatureVersion,
                AntivirusSignatureLastUpdated, AntivirusSignatureAge, DefenderSignaturesOutOfDate,
                SmartAppControlState, IsTamperProtected
        }
        preferences = Capture {
            Get-MpPreference | Select-Object DisableRealtimeMonitoring, DisableBehaviorMonitoring,
                DisableIOAVProtection, DisableArchiveScanning, ExclusionPath, ExclusionExtension,
                ExclusionProcess, MAPSReporting, SubmitSamplesConsent, PUAProtection,
                DisableBlockAtFirstSeen, CloudBlockLevel
        }
        threats = Capture {
            Get-MpThreatDetection | Select-Object * -ExcludeProperty CimClass, CimInstanceProperties, CimSystemProperties
        }
        signature = Capture {
            $signature = Get-AuthenticodeSignature -LiteralPath $Target
            @{
                status = $signature.Status.ToString(); message = $signature.StatusMessage
                signer = $signature.SignerCertificate | Select-Object Subject, Thumbprint, NotBefore, NotAfter
                timestamp_signer = $signature.TimeStamperCertificate | Select-Object Subject, Thumbprint
            }
        }
        zone_identifier = Capture { Get-Content -LiteralPath $Target -Stream Zone.Identifier -Raw }
    }
}
$result | ConvertTo-Json -Depth 12 -Compress
