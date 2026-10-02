[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$version = "0.4.3"
$assetUrl = "https://github.com/joshrotenberg/mcp-proxy/releases/download/v$version/mcp-proxy-x86_64-pc-windows-msvc.zip"
$expectedArchiveSha256 = "227BC94EB8C4F164F95A19FE38739CAC0396E95B1C5FEB9F02EB9E5B31D8B32C"
$expectedExeSha256 = "A1019FDA27715A984C1127DE90AC4398867984FFD8191C8CC9E4DA61B7D75CBC"
$cacheRoot = Join-Path $root "artifacts\mcp-proxy-v$version"
$archive = Join-Path $root "artifacts\mcp-proxy-v$version.zip"
$targetRoot = Join-Path $root "artifacts\build-mcp-proxy"
$targetExe = Join-Path $targetRoot "mcp-proxy.exe"
$sha256 = [System.Security.Cryptography.SHA256]::Create()
function Get-Sha256([string]$path) {
    $bytes = [System.IO.File]::ReadAllBytes($path)
    return ([System.BitConverter]::ToString($sha256.ComputeHash($bytes)) -replace "-", "").ToUpperInvariant()
}

New-Item -ItemType Directory -Force -Path (Split-Path $archive), $targetRoot | Out-Null
if (-not (Test-Path $archive)) {
    Write-Host "Downloading mcp-proxy v$version..."
    Invoke-WebRequest -Uri $assetUrl -OutFile $archive -UseBasicParsing
}

$archiveHash = Get-Sha256 $archive
if ($archiveHash -ne $expectedArchiveSha256) {
    throw "mcp-proxy archive hash mismatch: expected $expectedArchiveSha256, got $archiveHash"
}

if (-not (Test-Path (Join-Path $cacheRoot "mcp-proxy.exe"))) {
    if (Test-Path $cacheRoot) { Remove-Item -LiteralPath $cacheRoot -Recurse -Force }
    Expand-Archive -Path $archive -DestinationPath $cacheRoot -Force
}

$sourceExe = Join-Path $cacheRoot "mcp-proxy.exe"
if (-not (Test-Path $sourceExe)) { throw "mcp-proxy.exe is missing from the pinned release archive." }
$exeHash = Get-Sha256 $sourceExe
if ($exeHash -ne $expectedExeSha256) {
    throw "mcp-proxy.exe hash mismatch: expected $expectedExeSha256, got $exeHash"
}
foreach ($license in @("LICENSE-APACHE", "LICENSE-MIT")) {
    if (-not (Test-Path (Join-Path $cacheRoot $license))) { throw "Required license file is missing: $license" }
}

Copy-Item -LiteralPath $sourceExe -Destination $targetExe -Force
Copy-Item -LiteralPath (Join-Path $cacheRoot "LICENSE-APACHE") -Destination (Join-Path $targetRoot "LICENSE-APACHE") -Force
Copy-Item -LiteralPath (Join-Path $cacheRoot "LICENSE-MIT") -Destination (Join-Path $targetRoot "LICENSE-MIT") -Force
Copy-Item -LiteralPath (Join-Path $cacheRoot "CHANGELOG.md") -Destination (Join-Path $targetRoot "CHANGELOG.md") -Force

[pscustomobject]@{
    version = $version
    source = $assetUrl
    archiveSha256 = $archiveHash
    executableSha256 = $exeHash
    destination = "artifacts/build-mcp-proxy"
} | ConvertTo-Json -Compress
