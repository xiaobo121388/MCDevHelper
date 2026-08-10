$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$cargoPath = Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"
$rustcPath = Join-Path $env:USERPROFILE ".cargo\bin\rustc.exe"
if (-not (Test-Path -LiteralPath $cargoPath -PathType Leaf)) {
    $cargoPath = (Get-Command cargo -ErrorAction Stop).Source
}
if (-not (Test-Path -LiteralPath $rustcPath -PathType Leaf)) {
    $rustcPath = (Get-Command rustc -ErrorAction Stop).Source
}

$targetTriple = (& $rustcPath --print host-tuple).Trim()
if ($targetTriple -ne "x86_64-pc-windows-msvc") {
    throw "MCDH v0.1 supports x86_64-pc-windows-msvc only; current host is $targetTriple."
}

Push-Location $repoRoot
try {
    & $cargoPath build -p mcdh-mcp --release --locked
    if ($LASTEXITCODE -ne 0) {
        throw "The mcdh-mcp release build failed."
    }
} finally {
    Pop-Location
}

$sourceBinary = Join-Path $repoRoot "target\release\mcdh-mcp.exe"
$binaryDirectory = Join-Path $repoRoot "src-tauri\binaries"
$resourceDirectory = Join-Path $repoRoot "src-tauri\release-resources"
New-Item -ItemType Directory -Force -Path $binaryDirectory, $resourceDirectory | Out-Null
Copy-Item -LiteralPath $sourceBinary -Destination (Join-Path $binaryDirectory "mcdh-mcp-$targetTriple.exe") -Force
Copy-Item -LiteralPath (Join-Path $repoRoot "LICENSE") -Destination (Join-Path $resourceDirectory "LICENSE") -Force
Copy-Item -LiteralPath (Join-Path $repoRoot "README.md") -Destination (Join-Path $resourceDirectory "README.md") -Force
Copy-Item -LiteralPath (Join-Path $repoRoot "THIRD_PARTY_LICENSES.md") -Destination (Join-Path $resourceDirectory "THIRD_PARTY_LICENSES.md") -Force

Write-Output "Prepared mcdh-mcp sidecar for $targetTriple"
