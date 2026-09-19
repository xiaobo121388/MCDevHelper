$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
$utf8 = [Text.UTF8Encoding]::new($false)
$work = [IO.Path]::GetFullPath($PSScriptRoot)
$job = Get-Content -Encoding UTF8 -Raw -LiteralPath (Join-Path $work "job.json") | ConvertFrom-Json
$exe = [IO.Path]::GetFullPath([string]$job.executable)
$target = [IO.Path]::GetDirectoryName($exe)
$backup = Join-Path $work "backup"
$staged = Join-Path $work "staged"
$changed = [Collections.Generic.List[string]]::new()
$saved = [Collections.Generic.List[string]]::new()
$parentExited = $false
$installStarted = $false
$success = $false
$mutex = $null
$ownsMutex = $false

function Assert-Contained([string]$path, [string]$root) {
    $full = [IO.Path]::GetFullPath($path)
    if (-not $full.StartsWith($root.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) {
        throw "Update path escaped its root: $full"
    }
    $cursor = $full
    while ($cursor) {
        if (Test-Path -LiteralPath $cursor) {
            $item = Get-Item -Force -LiteralPath $cursor
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Reparse point in update path: $cursor" }
        }
        $cursor = [IO.Path]::GetDirectoryName($cursor)
    }
}

function Save-Original([string]$relative) {
    $source = Join-Path $target $relative
    $destination = Join-Path $backup $relative
    Assert-Contained $source $target
    Assert-Contained $destination $work
    if (Test-Path -LiteralPath $source -PathType Leaf) {
        # Fail on locked application/MCP files before changing any installed file.
        $probe = [IO.File]::Open($source, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
        $probe.Dispose()
        [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($destination)) | Out-Null
        Copy-Item -LiteralPath $source -Destination $destination
        $saved.Add($relative)
    } elseif (Test-Path -LiteralPath $source) {
        throw "A directory occupies an application file path: $source"
    }
}

function Restore-Originals {
    $failures = [Collections.Generic.List[string]]::new()
    foreach ($relative in $changed) {
        $destination = Join-Path $target $relative
        try {
            Assert-Contained $destination $target
            if ($saved.Contains($relative)) {
                Copy-Item -LiteralPath (Join-Path $backup $relative) -Destination $destination -Force
            } elseif (Test-Path -LiteralPath $destination -PathType Leaf) {
                Remove-Item -LiteralPath $destination -Force
            }
        } catch { $failures.Add($_.Exception.Message) }
    }
    if ($failures.Count) { throw ($failures -join '; ') }
}

function Start-Application {
    # ProcessStartInfo treats brackets and apostrophes literally; PowerShell's
    # Start-Process -WorkingDirectory resolves provider wildcard paths.
    $start = [Diagnostics.ProcessStartInfo]::new($exe)
    $start.WorkingDirectory = $target
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $process = [Diagnostics.Process]::Start($start)
    $process.Dispose()
}

try {
    Assert-Contained $work $target
    if ([IO.Path]::GetDirectoryName($work) -ne $target -or [IO.Path]::GetFileName($work) -notlike '.mcdh-update-*') {
        throw "Invalid update working directory"
    }
    if ($job.kind -notin @('installed', 'portable')) { throw "Invalid installation kind" }
    $expectedName = if ($job.kind -eq 'portable') { 'MCDH.exe' } else { 'mcdh-desktop.exe' }
    if ([IO.Path]::GetFileName($exe) -ne $expectedName) { throw "Invalid application executable" }
    $hash = [Security.Cryptography.SHA256]::Create()
    try { $mutexName = [BitConverter]::ToString($hash.ComputeHash($utf8.GetBytes($target.ToLowerInvariant()))).Replace('-', '') }
    finally { $hash.Dispose() }
    $mutex = [Threading.Mutex]::new($false, "Local\MCDH-Update-$mutexName")
    try { $ownsMutex = $mutex.WaitOne(0) } catch [Threading.AbandonedMutexException] { $ownsMutex = $true }
    if (-not $ownsMutex) { throw "Another MCDH update is already running" }

    $packageName = if ($job.kind -eq 'installed') { 'setup.exe' } else { 'portable.zip' }
    $package = Join-Path $work $packageName
    if ((Get-FileHash -Algorithm SHA256 -LiteralPath $package).Hash -ne $job.package_sha256) {
        throw "Update package SHA-256 changed after download"
    }
    # Hold the original process handle before acknowledging, avoiding PID reuse races.
    $parent = [Diagnostics.Process]::GetProcessById([int]$job.parent_pid)
    $null = $parent.Handle
    [IO.File]::WriteAllText((Join-Path $work 'ready'), 'ready', $utf8)
    try {
        if (-not $parent.WaitForExit(60000)) { throw "MCDH did not exit within 60 seconds; update cancelled" }
        $parentExited = $true
    } finally { $parent.Dispose() }

    if ($job.kind -eq 'portable') {
        $files = @(Get-ChildItem -LiteralPath $staged -File -Recurse)
        foreach ($required in @('MCDH.exe', 'mcdh-mcp.exe', 'mcdk\mcdk.exe', 'mcdk\bundled.json')) {
            if (-not (Test-Path -LiteralPath (Join-Path $staged $required) -PathType Leaf)) { throw "Missing release file: $required" }
        }
        foreach ($file in $files) {
            Assert-Contained $file.FullName $staged
            $relative = $file.FullName.Substring($staged.Length + 1)
            if ($relative -notin @('MCDH.exe', 'mcdh-mcp.exe', 'LICENSE', 'README.md', 'THIRD_PARTY_LICENSES.md') -and -not $relative.StartsWith('mcdk\')) {
                throw "Unexpected release file: $relative"
            }
            Save-Original $relative
        }
        $installStarted = $true
        foreach ($file in $files) {
            $relative = $file.FullName.Substring($staged.Length + 1)
            $destination = Join-Path $target $relative
            Assert-Contained $destination $target
            [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($destination)) | Out-Null
            $changed.Add($relative)
            Move-Item -LiteralPath $file.FullName -Destination $destination -Force
        }
    } else {
        $managed = @('mcdh-desktop.exe', 'mcdh-mcp.exe', 'uninstall.exe')
        $resources = Join-Path $target 'release-resources'
        if (Test-Path -LiteralPath $resources) {
            Assert-Contained $resources $target
            $managed += @(Get-ChildItem -LiteralPath $resources -File -Recurse | ForEach-Object { $_.FullName.Substring($target.Length + 1) })
        }
        foreach ($relative in $managed) { Save-Original $relative }
        foreach ($relative in $saved) { $changed.Add($relative) }
        $installStarted = $true
        # NSIS /D must be last and unquoted, including paths containing spaces.
        # The helper restarts only after checking the installer's exit status.
        $start = [Diagnostics.ProcessStartInfo]::new($package, "/S /UPDATE /D=$target")
        $start.UseShellExecute = $false
        $start.CreateNoWindow = $true
        $start.WindowStyle = [Diagnostics.ProcessWindowStyle]::Hidden
        $installer = [Diagnostics.Process]::Start($start)
        $installer.WaitForExit()
        if ($installer.ExitCode -ne 0) { throw "Silent installer failed (exit $($installer.ExitCode))" }
        $installer.Dispose()
    }
    Start-Application
    $success = $true
} catch {
    $message = "MCDH update failed: $($_.Exception.Message)"
    if ($installStarted) {
        try { Restore-Originals; $message += '. Original application files restored.' }
        catch { $message += ". Recovery failed: $($_.Exception.Message). Backup: $backup" }
    }
    $message += " Update files: $work"
    [IO.File]::WriteAllText([string]$job.result_path, $message, $utf8)
    if ($parentExited) {
        try { Start-Application } catch {
            [IO.File]::AppendAllText([string]$job.result_path, ". Restart failed: $($_.Exception.Message)", $utf8)
        }
    }
} finally {
    if ($ownsMutex) { $mutex.ReleaseMutex() }
    if ($null -ne $mutex) { $mutex.Dispose() }
    if ($success) {
        # Delete only this validated temporary directory, never the application directory.
        Assert-Contained $work $target
        Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
    }
}
