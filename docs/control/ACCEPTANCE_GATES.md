# tlauchcraft gates

The authoritative gates and shared rules live in [CONTROL_ROOM.md](CONTROL_ROOM.md#evidence-gates).

## Verification report format
```text
Chat:
Branch:
Commit SHA:
PR:
Files changed:
Architecture/API changes:
Unit tests (command/results):
Integration tests (command/results):
Real E2E tests (device/version/loader/evidence):
Verified gates:
Unverified gates:
Risks:
Blocked by:
Next control decision:
```

E2E PASS requires real game/process/world or multi-PC LAN evidence; use NOT_TESTED where unexecuted.
