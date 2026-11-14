# CONTROL — Chat 7 G8 clean Windows evidence toolkit acceptance

**Reviewed:** 2026-11-14
**PR:** [#21](https://github.com/1doiliemkhoet/tlauchcraft/pull/21)
**Feature HEAD:** `1237b145050ada51344dec50b91c13de5556f37a`
**Merged:** [`719494e6`](https://github.com/1doiliemkhoet/tlauchcraft/commit/719494e695f3c6c3aab43465240a25c55f6fd549)
**Windows CI:** [43596984154](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43596984154)

## Accepted source/test scope

- A documented operator-led clean-Windows 10/11 installer/WebView2/profile/SQLite/native-file-picker evidence checklist at `docs/control/G8_INTERACTIVE_TEST_PLAN.md`.
- Read-only capture helper `docs/control/scripts/g8-interactive-capture.ps1`: timestamps, operating system details, optional WebView2 registry data, screenshots of the current real screen, application process/window metadata, read-only SQLite profile ID query (when CLI installed), archive SHA-256, filtered Windows error events and log-file discovery.
- Static Windows PowerShell syntax/contract regression checks integrated into desktop Windows workflow with path filters; [CI 43596984154](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43596984154) PASS.
- `npm ci`, Vitest, Vite and Tauri cargo checks were **SKIPPED** for this evidence-only change; their absence is expected and **not** a new GUI build E2E proof.
- Updated instructions to reference Control-accepted real NSIS artifact from [run 43583383473](https://github.com/1doiliemkhoet/tlauchcraft/actions/runs/43583383473) and SHA256 `E030FA105ED8C23CB6BE10564C21AFE53BFEDE9C03C17F778A20A76FB55E6E5F`, rather than an older failed build.

## G8 remains PARTIAL — not accepted as interactive E2E

- No real clean Windows 10/11 user session evidence for wizard, WebView2 **rendered GUI**, navigation or clicks.
- No confirmed on-screen profile creation/reopen and read-only SQLite verification that its UUID stays identical.
- No actual native Windows ZIP/MRPACK picker + inspection metadata screenshot.
- The script **does not click the GUI or validate screenshot contents**, and always leaves operator review as NOT_REVIEWED. Static CI PASS is not G8.
- Collect evidence privately first, redact unrelated desktop contents, profile paths, usernames, IDs and secrets before uploading.

## Next acceptance

Operator should use the steps in `docs/control/G8_INTERACTIVE_TEST_PLAN.md` on a disposable clean Windows VM, attach screenshots, OS build, real installer artifact hash, WebView2 version, SQLite SELECT corroboration, native picker screenshots and inspection results to new follow-up issue/PR for independent Control review. G8 remains PARTIAL until all mandatory proof is reviewed.
