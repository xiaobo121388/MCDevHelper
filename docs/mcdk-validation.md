# MCDK verification

## Automated checks

Run from the repository root on Windows x64 with pnpm and the MSVC Rust toolchain:

```powershell
pnpm install --frozen-lockfile
pnpm mcdk:prepare
pnpm test
pnpm build
cargo test --workspace
cargo test -p mcdh-desktop bundled_binary -- --ignored
cargo test -p mcdh-desktop interactive_console -- --ignored
pnpm release:windows
```

The ordinary test suite uses local fixtures and no live GitHub responses. The bundled-binary test requires the previously prepared release resource but performs installation and fallback offline. The interactive-console test compiles a small local probe, launches it in a separate console with a Chinese/space/shell-metacharacter working directory, checks console stdin/stdout, verifies survival after releasing the launch handle, and checks exit code 7. It does not launch Minecraft or modify a game world.

Core tests cover manifest validation, hashing, PE architecture, staging without activation, no downgrade, previous-version fallback, cross-process locks, cancellation generations, PID reuse, process identity recovery, and component mutation guards. Desktop tests cover release filtering, HTTP failures, bounded response sizes, stale update status, and automatic-versus-manual cancellation. Frontend tests cover card placement, duplicate clicks, persistent update settings, manual installation, errors, session recovery, protected actions, and event subscription cleanup.

## Native application smoke test

Use a temporary `MCDH_DATA_DIR` and set `MCDH_DISABLE_MCS_SCAN=1`. Do not reuse production project directories for destructive or world-reset tests.

1. Start the portable executable, open Settings > Development tools, and verify the bundled MCDK version appears even without a network connection.
2. Disable automatic updates, close and restart the application, and verify the setting persists independently of the main Save settings button.
3. Perform a manual check. With automatic updates disabled, a newer candidate must remain pending until Install now is clicked. With automatic updates enabled, a candidate must download and activate automatically.
4. With a slow test download, disable automatic updates and verify the active version is unchanged. A failed hash, truncated body, HTTP error or interrupted application must not replace the previous executable.
5. Add disposable addon, material, and map projects. Verify Play opens a separate MCDK console using the selected component directory, missing configuration is handled by MCDK, and existing configuration is not rewritten by MCDH.
6. Select an installed NetEase Minecraft executable in the MCDK console. Confirm actual component/world loading. Close MCDH while the game is running, restart it, and verify the same session is recovered and another launch is blocked.
7. During a session, verify move/delete/UUID/version actions are blocked. Close the game and confirm launch and protected actions become available again.
8. Install an update during a game session. The session must keep its existing executable; the next session must use the new version.

The base MCDK launch can clean shared runtime packs and deploy worlds according to project configuration. Do not run these game tests alongside unrelated MCDK sessions started outside MCDH. This integration only coordinates sessions it manages under the same data directory.

## Packaging and visual checks

- Installer resources must contain `release-resources/mcdk/mcdk.exe`, the pinned manifest and license notices. Portable archives must contain `mcdk/mcdk.exe` and the same notices next to `MCDH.exe`.
- Run `powershell -NoProfile -File scripts/prepare-mcdk.ps1 -VerifyOnly` to reject missing/corrupt build resources. Package checks must not substitute a successful build for an actual game-loading check.
- Verify light/dark themes at 1180x760 and the 900x620 minimum window. The Play icon precedes Open directory; footer controls do not shrink or overflow; settings and error states remain readable.
- Network downloads and generated executables, archives, databases, screenshots and test worlds must not be committed.

## Verification record: 2026-09-19

The implementation was checked with 54 Rust workspace tests, 21 frontend tests, the frontend build, strict Clippy, the isolated native console probe, and the bundled-binary offline recovery test. Windows installer/portable builds succeeded; the portable ZIP's MCDK digest and license entries were checked, as were the NSIS resource mappings. Browser-based visual checks use a mocked Tauri transport and therefore verify layout, not native command execution.

Launching the portable application for the final native smoke test was blocked by the execution environment's policy. The isolated fixture data remains under the ignored `target/native-smoke` directory. Actual application-to-game startup, Minecraft world loading, closing/reopening MCDH during a real game session, and updating during that session still require the native checks above. Neither a successful package build nor the automated console probe certifies those game behaviors.
