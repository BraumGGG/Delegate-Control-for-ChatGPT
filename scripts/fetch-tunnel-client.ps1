[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$version = '0.0.14'
$name = "tunnel-client-v$version-windows-amd64"
$url = "https://github.com/openai/tunnel-client/releases/download/v$version/$name.zip"
$archive = Join-Path $root "artifacts/$name.zip"
$target = Join-Path $root 'artifacts/build-tunnel-client'
$archiveHash = '784AB8DA7B5A88F0109F1FD8AAF0A1C86067430B896DDDF307EF7E3CC49FA1A5'
$exeHash = 'FCC85A69EC0AD82518E4F8964F60C45E31787957782A0FC9C1B0C44E82D61B9B'
New-Item -ItemType Directory -Force $target | Out-Null
if (!(Test-Path -LiteralPath $archive)) { Invoke-WebRequest $url -OutFile $archive }
if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $archiveHash) { throw 'Tunnel archive SHA-256 mismatch.' }

# Extract only audited files; cloudflared is not part of the DCFC MCP transport.
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [IO.Compression.ZipFile]::OpenRead($archive)
try {
    foreach ($file in @('tunnel-client.exe', 'LICENSE', 'NOTICE', "$name-licenses.txt", "$name.spdx.json")) {
        $entry = $zip.GetEntry($file)
        if (!$entry) { throw "Missing Tunnel release file: $file" }
        [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, (Join-Path $target $file), $true)
    }
} finally { $zip.Dispose() }
if ((Get-FileHash -LiteralPath (Join-Path $target 'tunnel-client.exe') -Algorithm SHA256).Hash -ne $exeHash) { throw 'Tunnel executable SHA-256 mismatch.' }
[pscustomobject]@{ version=$version; source=$url; archiveSha256=$archiveHash; executableSha256=$exeHash; destination='artifacts/build-tunnel-client' } | ConvertTo-Json -Compress
