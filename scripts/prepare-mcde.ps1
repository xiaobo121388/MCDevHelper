[CmdletBinding()]
param(
    [string]$EditorRoot,
    [string]$McdkSourceDirectory,
    [string]$EditorBuildDirectory = "build\completion",
    [switch]$VerifyOnly,
    [string]$ResourceDirectory
)
$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$lockPath = Join-Path $repoRoot "assets\mcde\host-source.json"
$sourceLock = Get-Content -Encoding UTF8 -Raw -LiteralPath $lockPath | ConvertFrom-Json
$resourceParent = Join-Path $repoRoot "src-tauri\release-resources"
$destination = Join-Path $resourceParent "mcde"
if ($ResourceDirectory) {
    if (-not $VerifyOnly) { throw "-ResourceDirectory is supported only with -VerifyOnly." }
    $destination = [IO.Path]::GetFullPath($ResourceDirectory)
}

function Assert-ContainedPath([string]$Path, [string]$Root) {
    $full = [IO.Path]::GetFullPath($Path)
    $parent = [IO.Path]::GetFullPath($Root).TrimEnd('\', '/')
    if (-not $full.StartsWith($parent + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Path escaped its expected workspace root: $full"
    }
    $ancestor = $full
    while ($ancestor) {
        if (Test-Path -LiteralPath $ancestor) {
            if ((Get-Item -Force -LiteralPath $ancestor).Attributes -band [IO.FileAttributes]::ReparsePoint) {
                throw "Reparse points are not allowed in resource/build paths: $ancestor"
            }
        }
        $ancestor = [IO.Path]::GetDirectoryName($ancestor)
    }
    return $full
}

function Get-TreeFiles([string]$Root) {
    $pending = [Collections.Generic.Queue[string]]::new()
    $pending.Enqueue((Get-Item -Force -LiteralPath $Root).FullName)
    while ($pending.Count) {
        $directory = Get-Item -Force -LiteralPath $pending.Dequeue()
        if (-not $directory.PSIsContainer -or ($directory.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw "Expected a regular directory, not a reparse point: $($directory.FullName)"
        }
        foreach ($item in Get-ChildItem -Force -LiteralPath $directory.FullName) {
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Reparse point in resource input: $($item.FullName)" }
            if ($item.PSIsContainer) { $pending.Enqueue($item.FullName) } else { $item }
        }
    }
}

function Remove-WorkspaceTree([string]$Path, [string]$Root) {
    $full = Assert-ContainedPath $Path $Root
    if (Test-Path -LiteralPath $full) {
        Get-TreeFiles $full | Out-Null
        Remove-Item -LiteralPath $full -Recurse -Force
    }
}

function Read-Sha256([string]$Path) {
    $stream = [IO.File]::OpenRead($Path)
    $sha = [Security.Cryptography.SHA256]::Create()
    try { return [BitConverter]::ToString($sha.ComputeHash($stream)).Replace("-", "").ToLowerInvariant() }
    finally { $sha.Dispose(); $stream.Dispose() }
}

function Assert-PeX64([string]$Path) {
    $stream = [IO.File]::OpenRead($Path)
    $reader = [IO.BinaryReader]::new($stream)
    try {
        if ($stream.Length -lt 64 -or $reader.ReadUInt16() -ne 0x5a4d) { throw "Not a PE file: $Path" }
        $stream.Position = 0x3c
        $offset = $reader.ReadInt32()
        if ($offset -lt 64 -or [long]$offset + 26 -gt $stream.Length) { throw "Invalid PE header offset: $Path" }
        $stream.Position = $offset
        if ($reader.ReadUInt32() -ne 0x4550 -or $reader.ReadUInt16() -ne 0x8664) { throw "Not a Windows x64 PE file: $Path" }
        $stream.Position = $offset + 22
        $characteristics = $reader.ReadUInt16()
        $isDll = [IO.Path]::GetExtension($Path) -ieq ".dll"
        if (($characteristics -band 2) -eq 0 -or (($characteristics -band 0x2000) -ne 0) -ne $isDll) {
            throw "PE executable/DLL characteristics do not match the runtime file: $Path"
        }
        $stream.Position = $offset + 24
        if ($reader.ReadUInt16() -ne 0x20b) { throw "Not a PE32+ file: $Path" }
    } finally { $reader.Dispose(); $stream.Dispose() }
}

function Assert-Contract($Contract) {
    if (($Contract.schema -isnot [int] -and $Contract.schema -isnot [long]) -or $Contract.schema -ne 1 -or
        $Contract.repository -cne "https://github.com/GitHub-Zero123/MCDevTool" -or
        $Contract.branch -cne "feature/plugin-system" -or $Contract.revision -cne "da038f849a1c23211f5bb793a3450b579275308d" -or
        $Contract.plugins_enabled -isnot [bool] -or -not $Contract.plugins_enabled -or
        ($Contract.abi.major -isnot [int] -and $Contract.abi.major -isnot [long]) -or
        ($Contract.abi.minor -isnot [int] -and $Contract.abi.minor -isnot [long]) -or
        $Contract.abi.major -ne 1 -or $Contract.abi.minor -ne 0 -or
        @($Contract.capabilities).Count -ne 1 -or $Contract.capabilities[0] -cne "mcdk.game.process.create" -or
        @($Contract.game_versions).Count -ne 1 -or $Contract.game_versions[0] -cne "3.10.0.420447") {
        throw "MCDE requires the reviewed source lock, plugin ABI 1.0 and suspended-process capability; no automatic host/ABI upgrades."
    }
}

$hostRequired = @(
    "mcdk.exe", "mcdev-tracy-bridge.dll", "licenses/MCDevTool-LICENSE", "licenses/MCDevTool-plugin-SDK-LICENSE",
    "licenses/cpp-mcp-LICENSE", "licenses/NBT-LICENSE", "licenses/BinaryStream-LICENSE", "licenses/zlib-LICENSE",
    "licenses/CLI11-LICENSE", "licenses/cpp-httplib-LICENSE", "licenses/nlohmann-json-LICENSE",
    "licenses/MCDevLink-NOTICES.md", "licenses/asio-LICENSE", "licenses/native-profiler/LICENSE-Tracy.txt",
    "licenses/native-profiler/LICENSE-Capstone.txt"
)
$editorRequired = @(
    "plugin.json", "mcdeveditor.exe", "mcdeveditor-plugin.dll", "mcdeveditor-hook.dll",
    "Qt6Core.dll", "Qt6Gui.dll", "Qt6Widgets.dll", "Qt6Network.dll", "Qt6OpenGL.dll", "Qt6OpenGLWidgets.dll", "Qt6Svg.dll",
    "platforms/qwindows.dll", "iconengines/qsvgicon.dll", "imageformats/qsvg.dll",
    "assets/native_python.py", "assets/native_ui.py", "signatures/cpython-native.json",
    "licenses/MCDevTool-LICENSE", "licenses/Qt-LGPL-3.0-only.txt", "licenses/GLAD-LICENSE.txt",
    "licenses/MinHook-LICENSE.txt", "licenses/Lucide-LICENSE.txt", "licenses/PROVENANCE.txt"
)

function Assert-Editor([string]$Root, [switch]$AllowStaleLaunchSchema) {
    Get-TreeFiles $Root | Out-Null
    foreach ($relative in $editorRequired) {
        $path = Join-Path $Root $relative
        if (-not (Test-Path -LiteralPath $path -PathType Leaf) -or (Get-Item -LiteralPath $path).Length -le 0) {
            throw "Missing deployed MCDE runtime/notice: $relative. Build and deploy the Release editor first."
        }
    }
    $plugin = Get-Content -Encoding UTF8 -Raw -LiteralPath (Join-Path $Root "plugin.json") | ConvertFrom-Json
    if (($plugin.schema -isnot [int] -and $plugin.schema -isnot [long]) -or
        ($plugin.abi.major -isnot [int] -and $plugin.abi.major -isnot [long]) -or
        ($plugin.abi.minor -isnot [int] -and $plugin.abi.minor -isnot [long]) -or
        $plugin.version -isnot [string] -or [string]::IsNullOrWhiteSpace($plugin.version) -or
        $plugin.schema -ne 1 -or $plugin.id -cne "com.xiaobo.mcdeveditor" -or $plugin.abi.major -ne 1 -or
        $plugin.abi.minor -ne 0 -or $plugin.entry_symbol -cne "mcdk_plugin_entry" -or
        $plugin.libraries.'windows.x86_64' -cne "mcdeveditor-plugin.dll") { throw "MCDE plugin manifest is not the reviewed Windows x64 ABI 1.0 plugin." }
    if (-not $AllowStaleLaunchSchema -and ($plugin.config_schema.properties.initialDocument.type -cne "string" -or
        $plugin.config_schema.properties.stateDirectory.type -cne "string")) {
        throw "MCDE plugin metadata is missing the initialDocument/stateDirectory launch contract."
    }
    foreach ($file in Get-TreeFiles $Root) {
        if ($file.Extension -in ".exe", ".dll") { Assert-PeX64 $file.FullName }
    }
    if (@(Get-ChildItem -LiteralPath (Join-Path $Root "assets\icons") -File -Filter "*.svg").Count -eq 0) {
        throw "MCDE icon assets are missing."
    }
}

function Assert-Fingerprint([string]$Path, $Record) {
    if (($Record.size -isnot [int] -and $Record.size -isnot [long] -and $Record.size -isnot [uint64]) -or
        $Record.size -lt 0 -or $Record.sha256 -isnot [string] -or $Record.sha256 -cnotmatch '^[0-9a-f]{64}$') {
        throw "Invalid manifest size/SHA-256 for $Path"
    }
    if ((Get-Item -LiteralPath $Path).Length -ne $Record.size -or (Read-Sha256 $Path) -cne $Record.sha256) {
        throw "MCDE resource size/SHA-256 mismatch: $Path"
    }
}

function Get-Inventory([string]$Root) {
    $prefix = [IO.Path]::GetFullPath($Root).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    foreach ($file in Get-TreeFiles $Root | Sort-Object FullName) {
        $relative = $file.FullName.Substring($prefix.Length).Replace('\', '/')
        if ($relative -ceq "host/manifest.json") { continue }
        [ordered]@{ path = $relative; size = [long]$file.Length; sha256 = Read-Sha256 $file.FullName }
    }
}

function Assert-Resources([string]$Root) {
    if (-not (Test-Path -LiteralPath $Root -PathType Container)) { throw "MCDE resources are not prepared; run scripts/prepare-mcde.ps1 first." }
    Get-TreeFiles $Root | Out-Null
    $manifest = Get-Content -Encoding UTF8 -Raw -LiteralPath (Join-Path $Root "host\manifest.json") | ConvertFrom-Json
    Assert-Contract $manifest
    foreach ($relative in $hostRequired) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root ("host/" + $relative)) -PathType Leaf)) { throw "Missing host runtime/notice: $relative" }
    }
    Assert-Fingerprint (Join-Path $Root "host\mcdk.exe") $manifest.binary
    Assert-PeX64 (Join-Path $Root "host\mcdk.exe")
    Assert-PeX64 (Join-Path $Root "host\mcdev-tracy-bridge.dll")
    Assert-Editor (Join-Path $Root "editor")
    $expected = @{}
    foreach ($entry in $manifest.files) {
        if ($entry.path -isnot [string] -or $entry.path -cnotmatch '^(host|editor)/[^\\:]+$' -or
            @($entry.path.Split('/') | Where-Object { $_ -eq ".." -or $_ -eq "." -or $_ -eq "" }).Count -ne 0 -or
            $entry.path -ieq "host/manifest.json" -or $expected.ContainsKey($entry.path)) { throw "Unsafe or duplicate resource inventory path." }
        $expected[$entry.path] = $entry
    }
    $actual = @(Get-Inventory $Root)
    if ($actual.Count -ne $expected.Count) { throw "MCDE resource inventory changed (missing or extra files)." }
    foreach ($entry in $actual) {
        if (-not $expected.ContainsKey($entry.path)) { throw "Unlisted MCDE resource: $($entry.path)" }
        Assert-Fingerprint (Join-Path $Root $entry.path) $expected[$entry.path]
    }
    Write-Output "Verified MCDE resources: pinned plugin host, ABI 1.0, x64 PE files, $($actual.Count) runtime/notice files."
}

function Invoke-Git([string[]]$Arguments) {
    $result = & git -C $McdkSourceDirectory @Arguments
    if ($LASTEXITCODE -ne 0) { throw "MCDK source verification failed: git $($Arguments -join ' ')" }
    return $result
}

function Assert-Source {
    $top = [string](Invoke-Git @("rev-parse", "--show-toplevel"))
    if ([IO.Path]::GetFullPath($top) -ine $McdkSourceDirectory) { throw "-McdkSourceDirectory must be a repository root." }
    if ((Invoke-Git @("rev-parse", "HEAD")) -cne $sourceLock.revision) { throw "MCDK source must be checked out at $($sourceLock.revision), not the branch tip or a release." }
    $dirty = @(Invoke-Git @("status", "--porcelain", "--untracked-files=no", "--ignore-submodules=none"))
    if ($dirty.Count) { throw "MCDK source has tracked changes; refusing to certify a pinned host." }
    $remote = [string](Invoke-Git @("remote", "get-url", "origin"))
    if ($remote -cnotmatch '^(https://github\.com/GitHub-Zero123/MCDevTool(?:\.git)?|git@github\.com:GitHub-Zero123/MCDevTool\.git)$') {
        throw "MCDK origin does not match the source-lock repository."
    }
    $branchRef = "refs/remotes/origin/" + $sourceLock.branch
    Invoke-Git @("rev-parse", "--verify", $branchRef) | Out-Null
    Invoke-Git @("merge-base", "--is-ancestor", $sourceLock.revision, $branchRef) | Out-Null
    foreach ($line in Invoke-Git @("submodule", "status", "--recursive")) {
        if ($line -match '^[+\-U]') { throw "MCDK submodule missing or not at its pinned revision: $line" }
    }
}

function Invoke-LoggedCommand([string]$Executable, [string[]]$Arguments, [string]$Log) {
    $previousPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = "Continue"
        & $Executable @Arguments 2>&1 | Out-File -Encoding UTF8 -LiteralPath $Log
        $code = $LASTEXITCODE
    } finally { $ErrorActionPreference = $previousPreference }
    if ($code -ne 0) {
        Get-Content -Encoding UTF8 -LiteralPath $Log -Tail 30 | Write-Output
        throw "MCDE preparation command failed (exit $code). Full log: $Log"
    }
}

function Assert-HostCache([string]$Build) {
    $cache = @{}
    foreach ($line in Get-Content -Encoding UTF8 -LiteralPath (Join-Path $Build "CMakeCache.txt")) {
        if ($line -match '^([^/#][^:]*):[^=]+=(.*)$') { $cache[$matches[1]] = $matches[2] }
    }
    if ([IO.Path]::GetFullPath($cache.CMAKE_HOME_DIRECTORY) -ine $McdkSourceDirectory -or
        $cache.CMAKE_GENERATOR -cne "Visual Studio 17 2022" -or $cache.CMAKE_GENERATOR_PLATFORM -cne "x64" -or
        $cache.MCDK_ENABLE_PLUGINS -cne "ON" -or $cache.MC_DEV_TOOL_MCDK_ENABLE_CLI -cne "ON" -or
        $cache.MC_DEV_TOOL_BUILD_TEST -cne "OFF" -or $cache.MCDEV_TRACY_BUILD_TESTS -cne "OFF") {
        throw "Host CMake cache does not match the pinned plugin-enabled MSVC 2022 x64 build."
    }
}

function Copy-Notice([string]$Source, [string]$Target) {
    New-Item -ItemType Directory -Force -Path ([IO.Path]::GetDirectoryName($Target)) | Out-Null
    Copy-Item -LiteralPath $Source -Destination $Target
}

function Write-HeaderNotice([string]$Source, [string]$Target, [switch]$Mit) {
    $lines = [Collections.Generic.List[string]]::new()
    foreach ($line in Get-Content -Encoding UTF8 -LiteralPath $Source) {
        if (-not $line.StartsWith("//")) { break }
        $lines.Add($line.Substring(2).TrimStart())
    }
    if ($lines.Count -eq 0) { throw "Expected an upstream copyright/license header: $Source" }
    if ($Mit) {
        $lines.Add(@'

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
'@)
    }
    [IO.File]::WriteAllLines($Target, $lines, [Text.UTF8Encoding]::new($false))
}

Assert-Contract $sourceLock
Assert-ContainedPath $destination $repoRoot | Out-Null
if ($VerifyOnly) {
    Assert-Resources $destination
    return
}

if (-not $EditorRoot) { $EditorRoot = Join-Path ([IO.Path]::GetDirectoryName($repoRoot)) "MCDevEditor" }
$EditorRoot = (Resolve-Path -LiteralPath $EditorRoot).Path
if (-not $McdkSourceDirectory) { $McdkSourceDirectory = Join-Path $EditorRoot ".deps\mcdk-src" }
$McdkSourceDirectory = (Resolve-Path -LiteralPath $McdkSourceDirectory).Path
if (-not [IO.Path]::IsPathRooted($EditorBuildDirectory)) { $EditorBuildDirectory = Join-Path $EditorRoot $EditorBuildDirectory }
$editorInput = Join-Path ([IO.Path]::GetFullPath($EditorBuildDirectory)) "plugins\mcdeveditor\Release"
Assert-Editor $editorInput -AllowStaleLaunchSchema
$pluginSource = Join-Path $EditorRoot "plugins\workbench\plugin.json"
$sourcePlugin = Get-Content -Encoding UTF8 -Raw -LiteralPath $pluginSource | ConvertFrom-Json
$builtPlugin = Get-Content -Encoding UTF8 -Raw -LiteralPath (Join-Path $editorInput "plugin.json") | ConvertFrom-Json
foreach ($field in "schema", "id", "version", "entry_symbol", "abi", "libraries", "min_stage", "permissions") {
    if (($sourcePlugin.$field | ConvertTo-Json -Depth 8 -Compress) -cne ($builtPlugin.$field | ConvertTo-Json -Depth 8 -Compress)) {
        throw "MCDE source metadata changed binary binding '$field'; rebuild/deploy the editor before preparation."
    }
}
if ($sourcePlugin.config_schema.properties.initialDocument.type -cne "string" -or
    $sourcePlugin.config_schema.properties.stateDirectory.type -cne "string") {
    throw "MCDE source plugin.json must declare the initialDocument/stateDirectory launch contract."
}
Assert-Source
$vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
$vs = & $vswhere -latest -version '[17.0,18.0)' -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if ($LASTEXITCODE -ne 0 -or -not $vs) { throw "Visual Studio 2022 x64 C++ Build Tools are required." }
$cmake = Join-Path $vs "Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe"
if (-not (Test-Path -LiteralPath $cmake -PathType Leaf)) { throw "The Visual Studio 2022 CMake component is required." }
$python = (Get-Command python -ErrorAction Stop).Source
$build = Assert-ContainedPath (Join-Path $repoRoot "target\mcde-host") $repoRoot
New-Item -ItemType Directory -Force -Path $build, $resourceParent | Out-Null
$writerLock = [IO.File]::Open((Join-Path $build "prepare.lock"), [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
$stage = Assert-ContainedPath (Join-Path $resourceParent (".mcde-stage-" + [Guid]::NewGuid().ToString("N"))) $resourceParent
$backup = Assert-ContainedPath (Join-Path $resourceParent (".mcde-backup-" + [Guid]::NewGuid().ToString("N"))) $resourceParent
try {
    if (Test-Path -LiteralPath (Join-Path $build "CMakeCache.txt")) { Assert-HostCache $build }
    Write-Output "Building locked MCDE host from $($sourceLock.revision); logs: $build"
    $previousHashSeed = $env:PYTHONHASHSEED
    try {
        $env:PYTHONHASHSEED = "0"
        Invoke-LoggedCommand $python @("-X", "utf8", (Join-Path $McdkSourceDirectory "mods\generate.py")) (Join-Path $build "resources.log")
    } finally { $env:PYTHONHASHSEED = $previousHashSeed }
    Assert-Source
    Invoke-LoggedCommand $cmake @(
        "-S", $McdkSourceDirectory, "-B", $build, "-G", "Visual Studio 17 2022", "-A", "x64",
        "-DMCDK_ENABLE_PLUGINS=ON", "-DMC_DEV_TOOL_MCDK_ENABLE_CLI=ON", "-DMC_DEV_TOOL_BUILD_TEST=OFF",
        "-DMCDEV_TRACY_BUILD_TESTS=OFF", "-DBUILD_TESTING=OFF", "-DMCDK_PLUGIN_SDK_BUILD_CHECKS=OFF", "-DMCDK_PLUGIN_SDK_BUILD_EXAMPLES=OFF"
    ) (Join-Path $build "configure.log")
    Assert-HostCache $build
    Invoke-LoggedCommand $cmake @("--build", $build, "--config", "Release", "--target", "mcdk", "--parallel", "6") (Join-Path $build "build.log")
    Assert-Source
    Assert-HostCache $build
    $hostOutput = Join-Path $build "tools\mcdk\Release"
    $hostStage = Join-Path $stage "host"
    New-Item -ItemType Directory -Path $stage, $hostStage | Out-Null
    foreach ($name in "mcdk.exe", "mcdev-tracy-bridge.dll") { Copy-Item -LiteralPath (Join-Path $hostOutput $name) -Destination $hostStage }
    Get-TreeFiles (Join-Path $hostOutput "licenses") | Out-Null
    Copy-Item -LiteralPath (Join-Path $hostOutput "licenses") -Destination (Join-Path $hostStage "licenses") -Recurse
    $notices = @{
        "LICENSE" = "MCDevTool-LICENSE"; "libs/cpp-mcp/LICENSE" = "cpp-mcp-LICENSE"; "libs/nbt/LICENSE" = "NBT-LICENSE"
        "libs/nbt/libs/binarystream/LICENSE" = "BinaryStream-LICENSE"; "libs/nbt/libs/zlib-1.3.1/LICENSE" = "zlib-LICENSE"
        "components/MCDevLink/THIRD_PARTY_NOTICES.md" = "MCDevLink-NOTICES.md"
        "components/MCDevLink/third_party/asio/LICENSE_1_0.txt" = "asio-LICENSE"
    }
    foreach ($source in $notices.Keys) { Copy-Notice (Join-Path $McdkSourceDirectory $source) (Join-Path $hostStage ("licenses/" + $notices[$source])) }
    Copy-Notice (Join-Path $McdkSourceDirectory "LICENSE") (Join-Path $hostStage "licenses/MCDevTool-plugin-SDK-LICENSE")
    Write-HeaderNotice (Join-Path $McdkSourceDirectory "tools/mcdk/libs/CLI11.hpp") (Join-Path $hostStage "licenses/CLI11-LICENSE")
    Write-HeaderNotice (Join-Path $McdkSourceDirectory "libs/cpp-mcp/common/httplib.h") (Join-Path $hostStage "licenses/cpp-httplib-LICENSE") -Mit
    Write-HeaderNotice (Join-Path $McdkSourceDirectory "libs/nlohmann/nlohmann/json.hpp") (Join-Path $hostStage "licenses/nlohmann-json-LICENSE") -Mit
    Get-TreeFiles $editorInput | Out-Null
    Copy-Item -LiteralPath $editorInput -Destination (Join-Path $stage "editor") -Recurse
    Copy-Item -LiteralPath $pluginSource -Destination (Join-Path $stage "editor/plugin.json") -Force
    Copy-Notice (Join-Path $EditorRoot "README.md") (Join-Path $stage "editor/licenses/MCDevEditor-README.md")
    $binary = Join-Path $hostStage "mcdk.exe"
    $manifest = [ordered]@{}
    foreach ($name in "schema", "repository", "branch", "revision", "plugins_enabled", "abi", "capabilities", "game_versions") {
        $manifest[$name] = $sourceLock.$name
    }
    $manifest.binary = [ordered]@{ size = [long](Get-Item -LiteralPath $binary).Length; sha256 = Read-Sha256 $binary }
    $manifest.files = @(Get-Inventory $stage)
    [IO.File]::WriteAllText((Join-Path $hostStage "manifest.json"), ($manifest | ConvertTo-Json -Depth 8) + "`n", [Text.UTF8Encoding]::new($false))
    Assert-Resources $stage

    # Rename only a fully verified snapshot; retain the old resources until promotion succeeds.
    Assert-ContainedPath $destination $resourceParent | Out-Null
    if (Test-Path -LiteralPath $destination) {
        Get-TreeFiles $destination | Out-Null
        Move-Item -LiteralPath $destination -Destination $backup
    }
    $promoted = $false
    try {
        Assert-ContainedPath $stage $resourceParent | Out-Null
        Move-Item -LiteralPath $stage -Destination $destination
        $promoted = $true
        Assert-Resources $destination
    } catch {
        if ($promoted) { Remove-WorkspaceTree $destination $resourceParent }
        if (Test-Path -LiteralPath $backup) {
            Assert-ContainedPath $backup $resourceParent | Out-Null
            Assert-ContainedPath $destination $resourceParent | Out-Null
            Move-Item -LiteralPath $backup -Destination $destination
        }
        throw
    }
    if (Test-Path -LiteralPath $backup) {
        try { Remove-WorkspaceTree $backup $resourceParent }
        catch { Write-Warning "Prepared resources are valid, but the previous snapshot remains at $backup : $_" }
    }
    Write-Output "Prepared independent MCDE resources at $destination. No game or editor was launched."
} finally {
    try { Remove-WorkspaceTree $stage $resourceParent } finally { $writerLock.Dispose() }
}
