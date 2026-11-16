# Chat 4 — Rustic Waters II transactional staging contract

**Status:** code foundation; NOT a playable modpack. Minecraft 1.16.5 / exact Forge build from the CurseForge manifest. G5 NOT_VERIFIED. No game binary, API key, world or profile committed.

## Typed Rust entrypoints

```rust
use tl_pack::{
    inspect_archive,
    curseforge::{resolve_curseforge_files, configured_api_key, OfficialApiClient},
    transaction::{
        stage_transaction, recover_transaction, rollback_transaction,
        TransactionState, TransactionJournal, TransactionResult,
    },
    validation::{audit_dependencies, validate_staged_instance, ForgeRuntimeVerification},
};
```

- **Resolve:** `resolve_curseforge_files` validates the approved projectID/fileID, gameVersion and required/optional graph. Every missing authorized file or trusted SHA-1 stays explicitly blocked with parent IDs.
- **Download and verify:** `stage_transaction` creates a private `<operation-id>.journal.json` with `INSTALLING` **before** calling `stage_curseforge_downloads`. Only approved anonymous HTTPS CDN transfers; no redirects; upstream SHA-1/size enforced; computed SHA-256 is evidence. No secret is written to journal.
- **Stage:** Archive SHA-256 rechecked on extraction. Staging path `<operation-id>.staging` must not already exist. The journal and output directories are separate from user profiles/worlds.
- **Recovery:** `recover_transaction` converts an interrupted `INSTALLING` journal to `INTERRUPTED`; never treats residual files as complete or auto-replays unknown downloads. `rollback_transaction` is idempotent and marks the transaction `ROLLED_BACK`; it deliberately *retains* private staging bytes as diagnostic evidence instead of deleting any directory or user save. Explicit cleanup/retention policy remains a Control task.
- **Forge handoff:** Chat 3's unmerged PR #28 exposes `tl_loader::runtime::{ProcessorRunRequest, ProcessorRunReport, ProcessorOutcome}`; Chat 4 does not copy its processor runner. An accepted adapter must bind official Forge installer provenance, vetted SHA-1 outputs and Java compatibility, then validate the complete inherited Minecraft 1.16.5 instance.
- **Final audit:** `validate_staged_instance` audits exact required/optional inventories, checksum, size, symlinks, undeclared files and loader fields. Because Chat 3 attestation is **not merged** and generic booleans are forgeable, **this API currently returns `ready=false` even if synthetic fixture flags are true**. This is intentional fail-closed behavior. There is no atomic instance publication yet. Implementation of a private/non-serializable trusted loader attestation and an atomic stage-to-new-instance publish belongs after PR #28 is accepted.

## Windows real ZIP diagnostic harness (no actual API credentials embedded)

```powershell
cargo run --locked -p tl-pack --example rustic_waters_e2e -- "D:\packs\Rustic-Waters-II.zip" "D:\TLC-PrivateStages" scan-rw2-001
```

The command inspects, resolves with official API only if `CURSEFORGE_API_KEY` is present, and prints JSON. Without the key it reports every manifest file as blocked/NEEDS_ACTION. This is safe to run before API access. To fetch legally distributable mods into an **existing, trusted, empty parent directory** (no Forge launch or instance publication):

```powershell
cargo run --locked -p tl-pack --example rustic_waters_e2e -- "D:\packs\Rustic-Waters-II.zip" "D:\TLC-PrivateStages" scan-rw2-002 --stage-approved
```

Use a **new operation ID each time**. Do not share full local file paths or secrets in public CI logs. If API access is not authorized, do not stage; preserve blocked reports. No fake CDN mirror is used.

### Report counters

- `total`, `required`, `optional`: exact manifest counts.
- `resolved_official`: file identities with approved URL, upstream SHA-1 and game version.
- `blocked_file_count` and `blocked_items`: full unavailable list, including projectID/fileID and reason.
- `required_dependency_actions`, `optional_dependency_actions`: unresolved additional official dependency relations, not guessed fileIDs.
- `verified_downloads`, `installed_files[].sha256`: only checked bytes in private staging (if `--stage-approved`).
- `failed_downloads` and `failures`: explicit transfer failures; `G5=NOT_VERIFIED` and `ready=false` always.

## Fixture evidence / limitations

- Offline dependency fixture: 1 official primary file; 1 required unknown dependency, 1 optional unknown dependency; 0 real download attempts.
- Graph fixture: 3 nodes (2 required/1 optional), 1 missing required edge, 1 missing optional edge, 1 cycle.
- Local staged-byte fixture: one verified file, tampering causes validation failure, forged Forge booleans cannot authorize READY.
- Crash recovery tests create unrelated `level.dat` and `profiles.json` and confirm neither is deleted or modified on interrupt/failure/rollback.

This does **not** demonstrate Rustic Waters II mod install, Java 8 selection, Forge processor execution, modded Minecraft launch, or atomic game instance publication. Request those from Control only after Chat 3's trusted runtime merges into `develop`.
