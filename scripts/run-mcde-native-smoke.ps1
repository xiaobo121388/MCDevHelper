param([Parameter(Mandatory=$true)][string]$SetupPath, [switch]$BackendTest)
$ErrorActionPreference = 'Stop'
$workspace = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$allowed = Join-Path $workspace 'target\native-mcde-qa'
$setupFile = (Resolve-Path -LiteralPath $SetupPath).Path
if (-not $setupFile.StartsWith($allowed + '\', [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Use a setup.json produced by mcde_smoke_setup under this workspace.'
}
$setup = Get-Content -Encoding UTF8 -Raw -LiteralPath $setupFile | ConvertFrom-Json
if ($setup.case -ne 'mcdh_mcde_native_setup' -or -not $setup.ok) { throw 'Unrecognized native-test setup.' }
$fixtureRoot = [IO.Path]::GetFullPath($setup.root)
if ($fixtureRoot -ine [IO.Path]::GetDirectoryName($setupFile)) { throw 'Setup root does not match its file.' }
foreach ($name in @('root', 'data', 'project', 'appdata', 'localappdata')) {
    $path = [IO.Path]::GetFullPath($setup.$name)
    if ($name -ne 'root' -and $path -ine [IO.Path]::GetFullPath((Join-Path $fixtureRoot $name))) { throw ('Unexpected fixture path: ' + $name) }
    if (-not $path.StartsWith($allowed + '\', [StringComparison]::OrdinalIgnoreCase) -or -not (Test-Path -LiteralPath $path -PathType Container)) {
        throw ('Test path escaped its fixture root: ' + $name)
    }
    $ancestor = $path
    while ($ancestor -and $ancestor.StartsWith($workspace, [StringComparison]::OrdinalIgnoreCase)) {
        if ((Get-Item -Force -LiteralPath $ancestor).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Linked fixture paths are not permitted.' }
        $ancestor = [IO.Path]::GetDirectoryName($ancestor)
    }
}
$executable = Join-Path $workspace 'target\release\mcdh-desktop.exe'
if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) { throw 'Build MCDH with pnpm tauri build --no-bundle first.' }
& (Join-Path $PSScriptRoot 'prepare-mcde.ps1') -VerifyOnly -ResourceDirectory (Join-Path $workspace 'target\release\release-resources\mcde')
$environment = @{
    MCDH_DATA_DIR = $setup.data
    MCDH_DISABLE_MCS_SCAN = '1'
    APPDATA = $setup.appdata
    LOCALAPPDATA = $setup.localappdata
    WEBVIEW2_USER_DATA_FOLDER = (Join-Path $setup.root 'webview2')
    MCDE_QA_WORKSPACE_ROOT = $workspace
    MCDE_QA_ROAMING_APPDATA = $setup.appdata
    MCDE_QA_LOCAL_APPDATA = $setup.localappdata
    MCDEV_IS_SUBPROCESS_MODE = '0'
    MCDEV_AUTO_JOIN_GAME = $null
    MCDEV_OUTPUT_MODE = '1'
    MCDH_MCDE_QA_SETUP = $setupFile
}
$previous = @{}
try {
    foreach ($name in $environment.Keys) {
        $previous[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
        [Environment]::SetEnvironmentVariable($name, $environment[$name], 'Process')
    }
    if ($BackendTest) {
        Push-Location -LiteralPath $workspace
        try {
            & cargo test -p mcdh-desktop mcdk_test_mcde_native_backend --locked -- --ignored --nocapture --test-threads=1
            if ($LASTEXITCODE) { throw 'Native backend test failed; inspect the retained owned session and logs.' }
        } finally { Pop-Location }
    } else {
        # This is an interactive QA application, not a background helper.
        $process = Start-Process -FilePath $executable -WorkingDirectory $workspace -WindowStyle Normal -PassThru
        [PSCustomObject]@{case='mcdh_mcde_native_launch';pid=$process.Id;executable=$executable;setup=$setupFile;data=$setup.data;project=$setup.project} | ConvertTo-Json
    }
} finally {
    foreach ($name in $previous.Keys) { [Environment]::SetEnvironmentVariable($name, $previous[$name], 'Process') }
}
