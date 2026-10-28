$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$targetRoot = Join-Path $repoRoot "target"
if ($env:CARGO_TARGET_DIR) {
    $targetRoot = if ([IO.Path]::IsPathRooted($env:CARGO_TARGET_DIR)) {
        [IO.Path]::GetFullPath($env:CARGO_TARGET_DIR)
    } else {
        [IO.Path]::GetFullPath((Join-Path $repoRoot $env:CARGO_TARGET_DIR))
    }
}
& (Join-Path $PSScriptRoot "prepare-mcdk.ps1") -VerifyOnly
$config = Get-Content -Encoding UTF8 -Raw -LiteralPath (Join-Path $repoRoot "src-tauri\tauri.conf.json") | ConvertFrom-Json
$version = [string]$config.version
$releaseRoot = Join-Path $repoRoot "release"
$portableRoot = Join-Path $targetRoot "release\portable"
$staging = Join-Path $portableRoot "MCDH-$version"

New-Item -ItemType Directory -Force -Path $releaseRoot, $portableRoot | Out-Null
$portableResolved = (Resolve-Path -LiteralPath $portableRoot).Path
$stagingFull = [IO.Path]::GetFullPath($staging)
if (-not $stagingFull.StartsWith($portableResolved + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Portable staging path escaped its expected root: $stagingFull"
}
if (Test-Path -LiteralPath $stagingFull) {
    Remove-Item -LiteralPath $stagingFull -Recurse -Force
}
New-Item -ItemType Directory -Path $stagingFull | Out-Null

Copy-Item -LiteralPath (Join-Path $targetRoot "release\mcdh-desktop.exe") -Destination (Join-Path $stagingFull "MCDH.exe")
Copy-Item -LiteralPath (Join-Path $targetRoot "release\mcdh-mcp.exe") -Destination (Join-Path $stagingFull "mcdh-mcp.exe")
Copy-Item -LiteralPath (Join-Path $repoRoot "LICENSE") -Destination $stagingFull
Copy-Item -LiteralPath (Join-Path $repoRoot "README.md") -Destination $stagingFull
Copy-Item -LiteralPath (Join-Path $repoRoot "RELEASE_NOTES.md") -Destination $stagingFull
Copy-Item -LiteralPath (Join-Path $repoRoot "THIRD_PARTY_LICENSES.md") -Destination $stagingFull
Copy-Item -LiteralPath (Join-Path $repoRoot "src-tauri\release-resources\mcdk") -Destination (Join-Path $stagingFull "mcdk") -Recurse

$portableArchive = Join-Path $releaseRoot "MCDH-$version-windows-x64-portable.zip"
if (Test-Path -LiteralPath $portableArchive) {
    Remove-Item -LiteralPath $portableArchive -Force
}
$portableItems = @(Get-ChildItem -LiteralPath $stagingFull -Force | Select-Object -ExpandProperty FullName)
if ($portableItems.Count -eq 0) {
    throw "Portable staging directory is empty."
}
Compress-Archive -Path $portableItems -DestinationPath $portableArchive -CompressionLevel Optimal

$installer = Get-ChildItem -LiteralPath (Join-Path $targetRoot "release\bundle\nsis") -File -Filter "*_${version}_x64-setup.exe" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
if ($null -eq $installer) {
    throw "NSIS setup executable for $version was not found."
}
$installerTarget = Join-Path $releaseRoot "MCDH-$version-windows-x64-setup.exe"
Copy-Item -LiteralPath $installer.FullName -Destination $installerTarget -Force

$artifacts = @($installerTarget, $portableArchive)
$checksums = $artifacts | ForEach-Object {
    $stream = [IO.File]::OpenRead($_)
    try {
        $sha256 = [Security.Cryptography.SHA256]::Create()
        try {
            $hashBytes = $sha256.ComputeHash($stream)
        } finally {
            $sha256.Dispose()
        }
    } finally {
        $stream.Dispose()
    }
    $hash = [BitConverter]::ToString($hashBytes).Replace("-", "").ToLowerInvariant()
    "$hash  $([IO.Path]::GetFileName($_))"
}
[IO.File]::WriteAllLines((Join-Path $releaseRoot "SHA256SUMS.txt"), $checksums, [Text.UTF8Encoding]::new($false))

Write-Output "Created release artifacts in $releaseRoot"
