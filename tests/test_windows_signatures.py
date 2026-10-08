"""Verify Authenticode policy and temporary trust lifetime without changing local trust."""

import base64
import hashlib
import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
HELPER = ROOT / "scripts/windows_signatures.ps1"
POWERSHELL = shutil.which("pwsh")
VERSION = "1.2.3-nightly.20261009.12345.1"
PIN = hashlib.sha256(bytes([1, 2, 3])).hexdigest()


def ps_string(value):
    return "'" + str(value).replace("'", "''") + "'"


def run_powershell(command, *, timeout=30):
    encoded = base64.b64encode(command.encode("utf-16-le")).decode("ascii")
    return subprocess.run(
        [POWERSHELL, "-NoLogo", "-NoProfile", "-NonInteractive", "-EncodedCommand", encoded],
        capture_output=True,
        text=True,
        encoding="utf-8-sig",
        timeout=timeout,
        check=True,
    )


def load_functions():
    # Load actual function bodies through PowerShell's parser. The script's CLI
    # is not executed, and all OS/signature/store effects are replaced below.
    return f"""
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$tokens = $null
$errors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile({ps_string(HELPER)}, [ref]$tokens, [ref]$errors)
if ($errors.Count -ne 0) {{ throw ($errors | Out-String) }}
$ast.FindAll({{ param($node) $node -is [System.Management.Automation.Language.FunctionDefinitionAst] }}, $true) |
    ForEach-Object {{ Invoke-Expression $_.Extent.Text }}
"""


@unittest.skipUnless(POWERSHELL, "PowerShell 7 required")
class SignaturePolicyTests(unittest.TestCase):
    def test_powershell_syntax_and_native_certificate_helper_compile(self):
        command = (
            load_functions()
            + """
$ast.FindAll({ param($node)
    $node -is [System.Management.Automation.Language.CommandAst] -and $node.GetCommandName() -eq 'Add-Type'
}, $true) | ForEach-Object { Invoke-Expression $_.Extent.Text }
if (-not ('ReShiki.SignatureCertificate' -as [type])) { throw 'Native helper did not compile' }
"""
        )
        run_powershell(command)

    def scenario(self, case, policy="test-signing", hosted=True):
        with tempfile.TemporaryDirectory() as temporary:
            target = Path(temporary) / "fixture [literal].exe"
            target.write_bytes(b"inert artifact fixture")
            command = (
                load_functions()
                + f"""
$case = {ps_string(case)}
$global:events = [System.Collections.Generic.List[string]]::new()
$global:trusted = ($case -eq 'preexisting')
$global:signatureReads = 0
$certificate = [pscustomobject]@{{ RawData = [byte[]](1, 2, 3); Subject = 'CN=Pinned fixture'; Thumbprint = ('A' * 40) }}
$timestamp = [pscustomobject]@{{ Subject = 'CN=Timestamp fixture' }}
function Test-WindowsHost {{ $true }}
function Test-SelfSignedCertificate {{
    $global:events.Add('self-signed')
    $case -ne 'not-self-signed'
}}
function Get-TargetVersionInfo {{
    [pscustomobject]@{{
        ProductName = $(if ($case -eq 'wrong-product') {{ 'Other product' }} else {{ 'ReShiki' }})
        ProductVersion = $(if ($case -eq 'wrong-version') {{ '1.2.3' }} else {{ {ps_string(VERSION)} }})
        FileVersion = $(if ($case -eq 'wrong-file-version') {{ '1.2.3' }} else {{ {ps_string(VERSION)} }})
    }}
}}
function Get-EmbeddedSignature {{
    $global:signatureReads += 1
    $signer = $certificate
    if ($case -eq 'changed-signer' -and $global:signatureReads -gt 1) {{
        $signer = [pscustomobject]@{{ RawData = [byte[]](4, 5, 6) }}
    }}
    [pscustomobject]@{{
        SignerCertificate = $signer
        SignatureType = $(if ($case -eq 'catalog') {{ 'Catalog' }} else {{ 'Authenticode' }})
        Status = $(if ($case -eq 'tampered') {{ 'HashMismatch' }}
            elseif ($case -eq 'bad-chain' -or ({ps_string(policy)} -eq 'test-signing' -and -not $global:trusted)) {{ 'NotTrusted' }}
            else {{ 'Valid' }})
        StatusMessage = 'fixture status'
        TimeStamperCertificate = $(if ($case -eq 'missing-timestamp' -or
            ($case -eq 'lost-timestamp' -and $global:signatureReads -gt 1)) {{ $null }} else {{ $timestamp }})
    }}
}}
function Open-TestRootStore {{
    $global:events.Add('open')
    $store = [pscustomobject]@{{ Certificates = $(if ($global:trusted) {{ @($certificate) }} else {{ @() }}) }}
    $store | Add-Member -MemberType ScriptMethod -Name Add -Value {{
        param($certificate)
        $global:events.Add('add')
        $global:trusted = $true
        if ($case -eq 'add-failure') {{ throw 'partially failed Add' }}
    }}
    $store | Add-Member -MemberType ScriptMethod -Name Remove -Value {{
        param($certificate)
        $global:events.Add('remove')
        $global:trusted = $false
    }}
    $store | Add-Member -MemberType ScriptMethod -Name Dispose -Value {{ $global:events.Add('dispose') }}
    $store
}}
$env:GITHUB_ACTIONS = $(if ({"$true" if hosted else "$false"}) {{ 'true' }} else {{ 'false' }})
$env:RUNNER_ENVIRONMENT = $(if ({"$true" if hosted else "$false"}) {{ 'github-hosted' }} else {{ 'self-hosted' }})
$pin = $(if ($case -eq 'wrong-pin') {{ '0' * 64 }} else {{ {ps_string(PIN)} }})
$result = $null
$errorMessage = $null
try {{
    $result = Invoke-SignatureVerification -LiteralPath {ps_string(target)} -SigningPolicy {ps_string(policy)} `
        -ExpectedSha256 $pin -ExpectedVersion {ps_string(VERSION)}
}}
catch {{ $errorMessage = $_.Exception.Message }}
@{{ result = $result; error = $errorMessage; events = @($global:events); trusted = $global:trusted }} |
    ConvertTo-Json -Depth 8 -Compress
"""
            )
            return json.loads(run_powershell(command).stdout)

    def test_test_signing_temporarily_trusts_only_pinned_certificate(self):
        result = self.scenario("valid")
        self.assertIsNone(result["error"])
        self.assertEqual(result["events"], ["self-signed", "open", "add", "remove", "dispose"])
        self.assertFalse(result["trusted"])
        self.assertEqual(result["result"]["certificate_sha256"], PIN)
        self.assertEqual(result["result"]["timestamp_subject"], "CN=Timestamp fixture")
        self.assertEqual(result["result"]["policy"], "test-signing")
        self.assertEqual(
            result["result"]["file_sha256"], hashlib.sha256(b"inert artifact fixture").hexdigest()
        )

    def test_existing_trust_is_preserved_and_release_policy_never_opens_store(self):
        existing = self.scenario("preexisting")
        self.assertIsNone(existing["error"])
        self.assertEqual(existing["events"], ["self-signed", "open", "dispose"])
        self.assertTrue(existing["trusted"])
        release = self.scenario("valid", policy="release-signing", hosted=False)
        self.assertIsNone(release["error"])
        self.assertEqual(release["events"], [])
        self.assertEqual(release["result"]["policy"], "release-signing")

    def test_identity_metadata_timestamp_and_host_guards_precede_trust(self):
        for case in (
            "wrong-pin",
            "catalog",
            "tampered",
            "wrong-product",
            "wrong-version",
            "wrong-file-version",
            "missing-timestamp",
        ):
            with self.subTest(case=case):
                result = self.scenario(case)
                self.assertIsNone(result["result"])
                self.assertIsNotNone(result["error"])
                self.assertEqual(result["events"], [])
        result = self.scenario("valid", hosted=False)
        self.assertIn("GitHub-hosted", result["error"])
        self.assertEqual(result["events"], [])
        not_self_signed = self.scenario("not-self-signed")
        self.assertIn("self-signed", not_self_signed["error"])
        self.assertEqual(not_self_signed["events"], ["self-signed"])

    def test_temporary_trust_is_removed_after_each_later_failure(self):
        for case in ("bad-chain", "changed-signer", "lost-timestamp", "add-failure"):
            with self.subTest(case=case):
                result = self.scenario(case)
                self.assertIsNone(result["result"])
                self.assertIsNotNone(result["error"])
                self.assertEqual(
                    result["events"], ["self-signed", "open", "add", "remove", "dispose"]
                )
                self.assertFalse(result["trusted"])


@unittest.skipUnless(
    POWERSHELL
    and os.name == "nt"
    and os.environ.get("GITHUB_ACTIONS") == "true"
    and os.environ.get("RUNNER_ENVIRONMENT") == "github-hosted",
    "Real certificate-store integration requires GitHub-hosted Windows CI",
)
class WindowsAuthenticodeTests(unittest.TestCase):
    def test_real_pe_signature_rejections_and_root_cleanup(self):
        with tempfile.TemporaryDirectory() as temporary:
            folder = Path(temporary)
            resource = folder / "fixture.rc"
            resource.write_text(
                f'''1 VERSIONINFO
FILEVERSION 1,2,3,0
PRODUCTVERSION 1,2,3,0
FILEFLAGSMASK 0x3f
FILEFLAGS 0
FILEOS 0x40004
FILETYPE 1
FILESUBTYPE 0
BEGIN
BLOCK "StringFileInfo"
BEGIN
BLOCK "040904B0"
BEGIN
VALUE "ProductName", "ReShiki\\0"
VALUE "ProductVersion", "{VERSION}\\0"
VALUE "FileVersion", "{VERSION}\\0"
END
END
BLOCK "VarFileInfo"
BEGIN
VALUE "Translation", 0x0409, 1200
END
END
''',
                encoding="ascii",
            )
            source = folder / "fixture.cs"
            source.write_text("class Fixture { static void Main() {} }", encoding="ascii")
            command = (
                load_functions()
                + rf"""
$folder = {ps_string(folder)}
$rc = Get-ChildItem -LiteralPath "${{env:ProgramFiles(x86)}}\Windows Kits\10\bin" -Filter rc.exe -Recurse |
    Where-Object {{ $_.FullName -match '\\x64\\rc.exe$' }} | Sort-Object FullName -Descending | Select-Object -First 1
$csc = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
if ($null -eq $rc -or -not (Test-Path -LiteralPath $csc)) {{ throw 'Windows SDK resource compiler and .NET Framework compiler required' }}
$target = Join-Path $folder 'fixture.exe'
$res = Join-Path $folder 'fixture.res'
& $rc.FullName /nologo /fo $res (Join-Path $folder 'fixture.rc') | Out-Null
if ($LASTEXITCODE -ne 0) {{ throw 'Fixture resource compilation failed' }}
& $csc /nologo /target:exe "/out:$target" "/win32res:$res" (Join-Path $folder 'fixture.cs') | Out-Null
if ($LASTEXITCODE -ne 0) {{ throw 'Fixture PE compilation failed' }}
$certificate = New-SelfSignedCertificate -Type CodeSigningCert -Subject 'CN=ReShiki verifier integration fixture' `
    -CertStoreLocation 'Cert:\CurrentUser\My' -NotAfter (Get-Date).AddDays(2)
$pin = Get-CertificateSha256 $certificate
function Assert-RootAbsent {{
    $store = Open-TestRootStore
    try {{
        $matches = @($store.Certificates | Where-Object {{ (Get-CertificateSha256 $_) -eq $pin }})
        if ($matches.Count -ne 0) {{ throw 'Verifier left its fixture certificate trusted' }}
    }} finally {{ $store.Dispose() }}
}}
function Assert-Rejected([scriptblock]$Action, [string]$Message) {{
    $rejected = $false
    try {{ & $Action | Out-Null }}
    catch {{
        if ($_.Exception.Message -notlike "*$Message*") {{ throw }}
        $rejected = $true
    }}
    if (-not $rejected) {{ throw "Verifier accepted negative fixture: $Message" }}
    Assert-RootAbsent
}}
try {{
    Set-AuthenticodeSignature -LiteralPath $target -Certificate $certificate -HashAlgorithm SHA256 | Out-Null
    Assert-Rejected {{ Invoke-SignatureVerification $target 'test-signing' ('0' * 64) {ps_string(VERSION)} }} 'SHA-256 pin'
    Assert-Rejected {{ & {ps_string(HELPER)} -Path $target -Policy test-signing -CertificateSha256 $pin -ProductVersion {ps_string(VERSION)} }} 'timestamp'
    # This local signature has no external timestamp. Inject only timestamp
    # presence while preserving real Windows hash/chain results to exercise
    # Root lifetime without a network-dependent timestamp-service test.
    function Get-EmbeddedSignature([string]$LiteralPath) {{
        $actual = Get-AuthenticodeSignature -LiteralPath $LiteralPath
        [pscustomobject]@{{
            SignerCertificate = $actual.SignerCertificate; SignatureType = $actual.SignatureType
            Status = $actual.Status; StatusMessage = $actual.StatusMessage
            TimeStamperCertificate = $certificate
        }}
    }}
    Assert-Rejected {{ Invoke-SignatureVerification $target 'test-signing' $pin 'wrong-version' }} 'release identity'
    $result = Invoke-SignatureVerification $target 'test-signing' $pin {ps_string(VERSION)}
    if ($result.certificate_sha256 -ne $pin) {{ throw 'Wrong certificate result' }}
    Assert-RootAbsent
    $tampered = Join-Path $folder 'tampered.exe'
    Copy-Item -LiteralPath $target -Destination $tampered
    $bytes = [IO.File]::ReadAllBytes($tampered)
    $bytes[1024] = $bytes[1024] -bxor 1
    [IO.File]::WriteAllBytes($tampered, $bytes)
    Assert-Rejected {{ Invoke-SignatureVerification $tampered 'test-signing' $pin {ps_string(VERSION)} }} 'hash mismatch'
    function Get-EmbeddedSignature([string]$LiteralPath) {{
        $actual = Get-AuthenticodeSignature -LiteralPath $LiteralPath
        [pscustomobject]@{{
            SignerCertificate = $actual.SignerCertificate; SignatureType = $actual.SignatureType
            Status = 'NotTrusted'; StatusMessage = 'Injected post-import chain failure'
            TimeStamperCertificate = $certificate
        }}
    }}
    Assert-Rejected {{ Invoke-SignatureVerification $target 'test-signing' $pin {ps_string(VERSION)} }} 'verification failed'
}} finally {{
    # Fixture cleanup also runs when an assertion reveals a helper regression.
    $store = Open-TestRootStore
    try {{ $store.Remove($certificate) }} finally {{ $store.Dispose() }}
    Remove-Item -LiteralPath ('Cert:\CurrentUser\My\' + $certificate.Thumbprint) -DeleteKey
}}
'Real Windows Authenticode rejection and temporary Root cleanup checks passed.'
"""
            )
            result = run_powershell(command, timeout=120)
            self.assertIn("checks passed", result.stdout)


if __name__ == "__main__":
    unittest.main()
