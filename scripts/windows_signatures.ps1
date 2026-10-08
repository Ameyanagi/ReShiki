# Verify the downloaded artifact without running it. Temporary test trust is
# restricted to an ephemeral GitHub-hosted Windows runner and always removed.
param(
    [Parameter(Mandatory = $true)][ValidateNotNullOrEmpty()][string]$Path,
    [Parameter(Mandatory = $true)][ValidateSet('test-signing', 'release-signing')][string]$Policy,
    [Parameter(Mandatory = $true)][ValidatePattern('\A[0-9a-fA-F]{64}\z')][string]$CertificateSha256,
    [Parameter(Mandatory = $true)][ValidateNotNullOrEmpty()][string]$ProductVersion
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
Set-StrictMode -Version Latest
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)

function Test-WindowsHost {
    [Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT
}

function Get-CertificateSha256($Certificate) {
    $sha256 = [System.Security.Cryptography.SHA256]::Create()
    try {
        [BitConverter]::ToString($sha256.ComputeHash($Certificate.RawData)).Replace('-', '').ToLowerInvariant()
    }
    finally { $sha256.Dispose() }
}

function Get-EmbeddedSignature([string]$LiteralPath) {
    Get-AuthenticodeSignature -LiteralPath $LiteralPath
}

function Get-TargetVersionInfo([string]$LiteralPath) {
    [System.Diagnostics.FileVersionInfo]::GetVersionInfo($LiteralPath)
}

function Assert-SignatureIdentity($Signature, [string]$ExpectedSha256) {
    if ($null -eq $Signature.SignerCertificate) {
        throw 'The artifact has no Authenticode signer certificate.'
    }
    if ($Signature.SignatureType.ToString() -cne 'Authenticode') {
        throw 'An embedded Authenticode signature is required; catalog signatures are not accepted.'
    }
    # The DER pin is checked before any certificate-store access or native helper
    # loading. A subject name or SHA-1 thumbprint does not establish this identity.
    $actual = Get-CertificateSha256 $Signature.SignerCertificate
    if ($actual -cne $ExpectedSha256.ToLowerInvariant()) {
        throw "The Authenticode signer certificate does not match its SHA-256 pin: $actual"
    }
    if ($Signature.Status.ToString() -ceq 'HashMismatch') {
        throw 'The Authenticode signature reports a file hash mismatch.'
    }
    if ($null -eq $Signature.TimeStamperCertificate) {
        throw 'A cryptographic Authenticode timestamp is required.'
    }
}

function Test-SelfSignedCertificate($Certificate) {
    if ([Convert]::ToBase64String($Certificate.SubjectName.RawData) -cne
        [Convert]::ToBase64String($Certificate.IssuerName.RawData)) {
        return $false
    }
    if (-not ('ReShiki.SignatureCertificate' -as [type])) {
        # Verify the certificate signature with its own public key. Matching
        # subject and issuer names alone only proves that a cert is self-issued.
        Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace ReShiki {
    public static class SignatureCertificate {
        [DllImport("crypt32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool CryptVerifyCertificateSignatureEx(
            IntPtr provider, uint encoding, uint subjectType, IntPtr subject,
            uint issuerType, IntPtr issuer, uint flags, IntPtr extra);
    }
}
'@
    }
    # X509_ASN_ENCODING=1; subject/issuer types CERT=2; disable MD2/MD4=1.
    [ReShiki.SignatureCertificate]::CryptVerifyCertificateSignatureEx(
        [IntPtr]::Zero, 1, 2, $Certificate.Handle, 2, $Certificate.Handle, 1, [IntPtr]::Zero)
}

function Open-TestRootStore {
    # LocalMachine avoids the interactive CurrentUser Root import dialog. A
    # hosted runner has administrator access; no elevation/prompt is attempted.
    $store = [System.Security.Cryptography.X509Certificates.X509Store]::new(
        [System.Security.Cryptography.X509Certificates.StoreName]::Root,
        [System.Security.Cryptography.X509Certificates.StoreLocation]::LocalMachine)
    try {
        $store.Open([System.Security.Cryptography.X509Certificates.OpenFlags]::ReadWrite)
        return $store
    }
    catch {
        $store.Dispose()
        throw
    }
}

function Invoke-SignatureVerification(
    [string]$LiteralPath, [string]$SigningPolicy, [string]$ExpectedSha256, [string]$ExpectedVersion
) {
    if (-not (Test-WindowsHost)) { throw 'Authenticode verification requires Windows.' }
    $SigningPolicy = $SigningPolicy.ToLowerInvariant()
    if ($SigningPolicy -notin @('test-signing', 'release-signing')) {
        throw 'Unsupported signing policy.'
    }
    if ($SigningPolicy -ceq 'test-signing' -and
        ($env:GITHUB_ACTIONS -cne 'true' -or $env:RUNNER_ENVIRONMENT -cne 'github-hosted')) {
        throw 'Test-signing verification is restricted to GitHub-hosted Windows CI.'
    }
    $file = Get-Item -LiteralPath $LiteralPath
    if ($file.PSIsContainer) { throw 'The artifact must be a file.' }
    $LiteralPath = $file.FullName
    $beforeHash = (Get-FileHash -LiteralPath $LiteralPath -Algorithm SHA256).Hash.ToLowerInvariant()
    $signature = Get-EmbeddedSignature $LiteralPath
    Assert-SignatureIdentity $signature $ExpectedSha256
    $metadata = Get-TargetVersionInfo $LiteralPath
    if ($metadata.ProductName -cne 'ReShiki' -or
        $metadata.ProductVersion -cne $ExpectedVersion -or
        $metadata.FileVersion -cne $ExpectedVersion) {
        throw 'Windows ProductName, ProductVersion, or FileVersion does not match the release identity.'
    }

    $store = $null
    $added = $false
    $certificate = $signature.SignerCertificate
    try {
        if ($SigningPolicy -ceq 'test-signing') {
            if (-not (Test-SelfSignedCertificate $certificate)) {
                throw 'Test-signing requires the pinned certificate to be cryptographically self-signed.'
            }
            $store = Open-TestRootStore
            $existing = @($store.Certificates | Where-Object {
                (Get-CertificateSha256 $_) -ceq $ExpectedSha256.ToLowerInvariant()
            })
            if ($existing.Count -eq 0) {
                # Mark before Add so cleanup also covers a partially failed Add.
                $added = $true
                $store.Add($certificate)
            }
            $signature = Get-EmbeddedSignature $LiteralPath
            Assert-SignatureIdentity $signature $ExpectedSha256
        }
        if ($signature.Status.ToString() -cne 'Valid') {
            throw "Authenticode verification failed: $($signature.Status): $($signature.StatusMessage)"
        }
        $fileHash = (Get-FileHash -LiteralPath $LiteralPath -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($fileHash -cne $beforeHash) { throw 'The artifact changed during signature verification.' }
        $result = [ordered]@{
            certificate_sha256 = Get-CertificateSha256 $signature.SignerCertificate
            subject = $signature.SignerCertificate.Subject
            thumbprint = $signature.SignerCertificate.Thumbprint.ToLowerInvariant()
            timestamp_subject = $signature.TimeStamperCertificate.Subject
            policy = $SigningPolicy
            file_sha256 = $fileHash
        }
    }
    finally {
        if ($null -ne $store) {
            try {
                if ($added) { $store.Remove($certificate) }
            }
            finally { $store.Dispose() }
        }
    }
    return $result
}

Invoke-SignatureVerification -LiteralPath $Path -SigningPolicy $Policy `
    -ExpectedSha256 $CertificateSha256 -ExpectedVersion $ProductVersion |
    ConvertTo-Json -Depth 4 -Compress
