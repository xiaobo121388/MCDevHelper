$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$cargoPath = Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"
if (-not (Test-Path -LiteralPath $cargoPath -PathType Leaf)) {
    $cargoPath = (Get-Command cargo -ErrorAction Stop).Source
}

Push-Location $repoRoot
try {
    $cargoMetadata = (& $cargoPath metadata --format-version 1 --locked --filter-platform x86_64-pc-windows-msvc | ConvertFrom-Json)
    if ($LASTEXITCODE -ne 0) {
        throw "Could not read Cargo dependency metadata."
    }
    $nodeLicenses = (pnpm licenses list --prod --json | ConvertFrom-Json)
    if ($LASTEXITCODE -ne 0) {
        throw "Could not read pnpm dependency licenses."
    }
} finally {
    Pop-Location
}

$rustRows = $cargoMetadata.packages |
    Where-Object { $_.source -ne $null } |
    ForEach-Object { [pscustomobject]@{ Name = $_.name; Version = $_.version; License = if ($_.license) { $_.license } else { "Not declared" } } } |
    Sort-Object Name, Version -Unique

$nodeRows = foreach ($licenseProperty in $nodeLicenses.PSObject.Properties) {
    foreach ($package in $licenseProperty.Value) {
        foreach ($version in $package.versions) {
            [pscustomobject]@{ Name = $package.name; Version = $version; License = $licenseProperty.Name }
        }
    }
}
$nodeRows = $nodeRows | Sort-Object Name, Version -Unique

$lines = [Collections.Generic.List[string]]::new()
$lines.Add("# MCDH Third-Party License Manifest")
$lines.Add("")
$lines.Add("This generated manifest records the declared licenses of locked Rust packages for the Windows release and production npm packages. Copyright remains with each upstream author. Consult the package registry and upstream repository for complete license text and source links.")
$lines.Add("")
$lines.Add("Generation command: ``pnpm license-report``")
$lines.Add("")
$lines.Add("## Rust runtime dependencies")
$lines.Add("")
$lines.Add("| Package | Version | Declared license |")
$lines.Add("| --- | --- | --- |")
foreach ($row in $rustRows) {
    $lines.Add("| $($row.Name) | $($row.Version) | $($row.License) |")
}
$lines.Add("")
$lines.Add("## Frontend runtime dependencies")
$lines.Add("")
$lines.Add("| Package | Version | Declared license |")
$lines.Add("| --- | --- | --- |")
foreach ($row in $nodeRows) {
    $lines.Add("| $($row.Name) | $($row.Version) | $($row.License) |")
}
$lines.Add("")
$lines.Add("## Notes")
$lines.Add("")
$lines.Add("- MCDH does not ship official Minecraft trademark assets and does not copy MCDevTool or BDSAddonManager source code.")
$lines.Add("- Windows WebView2 is supplied by the operating system and is not redistributed in the portable package.")
$lines.Add("- Regenerate and review this manifest whenever a dependency lockfile changes.")

[IO.File]::WriteAllLines((Join-Path $repoRoot "THIRD_PARTY_LICENSES.md"), $lines, [Text.UTF8Encoding]::new($false))
Write-Output "Updated THIRD_PARTY_LICENSES.md"
