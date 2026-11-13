# Chat 6 handoff — 2026-11-13

Scope: `crates/tl-skin` only; branch `feat/tl-06-skins`.

Initial self-contained implementation of PNG validation, skin library, persistence,
Classic/Slim model selection, basic pixel edits, integrity checks and consent-gated
LAN payload preparation. Not an in-game skin mod, LAN socket service, or 3D viewer.

**Contract proposals for Control (do not silently integrate):**

- Chat 1: supply UUID profile and instance types; map persistence to shared
  `ProfileStore` and `InstanceStore` without overlapping catalog ownership.
- Chat 2/3: define supported skin-provider adapter matrix keyed by game version
  and loader (Fabric first), with explicit unsupported states.
- Chat 5: specify LAN appearance frame, max payload, opt-in handshake,
  content hash, trust boundary, identity spoofing warning and cache invalidation.
- Chat 7: bind UI import/library/pixel editor; design rotatable 3D viewer and
  show three independent statuses (preview, self in-game, friends in-game).

**Evidence:** this branch alone cannot prove an actual skin displayed in
Singleplayer or visible on two computers. G7 remains unverified until explicit
device screenshots/logs and compatible provider execution are attached.

**Validation:** unit tests provided; CI and real Windows runs not evidenced here.
