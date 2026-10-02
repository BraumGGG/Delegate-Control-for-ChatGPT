[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$runtimeRoot = Join-Path $root "artifacts\build-chatgpt-delegate\chatgpt-delegate-edit"
$runtimeExe = Join-Path $runtimeRoot "chatgpt-delegate-edit.exe"

if (-not (Test-Path $runtimeExe)) {
    Write-Host "Managed Connector runtime is missing; building the onedir resource..."
    & (Join-Path $PSScriptRoot "build-chatgpt-delegate.ps1")
}

if (-not (Test-Path $runtimeExe)) {
    throw "Managed Connector runtime was not produced: $runtimeExe"
}

& (Join-Path $PSScriptRoot "fetch-mcp-proxy.ps1")

Push-Location $root
try {
    & npm.cmd run check:managed-runtime
    if ($LASTEXITCODE -ne 0) { throw "Managed runtime integrity check failed." }
    & npm.cmd run check:managed-proxy
    if ($LASTEXITCODE -ne 0) { throw "Managed MCP Proxy integrity check failed." }
    & npm.cmd run tauri -- build
    if ($LASTEXITCODE -ne 0) { throw "Tauri production build failed." }
}
finally {
    Pop-Location
}
