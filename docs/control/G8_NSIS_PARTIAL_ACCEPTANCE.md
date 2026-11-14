# CONTROL — Windows NSIS build/install smoke acceptance (G8_PARTIAL)

**Reviewed:** 2026-11-14  
**PR:** [#20](https://github.com/1doiliemkhoet/tlauchcraft/pull/20)  
**Accepted head:** `417ba007975095c708bd1e66306b679615428a77`  
**Merged:** [`911597b5`](https://github.com/1doiliemkhoet/tlauchcraft/commit/911597b585319853f0aa4e8461d9e3123e6c6947)  
**Windows Desktop CI:** [43583383418](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43583383418)  
**Windows installer build/smoke:** [43583383473](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43583383473)

## Verified evidence
- Built actual Windows NSIS installer `tlauchcraft_0.1.0_x64-setup.exe` from Rust/Tauri Windows release build, **2,811,597 bytes**; SHA-256 `E030FA105ED8C23CB6BE10564C21AFE53BFEDE9C03C17F778A20A76FB55E6E5F`.
- Windows CI generated installer artifact and structured diagnostic evidence.
- NSIS /S smoke executed on a fresh GitHub-hosted Windows runner, installer exited **0** and installed binary was found.
- Desktop tests (9 Vitest), TypeScript/Vite build and locked Tauri cargo check pass on current PR head.
- Redundant artifact replay workflow referencing an expiring older run was removed.

## NOT_VERIFIED (mandatory for full G8 PASS)
- Actual GUI-based installation on a clean Windows 10/11 VM, WebView2 **rendered** UI, wizard and Start Menu entry.
- Creating an Offline Profile via installed app UI, closing/reopening, and verifying **same UUID** persisted in SQLite.
- Selecting a harmless ZIP/MRPACK using native file picker and verifying metadata is shown in the UI.
- Process spawn or silent installer exit 0 does **NOT** establish app window visibility, responsiveness, or feature correctness.
- The smoke script does not make a fast-exiting app process fail G8 CI. Keep G8_PARTIAL; no claim of full clean-machine installer acceptance.

## Next
- Chat 7 starts a **new feature branch/PR** from current `develop` to collect and review independent Windows interactive UI evidence. Do not reuse merged PR #20.
- CI maintenance: include `apps/desktop/scripts/g8-installer-smoke.ps1` in the NSIS workflow path trigger or explicitly dispatch the workflow when that script changes; do not silently leave script edits untested.
- Limit expensive installer CI to meaningful code/release changes.
