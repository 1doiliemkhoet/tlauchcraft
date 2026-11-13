# Roadmap nghiên cứu → implementation → E2E

**Trạng thái hiện tại: chưa bắt đầu implementation.**

## 0. Khóa yêu cầu và chứng cứ
- [x] Baseline nghiên cứu offline/single/modpack/LAN v5
- [x] Tạo danh sách test và research gaps
- [ ] Xác minh lại các đoạn bytecode trọng yếu, adapter từng loader và metadata theo version cụ thể

## 1. Core local launcher
- [ ] Local profile và định danh ổn định
- [ ] Minecraft version JSON inheritance + OS/architecture rules + mainClass/arguments
- [ ] Downloader hash, staging, retry, atomic commit, caching
- [ ] Java runtime selector và process supervisor
- [ ] Chạy Vanilla đến main menu, tạo/load Singleplayer world (E2E)

## 2. Modpack
- [ ] **P0 Import CurseForge export ZIP** (`manifest.json` + `overrides/`), resolve authorized mod downloads, install as isolated instance (no mock)
- [ ] **P0 Import Modrinth `.mrpack`** (`modrinth.index.json`), hash/overrides/side filtering
- [ ] **P1 Import Prism/MultiMC exported ZIP**; generic ZIP import with explicit version and loader
- [ ] Archive safety, staging, rollback, missing-mod report and lockfile — [technical contract](docs/specs/MODPACK_ARCHIVE_IMPORT_v1.md)
- [ ] Instance isolation và modpack lockfile
- [ ] `.mrpack` importer, dependencies & client/server side filtering
- [ ] Fabric adapter + modded Singleplayer E2E
- [ ] Forge/NeoForge adapter E2E theo version được chốt
- [ ] World backup, diagnostics, rollback

## 3. LAN
- [ ] Dedicated modded LAN server instance (trusted network only)
- [ ] Client/server pack compatibility checks
- [ ] Direct Connect + world join/rejoin + inventory persist E2E trên **hai máy thật**
- [ ] Open-to-LAN compatibility tests (separate; không cam kết với mọi phiên bản)

## 4. UI/Packaging
- [ ] UI, mod manager, logs, progress, errors
- [ ] Windows packaging, update policy, crash support, docs

**Gate nghiệm thu:** chỉ đánh dấu PASS khi có log và hành vi thực tế; không dùng mock làm bằng chứng E2E.