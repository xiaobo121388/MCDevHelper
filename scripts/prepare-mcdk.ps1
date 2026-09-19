param([switch]$VerifyOnly)
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$manifestPath = Join-Path $repoRoot "assets\mcdk\bundled.json"
$manifest = Get-Content -Encoding UTF8 -Raw -LiteralPath $manifestPath | ConvertFrom-Json
$destination = Join-Path $repoRoot "src-tauri\release-resources\mcdk"
$binary = Join-Path $destination "mcdk.exe"

function Read-Sha256([string]$Path) {
    $stream = [IO.File]::OpenRead($Path)
    $sha = [Security.Cryptography.SHA256]::Create()
    try { return [BitConverter]::ToString($sha.ComputeHash($stream)).Replace("-", "").ToLowerInvariant() }
    finally { $sha.Dispose(); $stream.Dispose() }
}

function Assert-McdkBinary([string]$Path) {
    $file = Get-Item -LiteralPath $Path
    if ($file.Length -ne $manifest.size) { throw "Bundled MCDK size mismatch." }
    $hash = Read-Sha256 $Path
    if ($hash -ne $manifest.sha256) { throw "Bundled MCDK SHA-256 mismatch." }
    $bytes = [IO.File]::ReadAllBytes($Path)
    $offset = [BitConverter]::ToInt32($bytes, 0x3c)
    if ($bytes[0] -ne 0x4d -or $bytes[1] -ne 0x5a -or $offset -lt 0 -or $offset + 26 -gt $bytes.Length) {
        throw "Bundled MCDK is not a PE executable."
    }
    if ([BitConverter]::ToUInt32($bytes, $offset) -ne 0x4550 -or
        [BitConverter]::ToUInt16($bytes, $offset + 4) -ne 0x8664 -or
        [BitConverter]::ToUInt16($bytes, $offset + 24) -ne 0x20b) {
        throw "Bundled MCDK is not a Windows x64 executable."
    }
}

if (-not $VerifyOnly) {
    New-Item -ItemType Directory -Force -Path $destination | Out-Null
    $valid = $false
    if (Test-Path -LiteralPath $binary) {
        try { Assert-McdkBinary $binary; $valid = $true } catch { Write-Output "Refreshing invalid MCDK cache." }
    }
    if (-not $valid) {
        $temporary = Join-Path $destination ("download-" + [Guid]::NewGuid().ToString("N") + ".tmp")
        try {
            $url = "https://github.com/GitHub-Zero123/MCDevTool/releases/download/$($manifest.tag)/$($manifest.asset_name)"
            Invoke-WebRequest -UseBasicParsing -Uri $url -OutFile $temporary -TimeoutSec 120
            Assert-McdkBinary $temporary
            Move-Item -LiteralPath $temporary -Destination $binary -Force
        } finally {
            if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary }
        }
    }
    Copy-Item -LiteralPath $manifestPath -Destination (Join-Path $destination "bundled.json") -Force
    $licenses = Join-Path $destination "licenses"
    New-Item -ItemType Directory -Force -Path $licenses | Out-Null
    Get-ChildItem -LiteralPath (Join-Path $repoRoot "assets\mcdk\licenses") -File | ForEach-Object {
        Copy-Item -LiteralPath $_.FullName -Destination $licenses -Force
    }
}
Assert-McdkBinary $binary
foreach ($notice in Get-ChildItem -LiteralPath (Join-Path $repoRoot "assets\mcdk\licenses") -File) {
    $copy = Join-Path $destination ("licenses\" + $notice.Name)
    if ((Read-Sha256 $copy) -ne (Read-Sha256 $notice.FullName)) {
        throw "MCDK license notice missing or modified: $($notice.Name)"
    }
}
Write-Output "Verified bundled MCDK $($manifest.version) and license notices."
