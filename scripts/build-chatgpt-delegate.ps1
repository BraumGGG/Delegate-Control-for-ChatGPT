[CmdletBinding()]
param(
    [string]$OutputDirectory = "artifacts/build-chatgpt-delegate"
)

$ErrorActionPreference = "Stop"
$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$vendor = Join-Path $root "vendor\chatgpt-delegate"
$buildRoot = Join-Path $root ".build\chatgpt-delegate"
$venv = Join-Path $buildRoot "venv"
$output = Join-Path $root $OutputDirectory

if (-not (Test-Path (Join-Path $vendor "pyproject.toml"))) {
    throw "找不到固定版本的 chatgpt-delegate 源码：$vendor"
}

New-Item -ItemType Directory -Force $buildRoot, $output | Out-Null
if (-not (Test-Path (Join-Path $venv "Scripts\python.exe"))) {
    py -3 -m venv $venv
}

$python = Join-Path $venv "Scripts\python.exe"
& $python -m pip install --disable-pip-version-check --upgrade pip setuptools wheel | Out-Host
if ($LASTEXITCODE -ne 0) { throw 'Python build tooling installation failed.' }
& $python -m pip install --disable-pip-version-check "fastmcp>=2,<4" "mcp[cli]>=1.24,<2" "pyinstaller>=6,<7" | Out-Host
if ($LASTEXITCODE -ne 0) { throw 'Connector dependency installation failed.' }
$env:PYTHONPATH = Join-Path $vendor "src"
& $python -m unittest discover -s (Join-Path $vendor "tests") -p "test_*.py" | Out-Host
if ($LASTEXITCODE -ne 0) { throw 'Connector regression tests failed.' }

& $python -m PyInstaller `
    --noconfirm `
    --clean `
    --onedir `
    --name "chatgpt-delegate-edit" `
    --distpath $output `
    --workpath (Join-Path $buildRoot "pyinstaller-work") `
    --specpath (Join-Path $buildRoot "pyinstaller-spec") `
    --paths (Join-Path $vendor "src") `
    --collect-all fastmcp `
    --collect-all mcp `
    (Join-Path $vendor "build_entry.py") | Out-Host
if ($LASTEXITCODE -ne 0) { throw 'Connector packaging failed; refusing to reuse an older runtime.' }

$exe = Join-Path $output "chatgpt-delegate-edit\chatgpt-delegate-edit.exe"
if (-not (Test-Path $exe)) {
    throw "PyInstaller 未生成预期文件：$exe"
}
& $exe --help | Select-Object -First 4 | Out-Host
& $exe capabilities --json | Out-Host
Write-Output "已生成：$exe"
