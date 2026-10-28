# Independent MCDE Resources

MCDE uses its own source-built, plugin-enabled MCDK host. It never replaces the
ordinary release MCDK in `src-tauri/release-resources/mcdk`, the MCDK update path,
an installed MCDH, or an existing host/game session. Preparation does not launch
the editor or a game, rewrite project configuration, or touch shared runtime packs.

## Source Lock

MCDH offers the native adapter as the optional bundled plugin `mcdh.mcde`.
Installation is explicit from the plugin page's built-in catalog. Startup only
refreshes already-installed registrations; uninstalling is persistent across
restarts and application updates. The plugin page owns its enable switch and
uninstall command and reuses the existing MCDE directory configuration. Both the component plugin command and the legacy quick
launch check the registration before creating a process. A live editor session
blocks disabling or uninstalling the adapter; normal MCDK launches are unaffected.
Without an enabled adapter, MCDH hides MCDE controls/settings and does not probe
its native resources. Uninstalling preserves native binaries, editor configuration
and project files, even when plugin management data is explicitly cleared. This does not
change the native MCDK plugin ID, ABI, packaging or host verification described
below. The adapter is desktop-only, cannot be granted MCP access, and updates with
MCDH rather than through GitHub/local package installation.

`assets/mcde/host-source.json` is the committed input lock:

- Repository: `https://github.com/GitHub-Zero123/MCDevTool`
- Branch lineage: `feature/plugin-system`
- Exact checkout: `da038f849a1c23211f5bb793a3450b579275308d`
- Plugin ABI: **1.0**, with `mcdk.game.process.create`
- Supported native game target: **3.10.0.420447**

The pin intentionally predates the branch tip. Do not switch to the newest branch
commit, a newer SDK/ABI, or an ordinary downloaded release to satisfy preparation.
The script verifies the checkout is a repository root, HEAD equals the pin,
tracked files/submodules are clean, `origin` matches the locked repository,
submodules are initialized at their recorded revisions, and the pin is an
ancestor of local `refs/remotes/origin/feature/plugin-system`. It does not fetch
or modify branch refs/checkouts. Refresh that remote-tracking ref during setup
if it is missing; detached HEAD at the pin is supported.

## Development Setup

Build prerequisites: Windows x64, PowerShell 5.1+, Git, Python 3 with `-X utf8`,
and Visual Studio **2022** C++ x64 Build Tools with its CMake component. The
existing MCDE build additionally needs its Qt **6.8.3 MSVC 2022 x64** kit; follow
MCDE's setup instructions. MCDE is a development input, not a runtime dependency
on a neighboring checkout. No absolute development drive path is embedded by
MCDH's preparation or runtime resource layout.

From the MCDH repository root, a neighboring `../MCDevEditor` is the default:

```powershell
$editor = Join-Path (Split-Path (Get-Location).Path -Parent) 'MCDevEditor'
powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $editor 'tools/Setup.ps1')
git -C (Join-Path $editor '.deps/mcdk-src') fetch origin 'feature/plugin-system:refs/remotes/origin/feature/plugin-system'
git -C (Join-Path $editor '.deps/mcdk-src') checkout --detach da038f849a1c23211f5bb793a3450b579275308d
git -C (Join-Path $editor '.deps/mcdk-src') submodule update --init --recursive
powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $editor 'tools/Build.ps1')
pnpm mcde:prepare
```

Do not discard tracked dependency changes to make this pass. Review them or
prepare a separate clean checkout. Setup/build tools belong to MCDE and are
not run by MCDH's release preparation.

Alternative development inputs are explicit:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/prepare-mcde.ps1 -EditorRoot ../OtherEditor -McdkSourceDirectory ../PinnedMCDevTool -EditorBuildDirectory build/completion
```

`-EditorBuildDirectory` defaults to `build/completion`, relative to `-EditorRoot`;
an absolute build directory is also accepted. Its deployed plugin input is
`plugins/mcdeveditor/Release`. The script requires the complete deployed Qt
directory, native hook, assets, icons, signatures, and MCDE license/provenance
files. It copies the entire directory, not just three binaries. Source
`plugins/workbench/plugin.json` refreshes the staged metadata, provided its
ID/version/ABI/entry/library/stage/permissions still match the deployed build.
`initialDocument` and `stateDirectory` must have string config schemas. Changes
to actual binary bindings require rebuilding MCDE, not editing only metadata.

The host is built directly with CMake in MCDH's ignored `target/mcde-host`:
`Visual Studio 17 2022`, `x64`, `Release`, target `mcdk`, plugins and CLI **ON**,
host/profiler tests and SDK examples/checks **OFF**. `mods/generate.py` always runs
first, with a fixed Python hash seed for stable generated resource identifiers.
Native-profiler dependencies may need network access on a cold CMake cache;
upstream uses pinned archives and archive hashes, not ordinary host downloads.
A cache bound to another source path/build contract is rejected. Use a fresh
working copy or deliberately clear only that build cache before changing its
source location. Full command logs are `resources.log`, `configure.log`, and
`build.log` in `target/mcde-host`; failures print at most 30 log lines.

## Generated Contract

Prepared resources are independent and relocatable:

```text
src-tauri/release-resources/mcde/
  host/mcdk.exe
  host/mcdev-tracy-bridge.dll
  host/manifest.json
  host/licenses/...
  editor/plugin.json
  editor/mcdeveditor.exe
  editor/mcdeveditor-plugin.dll
  editor/mcdeveditor-hook.dll
  editor/Qt6*.dll
  editor/platforms/, iconengines/, imageformats/, ...
  editor/assets/, signatures/, licenses/
```

`host/manifest.json` is UTF-8 JSON. It contains exactly the source contract keys
`schema: 1`, `repository`, `branch`, `revision`, `plugins_enabled: true`,
`abi: {major: 1, minor: 0}`, `capabilities: ["mcdk.game.process.create"]`,
`game_versions: ["3.10.0.420447"]`, plus `binary: {size, sha256}` and `files`.
`binary.size` is an unsigned JSON integer; `binary.sha256` is 64 lowercase
hexadecimal characters computed from the built `mcdk.exe`.

The additional `files` array contains `{path, size, sha256}` for every staged
file except the manifest itself. Paths use `/` and are relative to the **mcde
resource root**, not `host`; for example, `host/licenses/MCDevTool-LICENSE` and
`editor/plugin.json`. Verification rejects traversal, duplicate entries,
missing/extra files, altered runtime files/notices, incompatible plugin
metadata, and non-x64 PE images. The core launch contract can independently
check the host manifest and host byte hash/PE before launch.

Host notices are obtained from the pinned source, embedded header notices, and
CMake's native-profiler license output. MCDevTool's root BSD license also covers
its SDK and is staged under both host and SDK notice names. MCDE's existing
licenses/provenance are retained, with its README included as an additional
notice. This is an integrity/notice check, not publishing-compliance certification.

The script builds and verifies a unique sibling temporary stage before renaming
it into place, retains old resources until promotion verifies, and restores
them on promotion failure. Cleanup/moves validate final workspace paths and
reject reparse points. A preparation lock excludes concurrent writers. A killed
process may leave a `.mcde-stage-*` or `.mcde-backup-*` directory; inspect it and
any backup before removing it. Never commit executables, the generated manifest
with machine-specific hashes, build caches, or staged notices. Only the source
lock and preparation/documentation changes belong in source control.

## Verify And Package

```powershell
pnpm mcde:prepare
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/prepare-mcde.ps1 -VerifyOnly
pnpm release:prepare
pnpm tauri build
pnpm release:package
```

Run `pnpm mcde:prepare` explicitly before both MCDE development validation and
an MCDE-enabled release. `-VerifyOnly` neither builds nor needs the neighboring
editor checkout, Git, Python, or Visual Studio. It checks staged bytes and
notices. `-ResourceDirectory <path>` is allowed only with `-VerifyOnly`, for a
copied resource tree inside this MCDH workspace, including portable staging.

Build wiring is integrated: `mcde:prepare` invokes
this script; Tauri includes `release-resources/mcde/`; ordinary release prepare
creates an empty generated directory on clean clones, and verifies MCDE only
when `host/manifest.json` exists. An empty placeholder means MCDE is not ready;
it is not a substitute host and does not make the optional feature available.
No ordinary-release download or automatic ABI upgrade is permitted.

`package-portable.ps1` includes and verifies MCDE when a prepared host manifest
exists; it skips an empty optional placeholder. `-IncludeMcde` can explicitly
require preparation and fail if it is missing. Both the source resource tree
and the copied portable tree are verified before archive generation. Installer
and portable runtime resource layouts are `mcde/host` and `mcde/editor`; do not
ship only `mcdk.exe` or copy the SDK/development checkout into an installation.

## Runtime Limits

A project must already have a valid UTF-8 `.mcdev.json` with a configured
`game_executable_path`, real resource/behavior pack directories, and appropriate
world settings. First configure an ordinary MCDK project rather than guessing
an executable path. Native MCDE launch requires the installed game
`Minecraft.Windows.exe` under version directory `3.10.0.420447`; **3.9 is not
accepted** by this locked integration. Use `reset_world: false`. MCDH creates
an owned launch profile with the MCDE plugin path/config; it does not overwrite
the project's original config. Direct standalone host launches additionally
need an explicit enabled `com.xiaobo.mcdeveditor` declaration pointing to the
deployed editor directory. Avoid duplicate declarations/other enabled plugins
in MCDH's initial MCDE integration.

Runtime requires Windows x64, a working graphics driver and the installed
Visual C++ 2015-2022 x64 runtime for dynamically linked Qt/editor libraries.
The development snapshot does not bundle that redistributable installer.
Qt DLL/platform/image/icon plugins are deployed beside the editor executable.
An offline editor has no native game preview. Packaging, manifest/hash checks,
and synthetic PE tests do not certify shader/native injection, real-game UI,
IME, visual rendering, or lifecycle cleanup; those require separate owned
project acceptance tests. Never replace DLLs in an active editor session or
interrupt an unrelated installed MCDH/host/game to test preparation.

## Native Test Setup

The explicit test-only example copies the owned demo packs and creates a new
index under the ignored `target/native-mcde-qa/<uuid>` directory. It does not
start a game, scan production MCS projects, or rewrite their configuration:

```powershell
cargo run -p mcdh-core --example mcde_smoke_setup -- ../MCDevEditor <installed-game-exe>
```

The returned JSON provides the fixture's data/project/appdata paths and port
19147. Check the port is free before launch. Start the built MCDH with only that
process's `MCDH_DATA_DIR` set to the returned data directory and
`MCDH_DISABLE_MCS_SCAN=1`. For isolated real-game testing also set `APPDATA`,
`LOCALAPPDATA`, `MCDE_QA_WORKSPACE_ROOT`, `MCDE_QA_ROAMING_APPDATA`, and
`MCDE_QA_LOCAL_APPDATA` to the reviewed fixture paths, and use an independent
`WEBVIEW2_USER_DATA_FOLDER`. Restore the launching process's environment after
creating the child. These QA markers are explicit, not normal-launch defaults.

The maintained launcher handles those environment variables and restores them:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-mcde-native-smoke.ps1 -SetupPath <generated-setup.json>
```

`-BackendTest` invokes the opt-in `mcdk_test_mcde_native_backend` using the same
production `start_session` function, without a Tauri button click. Its report
labels that distinction, checks live/increasing shared frames and persistent
session identity, and retains the owned editor/game for normal close. Never run
it while that fixture already has a live session. It does not manually import
or register the fixture's mod entry, certify Save and Apply, or force cleanup.

Click the fixture's MCDE icon in the actual desktop application. Assert the
persisted session mode, original component path, exact host binary path/hash,
owned game/editor process ancestry, original config hash, and increasing shared
frames through MCDE's structured controller. A successful host process alone
is not a game-ready assertion. Close/reopen MCDH while the owned game runs to
check session recovery and disabled launches, then close MCDE normally and
verify only the owned host/game ended. Keep the user's other sessions untouched.
