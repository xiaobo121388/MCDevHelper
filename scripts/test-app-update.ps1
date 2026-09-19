$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$testBase = Join-Path $repo 'target\update-worker-tests'
[IO.Directory]::CreateDirectory($testBase) | Out-Null
$root = Join-Path $testBase ([Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($root) | Out-Null
$utf8 = [Text.UTF8Encoding]::new($false)
$fixture = Join-Path $root 'fixture.exe'
$source = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $PSScriptRoot 'tests\update-fixture.cs')
Add-Type -TypeDefinition $source -OutputAssembly $fixture -OutputType ConsoleApplication
$shell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'

function Assert([bool]$condition, [string]$message) {
    if (-not $condition) { throw $message }
}
function Wait-File([string]$path) {
    $timer = [Diagnostics.Stopwatch]::StartNew()
    while (-not (Test-Path -LiteralPath $path)) {
        if ($timer.Elapsed.TotalSeconds -gt 20) { throw "Timed out: $path" }
        Start-Sleep -Milliseconds 50
    }
}
function Write-Text([string]$path, [string]$text) {
    [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($path)) | Out-Null
    [IO.File]::WriteAllText($path, $text, $utf8)
}

$passed = 0
foreach ($scenario in @('portable-success', 'installed-success', 'installed-failure', 'portable-locked', 'portable-restart-failure', 'corrupted-package')) {
    # Exercise non-ASCII, spaces, brackets and apostrophes without shell interpolation.
    $target = Join-Path $root ($scenario + " [app]' " + [char]0x4E2D + [char]0x6587)
    $work = Join-Path $target '.mcdh-update-test'
    [IO.Directory]::CreateDirectory($work) | Out-Null
    $kind = if ($scenario.StartsWith('installed')) { 'installed' } else { 'portable' }
    $name = if ($kind -eq 'installed') { 'mcdh-desktop.exe' } else { 'MCDH.exe' }
    $exe = Join-Path $target $name
    Copy-Item -LiteralPath $fixture -Destination $exe
    Write-Text (Join-Path $target 'mcdh-mcp.exe') 'old-mcp'
    Write-Text (Join-Path $target 'user-settings.json') 'keep-user-settings'
    Write-Text (Join-Path $target 'mcdk\user-file.txt') 'keep-user-file'
    $result = Join-Path $target 'last-update-error.txt'
    if ($kind -eq 'installed') {
        Write-Text (Join-Path $target 'uninstall.exe') 'old-uninstaller'
        Write-Text (Join-Path $target 'release-resources\README.md') 'old-readme'
        Copy-Item -LiteralPath $fixture -Destination (Join-Path $work 'setup.exe')
        Copy-Item -LiteralPath $fixture -Destination (Join-Path $work 'new-app.exe')
        if ($scenario -eq 'installed-failure') { Write-Text (Join-Path $work 'fail-installer') 'fail' }
        $package = Join-Path $work 'setup.exe'
    } else {
        $staged = Join-Path $work 'staged'
        [IO.Directory]::CreateDirectory($staged) | Out-Null
        Copy-Item -LiteralPath $fixture -Destination (Join-Path $staged 'MCDH.exe')
        Write-Text (Join-Path $staged 'mcdh-mcp.exe') 'new-mcp'
        Write-Text (Join-Path $staged 'mcdk\mcdk.exe') 'new-mcdk'
        Write-Text (Join-Path $staged 'mcdk\bundled.json') '{}'
        if ($scenario -eq 'portable-restart-failure') { Write-Text (Join-Path $staged 'MCDH.exe') 'invalid executable' }
        $package = Join-Path $work 'portable.zip'
        Write-Text $package 'verified archive fixture'
    }
    Copy-Item -LiteralPath (Join-Path $repo 'src-tauri\src\update-worker.ps1') -Destination (Join-Path $work 'worker.ps1')
    $parent = $null
    $worker = $null
    $lock = $null
    try {
        $parent = Start-Process -FilePath $exe -ArgumentList '--wait' -PassThru -WindowStyle Hidden
        Wait-File (Join-Path $target 'parent-started')
        $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $package).Hash
        if ($scenario -eq 'corrupted-package') { $hash = '0' * 64 }
        $job = @{ parent_pid = $parent.Id; executable = $exe; kind = $kind; result_path = $result; package_sha256 = $hash }
        Write-Text (Join-Path $work 'job.json') ($job | ConvertTo-Json)
        if ($scenario -eq 'portable-locked') {
            $lock = [IO.File]::Open((Join-Path $target 'mcdh-mcp.exe'), [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
        }
        $arguments = '-NoProfile -NonInteractive -ExecutionPolicy Bypass -File "' + (Join-Path $work 'worker.ps1') + '"'
        $worker = Start-Process -FilePath $shell -ArgumentList $arguments -PassThru -WindowStyle Hidden
        if ($scenario -ne 'corrupted-package') {
            Wait-File (Join-Path $work 'ready')
            Assert (-not (Test-Path -LiteralPath (Join-Path $target 'restarted.txt'))) 'Worker restarted before parent exited'
            Assert ((Get-Content -Encoding UTF8 -Raw -LiteralPath (Join-Path $target 'mcdh-mcp.exe')) -eq 'old-mcp') 'Worker changed files before parent exited'
        }
        Write-Text (Join-Path $target 'exit-parent') 'exit'
        Assert ($parent.WaitForExit(10000)) 'Parent did not exit'
        Assert ($worker.WaitForExit(30000)) 'Worker did not exit'
        if ($null -ne $lock) { $lock.Dispose(); $lock = $null }
        $succeeded = $scenario.EndsWith('-success')
        if ($succeeded) {
            Assert (-not (Test-Path -LiteralPath $result)) "Unexpected update error: $result"
            Assert ((Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $target 'mcdh-mcp.exe')) -eq 'new-mcp') 'New version not installed'
            Assert (-not (Test-Path -LiteralPath $work)) 'Successful update left its temporary directory'
        } else {
            Assert (Test-Path -LiteralPath $result) 'Failure was not reported'
            Assert ((Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $target 'mcdh-mcp.exe')) -eq 'old-mcp') 'Original MCP was not preserved/restored'
            Assert ((Get-FileHash -LiteralPath $exe).Hash -eq (Get-FileHash -LiteralPath $fixture).Hash) 'Original executable was not restored'
            if ($scenario -eq 'installed-failure') {
                $argsText = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $work 'installer-args.txt')
                Assert ($argsText.Contains('/S /UPDATE /D=' + $target)) 'Installer not called silently at the original location'
            }
        }
        if ($scenario -ne 'corrupted-package') { Wait-File (Join-Path $target 'restarted.txt') }
        Assert ((Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $target 'user-settings.json')) -eq 'keep-user-settings') 'User settings changed'
        Assert ((Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $target 'mcdk\user-file.txt')) -eq 'keep-user-file') 'Unmanaged file changed'
        Write-Output "PASS $scenario"
        $passed++
    } finally {
        if ($null -ne $lock) { $lock.Dispose() }
        foreach ($process in @($parent, $worker)) {
            if ($null -ne $process) {
                if (-not $process.HasExited) { $process.Kill(); $process.WaitForExit() }
                $process.Dispose()
            }
        }
    }
}
Write-Output "$passed update worker scenarios passed. Fixtures: $root"
