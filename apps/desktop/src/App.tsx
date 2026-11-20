import { startTransition, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { Activity, Box, CircleHelp, Download, FolderArchive, Gamepad2, HardDrive, LayoutDashboard, Monitor, Palette, Play, Settings2, ShieldAlert, Terminal, Trash2, Users, Wifi, XCircle } from 'lucide-react';
import { backendError, installFailure, terminalInstallEvent, canPlayInstance, canCancelInstall, validateJavaForm, backendStatusLine, backendStatusSignature, isInstallPending, downloadEventsForView, statusPollInterval } from './desktopState';
import { EMPTY_CAPABILITIES, negotiate, operations, safeArchive, safePercent, subscribeProgress, type Capabilities, type Feature, type GameInstance, type OfflineProfile, type ProgressEvent, type PackMetadata, type CurseForgeResolveReport, type CurseForgeStageReport, type StagedPackReceipt, type InstalledPackMod, type JavaSettings, type MemoryInfo, type GameStatus, type LaunchPreflight, type SkinRecord, type SkinModel } from './backend';


const PACK_PROGRESS_LABELS: Record<string, string> = {
  'pack.api_resolve': 'Đang lấy metadata từ CurseForge API',
  'pack.staging': 'Đang tải và xác minh các file mod',
  'pack.awaiting_verified_forge': 'Đã xong bước staging; cần kiểm tra kết quả trước khi cài Forge',
  'pack.rehash_official_mods': 'Đang kiểm tra lại SHA-1 từng mod',
  'forge.pinned_mojang_maven': 'Đang tải và kiểm tra Minecraft, Forge và thư viện chính thức',
  'forge.processor_output_verified': 'Đã xác minh processor; đang publish instance',
  'forge.ready_after_independent_audit': 'Instance đã được xác minh và publish',
  'local_import.manifest_ids': 'Đang đối chiếu danh sách mod JAR với metadata profile đã cài',
  'local_import.read_installed_folder': 'Đang kiểm tra thư mục CurseForge và dựng manifest local, không cần ZIP',
  'local_import.verifying_skin_addon': 'Đang tải và xác minh CustomSkinLoader',
  'local_import.hashing_mods': 'Đang tính SHA-1/SHA-256 cho mod JAR đã cài',
  'local_import.scanning_assets': 'Đang kiểm tra các thư mục cấu hình modpack',
  'local_import.copying_assets': 'Đang tạo snapshot cấu hình vào ZIP nội bộ',
  'local_import.snapshot_manifest_verified': 'Đã xác minh manifest nội bộ và cấu hình; bắt đầu stage mod',
  'local_import.copied_mods_hashed': 'Đã chép và hash từng mod JAR; đang chuẩn bị Forge chính thức',
  'local_import.publishing_verified_forge': 'Đang ghép Minecraft, Forge và modpack vào instance riêng',
  'local_import.ready_local_hashes_only': 'Đã publish bản local, chưa kiểm chứng game chạy thật',
};

type Page = 'home' | 'profiles' | 'instances' | 'modpacks' | 'skins' | 'lan' | 'downloads' | 'settings' | 'logs';
const navigation = [
  { id: 'home', label: 'Overview', icon: LayoutDashboard },
  { id: 'profiles', label: 'Offline profiles', icon: Users },
  { id: 'instances', label: 'Instances & versions', icon: Box },
  { id: 'modpacks', label: 'Modpack library', icon: FolderArchive },
  { id: 'skins', label: 'Skin studio', icon: Palette },
  { id: 'lan', label: 'LAN multiplayer', icon: Wifi },
  { id: 'downloads', label: 'Downloads', icon: Download },
  { id: 'settings', label: 'Settings', icon: Settings2 },
  { id: 'logs', label: 'Activity logs', icon: Terminal }
] as const;
const featurePage: Partial<Record<Page, Feature>> = { profiles: 'profiles', instances: 'instances', modpacks: 'modpacks', skins: 'skins', lan: 'lan', downloads: 'downloads', settings: 'settings' };
function Notice({ children }: { children: React.ReactNode }) { return <div className="notice"><ShieldAlert size={18}/><div>{children}</div></div>; }
function Unavailable({ feature }: { feature: Feature }) { return <Notice><strong>Backend chưa khả dụng: {feature}</strong><p>Phần này đang chờ Rust service và lệnh Tauri tương ứng. Không có tác vụ nào được thực hiện hoặc dữ liệu nào được giả lập.</p></Notice>; }
function Empty({ title, description }: { title: string, description: string }) { return <div className="empty"><HardDrive size={28}/><strong>{title}</strong><p>{description}</p></div>; }
export default function App() {
  const [page, setPage] = useState<Page>('home');
  const [caps, setCaps] = useState<Capabilities>(EMPTY_CAPABILITIES);
  const [profiles, setProfiles] = useState<OfflineProfile[]>([]);
  const [instances, setInstances] = useState<GameInstance[]>([]);
  const [username, setUsername] = useState('');
  const [selectedProfile, setSelectedProfile] = useState('');
  const [selectedInstance, setSelectedInstance] = useState('');
  const [events, setEvents] = useState<ProgressEvent[]>([]);
  const [showAllEvents, setShowAllEvents] = useState(false);
  const [packPreview, setPackPreview] = useState<PackMetadata | null>(null);
  const [packArchivePath, setPackArchivePath] = useState<string | null>(null);
  const [packResolution, setPackResolution] = useState<CurseForgeResolveReport | null>(null);
  const [packStaging, setPackStaging] = useState<CurseForgeStageReport | null>(null);
  const [packReceipt, setPackReceipt] = useState<StagedPackReceipt | null>(null);
  const [packOperationId, setPackOperationId] = useState<string | null>(null);
  const [packManualImports, setPackManualImports] = useState<InstalledPackMod[]>([]);
  const [forgeJavaPath, setForgeJavaPath] = useState<string | null>(null);
  const [forgeHostConsent, setForgeHostConsent] = useState(false);
  const [lanProperties, setLanProperties] = useState(false);
  const [customSkin, setCustomSkin] = useState(false);
  const [skins, setSkins] = useState<SkinRecord[]>([]);
  const [skinModel, setSkinModel] = useState<SkinModel>('classic');
  const [skinName, setSkinName] = useState('My skin');
  const [skinId, setSkinId] = useState('');
  const [skinPreview, setSkinPreview] = useState('');
  const [skinMessage, setSkinMessage] = useState('PreviewOnly — select a profile and skin.');
  const [installedCurseForgeFolder, setInstalledCurseForgeFolder] = useState<string | null>(null);
  const [logs, setLogs] = useState<string[]>([]);
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);
  const [activeTask, setActiveTask] = useState<string | null>(null);
  const [curseForgeKeyConfigured, setCurseForgeKeyConfigured] = useState<boolean | null>(null);
  const [playReady, setPlayReady] = useState<LaunchPreflight | null>(null);
  const [playReadinessError, setPlayReadinessError] = useState<string | null>(null);
  const [newInstanceName, setNewInstanceName] = useState('Vanilla');
  const [newVersion, setNewVersion] = useState('1.21.1');
  const [javaSettings, setJavaSettings] = useState<JavaSettings | null>(null);
  const [memoryInfo, setMemoryInfo] = useState<MemoryInfo | null>(null);
  const [gameStatusById, setGameStatusById] = useState<Record<string, GameStatus>>({});
  const statusSignatures = useRef<Record<string,string>>({});
  const log = useCallback((line: string) => setLogs(old => [`${new Date().toLocaleTimeString()} · ${line}`, ...old].slice(0, 250)), []);
  const run = useCallback(async (label: string, fn: () => Promise<unknown>) => {
    setBusy(true); setActiveTask(label); setError(undefined);
    log(`${label}: started`);
    try { await fn(); log(`${label}: backend responded; verify status below (not a completed install)`); }
    catch (e) { const message = backendError(e); setError(message); log(`${label} failed: ${message}`); }
    finally { setBusy(false); setActiveTask(null); }
  }, [log]);
  const refresh = useCallback(async (c: Capabilities) => {
    const tasks = [
      c.features.profiles ? operations.listProfiles(c).then(setProfiles) : Promise.resolve(),
      c.features.instances ? operations.listInstances(c).then(setInstances) : Promise.resolve()
    ];
    const results = await Promise.allSettled(tasks);
    results.forEach(r => { if (r.status === 'rejected') {
      const message = 'Backend list failed: ' + backendError(r.reason);
      setError(message); log(message);
    } });
  }, [log]);
  useEffect(() => {
    if (page==='skins' && caps.features.skins) {
      void operations.listSkins(caps).then(setSkins).catch(e=>setError(backendError(e)));
    }
  }, [page,caps]);
  useEffect(() => {
    let active = true;
    let unsubscribe: (() => void) | undefined;
    const onEvent = (event: ProgressEvent) => {
      if (!active) return;
      setEvents(old => [event, ...old].slice(0, 100));
      const failure = installFailure(event);
      if (failure) setError(failure);
      log(`backend event ${event.phase}: ${event.message_key}`);
    };
    // Install event listeners BEFORE exposing the backend as ready to the UI.
    // StrictMode may unmount during async registration: dispose even late listeners.
    const initialise = async () => {
      try {
        const stop = await subscribeProgress(onEvent);
        if (!active) stop(); else unsubscribe = stop;
      } catch (e) {
        if (active) log('Event subscription failed (' + backendError(e) + '); game status polling remains available.');
      }
      if (!active) return;
      const negotiated = await negotiate();
      if (!active) return;
      setCaps(negotiated);
      await refresh(negotiated);
      if (active) log(`Backend provider: ${negotiated.provider}`);
    };
    void initialise();
    return () => { active = false; unsubscribe?.(); };
  }, [refresh, log]);
  useEffect(() => {
    if (page !== 'modpacks' || !caps.features.pack_inspection) return;
    let alive = true;
    setCurseForgeKeyConfigured(null);
    void operations.curseForgeKeyConfigured(caps)
      .then(found => { if (alive) setCurseForgeKeyConfigured(found); })
      .catch(e => {
        if (alive) {
          setCurseForgeKeyConfigured(null);
          log('CurseForge API key preflight failed: ' + backendError(e));
        }
      });
    return () => { alive = false; };
  }, [page, caps, log]);
  const importFile = async (path?: string) => {
    if (!caps.features.pack_inspection) { setError('Archive inspector unavailable.'); return; }
    let input = path;
    if (!input) { const selected = await open({ multiple: false, filters: [{ name: 'Modpack archives', extensions: ['zip', 'mrpack'] }] }); if (typeof selected === 'string') input = selected; }
    if (!input) return;
    if (!safeArchive(input)) { setError('Chỉ nhận .zip hoặc .mrpack. Backend sẽ kiểm tra nội dung archive.'); return; }
    setPackPreview(null);
    setPackArchivePath(null);
    setPackResolution(null);
    setPackStaging(null);
    setPackReceipt(null);
    setPackOperationId(null);
    setPackManualImports([]);
    setForgeHostConsent(false);
    setInstalledCurseForgeFolder(null);
    setLanProperties(false);
    await run('Inspect modpack', async () => { const metadata = await operations.inspectPack(caps, input!); setPackPreview(metadata); setPackArchivePath(input!); });
  };
  const resolvePackFiles = () => {
    if (!packArchivePath || packPreview?.source !== 'curseforge_zip' || curseForgeKeyConfigured !== true) return;
    void run('Resolve authorized CurseForge files', async () => {
      const plan = await operations.resolveCurseForge(caps, packArchivePath);
      setPackResolution(plan);
      log(`Official resolver: ${plan.resolved_count} downloadable, ${plan.needs_action_count} needing action. No mods installed.`);
    });
  };
  const stagePackFiles = () => {
    if (!packArchivePath || packPreview?.source !== 'curseforge_zip' || curseForgeKeyConfigured !== true) return;
    const operationId = `pack-${crypto.randomUUID()}`;
    setPackOperationId(operationId);
    setPackStaging(null);
    setPackReceipt(null);
    setPackManualImports([]);
    void run('Download and verify to private staging', async () => {
      const staged = await operations.stageCurseForge(caps, packArchivePath, operationId);
      setPackStaging(staged);
      log(`Staged ${staged.installed.length} independently checked mod files. Blocked: ${staged.blocked.length}; failed: ${staged.failures.length}. PLAY IS NOT READY.`);
    });
  };
  const verifyPackInventory = () => {
    if (!packArchivePath || !packOperationId) return;
    void run('Rehash staged pack inventory', async () => {
      const evidence = await operations.stagedPackReceipt(caps, packArchivePath, packOperationId);
      setPackReceipt(evidence);
      log(`On-disk SHA-1 rehash: ${evidence.verified.length} files, missing required ${evidence.blocked_required.length}, errors ${evidence.failures.length}. Forge is not verified.`);
    });
  };
  const importManualMod = (projectId: number, fileId: number) => {
    if (!packArchivePath || !packOperationId) return;
    void (async () => {
      const path = await open({ multiple: false, filters: [{ name: 'Provider-authenticated mod', extensions: ['jar'] }] });
      if (typeof path !== 'string') return;
      await run('Verify manual restricted mod', async () => {
        const verified = await operations.importRestrictedPackMod(
          caps, packArchivePath, packOperationId, projectId, fileId, path
        );
        setPackManualImports(current => [...current, verified]);
        setPackReceipt(null);
        log(`SHA-1 matched official CurseForge file ${projectId}/${fileId}; staged only, not READY.`);
      });
    })().catch(e => setError(backendError(e)));
  };
  const importInstalledFolder = () => {
    if (!installedCurseForgeFolder || !selectedProfile ||
        !forgeHostConsent || busy) return;
    const operationId = `local-${crypto.randomUUID()}`;
    setPackOperationId(operationId);
    void run('Import installed CurseForge folder', async () => {
      const instance = await operations.importInstalledFolder(
        caps, installedCurseForgeFolder, operationId,
        selectedProfile, forgeJavaPath ?? '', forgeHostConsent, lanProperties, customSkin
      );
      log(`Installed folder imported as ${instance.id}. Local JAR hashes are NOT provider-authenticated. Real gameplay NOT_VERIFIED.`);
      await refresh(caps);
      setSelectedInstance(instance.id);
      setPage('instances');
    });
  };
  const installVerifiedForge = () => {
    if (!packArchivePath || !packOperationId || !selectedProfile ||
        !forgeHostConsent || !packPreview) return;
    void run('Install reviewed official Forge modpack', async () => {
      const installed = await operations.installOfficialForge(
        caps, packArchivePath, packOperationId, packPreview.name,
        selectedProfile, forgeJavaPath ?? '', forgeHostConsent
      );
      log('Forge instance ' + installed.id + ' READY after hash+SQLite verification; real gameplay NOT_VERIFIED.');
      await refresh(caps);
      setSelectedInstance(installed.id);
      setPage('instances');
    });
  };
  useEffect(() => {
    if (page !== 'settings' || !selectedInstance || !caps.features.settings) {
      setJavaSettings(null);
      return;
    }
    let alive = true;
    Promise.all([operations.javaSettingsGet(caps, selectedInstance), operations.javaMemoryInfo(caps)])
      .then(([prefs, ram]) => { if (alive) { setJavaSettings(prefs); setMemoryInfo(ram); } })
      .catch(e => { if (alive) { setJavaSettings(null); setError(String(e)); } });
    return () => { alive = false; };
  }, [page, selectedInstance, caps]);
  useEffect(() => {
    if (!events.length || !caps.features.instances) return;
    const latest=events[0];
    if (terminalInstallEvent(latest) && latest.instance_id) {
      void refresh(caps);
      void operations.gameStatus(caps,latest.instance_id).then(status => {
        setGameStatusById(old => ({...old,[latest.instance_id!]:status}));
        if (status.last_error) setError(status.last_error);
      }).catch(e => setError(backendError(e)));
    }
  }, [events, caps, refresh]);
  const instanceIds = useMemo(() => instances.filter(i=>i.loader.kind==='vanilla'||(i.loader.kind==='forge'&&i.state==='ready')).map(i=>i.id).join('|'), [instances]);
  useEffect(() => {
    if (!caps.features.game || !instanceIds) return;
    let active = true;
    let fetching = false;
    const ids = instanceIds.split('|');
    const poll = async () => {
      if (!active || fetching) return;
      fetching = true;
      try {
        const results = await Promise.allSettled(ids.map(async id => ({
          id, status: await operations.gameStatus(caps,id)
        })));
        if (!active) return;
        const updated: Record<string,GameStatus> = {};
        let changedInstallState = false;
        for (const result of results) {
          if (result.status !== 'fulfilled') continue;
          const {id,status} = result.value;
          updated[id] = status;
          const signature = backendStatusSignature(status);
          if (statusSignatures.current[id] !== signature) {
            statusSignatures.current[id] = signature;
            log(backendStatusLine(status));
            if (status.last_error) setError(status.last_error);
            if (status.install_state !== instances.find(x => x.id === id)?.state) changedInstallState = true;
          }
        }
        setGameStatusById(old => {
          const changed = Object.entries(updated).some(([id,status]) =>
            !old[id] || backendStatusSignature(old[id]) !== backendStatusSignature(status));
          return changed ? {...old,...updated} : old;
        });
        // State is read from Rust/SQLite, not guessed from a timer.
        if (changedInstallState) void refresh(caps);
      } catch (e) {
        if (active) log('Backend status query failed: ' + backendError(e));
      } finally { fetching = false; }
    };
    void poll();
    // Active installs update frequently; idle status uses a low-cost read-only fallback.
    const timer = window.setInterval(() => { void poll(); }, statusPollInterval(Object.values(gameStatusById)));
    return () => { active = false; window.clearInterval(timer); };
  }, [caps, instanceIds, refresh, log, gameStatusById]);
  useEffect(() => {
    let active=true;
    setPlayReady(null); setPlayReadinessError(null);
    const instance=instances.find(i=>i.id===selectedInstance);
    if (caps.features.game && instance?.state==='ready' && instance.profile_id===selectedProfile) {
      operations.gamePreflight(caps,selectedInstance,selectedProfile)
        .then(info=>{if(active)setPlayReady(info);})
        .catch(e=>{if(active)setPlayReadinessError(String(e));});
    }
    return ()=>{active=false;};
  }, [caps,instances,selectedInstance,selectedProfile,javaSettings?.revision]);
  const navigate = useCallback((next: Page) => {
    const before = performance.now();
    startTransition(() => setPage(next));
    setError(undefined);
    requestAnimationFrame(() => requestAnimationFrame(() => {
      log(`UI navigation ${next}: ${(performance.now() - before).toFixed(1)}ms (two animation frames, device measurement)`);
    }));
  }, [log]);
  const visibleEvents = downloadEventsForView(events, showAllEvents);
  const latestPackEvent = packOperationId ? events.find(event => event.operation_id === packOperationId) : undefined;
  const current = navigation.find(n => n.id === page)!;
  const selected = instances.find(i => i.id === selectedInstance);
  return <div className="shell" onDragOver={e => { if (e.dataTransfer.types.includes('Files')) e.preventDefault(); }} onDrop={e => {
    // Browser File.path is intentionally not trusted; native OS dialog is the approved source for paths.
    e.preventDefault(); setPage('modpacks'); setError('Kéo thả đã nhận diện. Hãy chọn ZIP bằng hộp thoại hệ thống để lấy đường dẫn an toàn.');
  }}>
    <aside className="sidebar">
      <div className="brand"><div className="brand-icon"><Gamepad2 size={24}/></div><div><b>tlauchcraft</b><small>OFFLINE / LAN LAUNCHER</small></div></div>
      <div className="side-heading">WORKSPACE</div>
      <nav aria-label="Main navigation">{navigation.map(item => <button type="button" key={item.id} className={page === item.id ? 'nav active' : 'nav'} onClick={() => navigate(item.id)}><item.icon size={18}/>{item.label}{featurePage[item.id as Page] && caps.features[featurePage[item.id as Page]!] && <span className="ready-dot"/>}</button>)}</nav>
      <div className="sidebar-foot"><div className="tiny-status"><span className={Object.values(caps.features).some(Boolean) ? 'status-dot green' : 'status-dot'}/> {caps.provider === 'unavailable' ? 'Core not connected' : caps.provider}</div><small>Build 0.1.0 · Development preview</small></div>
    </aside>
    <main className="main">
      <header className="topbar"><div className="breadcrumb">Launcher <span>/</span> {current.label}</div><div className="top-actions"><span className="chip"><Monitor size={13}/> Windows Desktop</span><span className="chip subtle">LOCAL PROFILE ONLY</span></div></header>
      <div className="content">
        <div className="heading"><div><div className="eyebrow">TLAUCHCRAFT CONTROL CENTER</div><h1>{current.label}</h1><p>{page === 'home' ? 'Your Minecraft worlds, modpacks and LAN sessions in one place.' : 'Status and actions are backed by the real Rust facade when available.'}</p></div><span className="version-tag">PREVIEW · NOT E2E VERIFIED</span></div>
        {activeTask && <div className="activity-banner" role="status" aria-live="polite">
          <div className="activity-head"><Activity size={18}/><strong>Đang chạy: {activeTask}</strong></div>
          {page === 'modpacks' && latestPackEvent
            && <div>{PACK_PROGRESS_LABELS[latestPackEvent.message_key] ?? `Backend: ${latestPackEvent.phase} · ${latestPackEvent.message_key}`}</div>}
          {page === 'modpacks' && latestPackEvent &&
            typeof latestPackEvent.details?.completed_files === 'string' &&
            typeof latestPackEvent.details?.total_files === 'string' &&
            <small>Đã xử lý {latestPackEvent.details.completed_files}/{latestPackEvent.details.total_files} mục ở giai đoạn này (số file thật, không phải % thời gian).</small>}
          <progress aria-label="Đang xử lý, chưa có tổng dung lượng để tính phần trăm"/>
          <small>Đang chờ backend hoàn tất. Thanh tiến trình không hiển thị % giả khi chưa có số byte xác minh.</small>
          {page === 'modpacks' && packOperationId && <button type="button" onClick={() => void operations.cancelPackStage(caps, packOperationId).catch(e => setError(backendError(e)))}>Yêu cầu hủy tác vụ modpack</button>}
        </div>}
        {error && <div role="alert" className="error"><XCircle size={18}/><span>{error}</span><button aria-label="Dismiss error" onClick={() => setError(undefined)}>×</button></div>}
        {page === 'home' && <>
          <div className="hero"><div className="hero-text"><div className="eyebrow">YOUR NEXT ADVENTURE</div><h2>Make room for<br/><em>better adventures.</em></h2><p>Build an isolated modded instance, choose an offline profile and launch only when the core is ready.</p><button className="primary" disabled={!caps.features.game || busy || !canPlayInstance(selected, selectedProfile, playReady, gameStatusById[selectedInstance])} onClick={() => void run('Launch', () => operations.launch(caps, { instance_id: selectedInstance, profile_id: selectedProfile }))}><Play size={16} fill="currentColor"/> Launch Minecraft</button></div><div className="hero-art"><div className="sun"/><div className="mountain m1"/><div className="mountain m2"/><div className="mountain m3"/><div className="grid-art"/></div></div>
          <div className="metric-grid"><div className="panel metric"><Box/><strong>{instances.length}</strong><span>Known instances</span></div><div className="panel metric"><Users/><strong>{profiles.length}</strong><span>Offline profiles</span></div><div className="panel metric"><Download/><strong>{events.length}</strong><span>Received events</span></div></div>
          <div className="panel"><h3>Launch readiness</h3><p className="muted">Choose a real profile and ready instance. Actions remain unavailable while their service is missing.</p><div className="form-row"><select aria-label="Selected profile" value={selectedProfile} onChange={e => setSelectedProfile(e.target.value)}><option value="">Select profile</option>{profiles.map(p => <option value={p.id} key={p.id}>{p.username}</option>)}</select><select aria-label="Selected instance" value={selectedInstance} onChange={e => setSelectedInstance(e.target.value)}><option value="">Select instance</option>{instances.filter(i => i.state === 'ready').map(i => <option value={i.id} key={i.id}>{i.display_name} · #{i.id.slice(0,8)} · {i.minecraft_version}</option>)}</select></div>{playReady && <div className="item">Java {playReady.required_java} · Xms {playReady.xms_mb} MB · Xmx {playReady.xmx_mb} MB <span>Verified for launch</span></div>}{playReadinessError && <Notice>Play unavailable: {playReadinessError}</Notice>}{!caps.features.game && <Unavailable feature="game"/>}</div>
        </>}
        {page === 'profiles' && <div className="panel"><h3>Offline identities</h3><p className="muted">A persistent UUID is provided by the core, not derived from the editable nickname.</p><div className="form-row"><input aria-label="Minecraft username" placeholder="Minecraft username" maxLength={16} value={username} onChange={e => setUsername(e.target.value)}/><button className="primary" disabled={!caps.features.profiles || !/^[A-Za-z0-9_]{3,16}$/.test(username) || busy} onClick={() => void run('Create profile', async () => { await operations.createProfile(caps, username); setUsername(''); await refresh(caps); })}>Create profile</button></div>{profiles.map(p => <div className="item" key={p.id}><Users size={18}/><strong>{p.username}</strong><span className="mono">{p.id}</span></div>)}{!profiles.length && <Empty title="No local profiles loaded" description="Create one when the profile service is connected."/>}{!caps.features.profiles && <Unavailable feature="profiles"/>}</div>}
        {page === 'instances' && <div className="panel">
          <h3>Vanilla and verified Forge instances</h3>
          <p className="muted">Vanilla creation and installation remain separate. Published Forge instances become playable only after backend receipt and file verification.</p>
          <div className="form-row">
            <input aria-label="Instance name" value={newInstanceName} onChange={e => setNewInstanceName(e.target.value)} placeholder="Instance name"/>
            <input aria-label="Minecraft release version" value={newVersion} onChange={e => setNewVersion(e.target.value)} placeholder="Minecraft release"/>
            <select aria-label="Profile for new instance" value={selectedProfile} onChange={e => setSelectedProfile(e.target.value)}>
              <option value="">Choose offline profile</option>
              {profiles.map(p => <option key={p.id} value={p.id}>{p.username}</option>)}
            </select>
            <button disabled={!caps.features.game || !selectedProfile || !newInstanceName.trim() || !/^[0-9A-Za-z._+-]+$/.test(newVersion) || busy}
              onClick={() => void run('Create Vanilla instance', async () => {
                const instance = await operations.createVanillaInstance(caps, { name: newInstanceName, version: newVersion, profile_id: selectedProfile });
                setSelectedInstance(instance.id); await refresh(caps);
              })}>Create Vanilla</button>
          </div>
          {instances.map(i => <div className="item" key={i.id}>
            <Box size={18}/><strong>{i.display_name}</strong>
            <small className="mono" title={i.id}>#{i.id.slice(0,8)}</small>
            <span>{i.minecraft_version} · {i.loader.kind}{i.loader.version ? ' ' + i.loader.version : ''}</span>
            <span className="chip">{i.state}</span>
            {gameStatusById[i.id]?.process_state === 'running' && <span>PID {gameStatusById[i.id]?.pid}</span>}
            {i.loader.kind === 'vanilla' && <>
              <button disabled={!caps.features.game || busy || i.loader.kind !== 'vanilla' || !['empty','failed','cancelled','needs_action'].includes(i.state)} onClick={() => void run('Install Vanilla', async () => {
                await operations.installVanilla(caps, i.id);
                log('Install command queued for ' + i.display_name + '; awaiting backend downloading/verifying/ready or failed.');
                await refresh(caps);
                const status = await operations.gameStatus(caps,i.id);
                setGameStatusById(old => ({...old,[i.id]:status}));
                if (status.last_error) setError(status.last_error);
              })}>Install Vanilla</button>
              <button disabled={!caps.features.game || busy} onClick={() => void run('Check real status', async () => {
                const status = await operations.gameStatus(caps,i.id);
                setGameStatusById(old => ({...old,[i.id]:status}));
                if (status.last_error) setError(status.last_error);
                await refresh(caps);
              })}>Status</button>
              <button disabled={!caps.features.game || busy || !canCancelInstall(gameStatusById[i.id])} onClick={() => void run('Cancel install', async () => {
                await operations.cancelInstall(caps,i.id);
                log('Cancellation requested for ' + i.display_name + '; worker has not confirmed cancelled yet.');
                await refresh(caps);
              })}>Cancel install</button>
              <button disabled={!caps.features.game || busy || gameStatusById[i.id]?.process_state !== 'running'} onClick={() => void run('Stop Minecraft', async () => {
                await operations.stopGame(caps,i.id);
                const status=await operations.gameStatus(caps,i.id); setGameStatusById(old=>({...old,[i.id]:status}));
              })}>Stop</button>
            </>}
            <button disabled={!caps.features.settings || busy}
              onClick={() => { setSelectedInstance(i.id);setPage('settings'); }}>Java & Memory</button>
            <button aria-label={`Remove instance ${i.display_name} ${i.id.slice(0,8)}`}
              disabled={!caps.features.instances || busy || !['empty','ready','needs_action','failed','cancelled'].includes(i.state) || gameStatusById[i.id]?.process_state === 'running'}
              onClick={() => {
                if (!window.confirm(`Remove instance "${i.display_name}" (#${i.id.slice(0,8)}) from the launcher?\n\nIts private files, including saves/worlds, will be preserved in the TLaunchCraft deleted-instances folder. The original CurseForge modpack is never touched.`)) return;
                void run('Remove instance',async()=>{
                  const removed=await operations.removeInstance(caps,i.id,i.id);
                  log(`Removed instance ${removed.id}. Recovery folder: ${removed.archived_path ?? '(no private files)'}`);
                  if (selectedInstance===i.id) setSelectedInstance('');
                  setGameStatusById(old=>{const next={...old};delete next[i.id];return next;});
                  await refresh(caps);
                });
              }}><Trash2 size={14}/> Remove</button>
          </div>)}
          {!instances.length && <Empty title="No instances" description="Create a real Vanilla instance linked to your offline profile."/>}
          <Notice>Install is asynchronous. Current status is read from Rust/SQLite; stage events are not completion confirmations. Exact download percentage is unavailable until tl-game reports verified byte counts.</Notice>
          {instances.filter(i=>gameStatusById[i.id]?.last_error).map(i=><Notice key={i.id}>
            {i.display_name}: {gameStatusById[i.id]?.last_error}
          </Notice>)}
          {!caps.features.game && <Unavailable feature="game"/>}
        </div>}
        {page === 'modpacks' && <div className="panel"><div className="panel">
        <h3>Import Installed CurseForge — Chỉ cần thư mục</h3>
        <p className="muted">Chọn folder modpack CurseForge đã cài đầy đủ, có minecraftinstance.json và mods/. Không cần ZIP gốc, CurseForge API key hay tải lại mod.</p>
        <p className="muted">Launcher dùng metadata local để xác định Minecraft/Forge và danh sách mod, sao chép các JAR cùng cấu hình thường dùng vào instance riêng, hash lại file và không chạm tới saves/worlds gốc. Hỗ trợ release đã kiểm định: Forge 1.16.5 (Java 8) và 1.20.1/47.4.0 (Java 17); gameplay phiên bản mới cần test thực tế.</p>
        <div className="form-row">
          <button disabled={busy} onClick={() => void (async () => {
            const path = await open({ directory: true, multiple: false });
            if (typeof path === 'string') {
              setInstalledCurseForgeFolder(path);
              setLanProperties(false);
            }
          })().catch(e => setError(backendError(e)))}>Chọn thư mục modpack đã cài</button>
          {installedCurseForgeFolder && <span className="mono">{installedCurseForgeFolder}</span>}
        </div>
        <div className="form-row">
          <select aria-label="Offline player for installed folder" value={selectedProfile} onChange={e => setSelectedProfile(e.target.value)}>
            <option value="">Chọn tài khoản Offline</option>
            {profiles.map(p => <option key={p.id} value={p.id}>{p.username}</option>)}
          </select>
          <button disabled={busy} onClick={() => setForgeJavaPath(null)}>Tự động chọn Java theo modpack</button>
          <button disabled={busy} onClick={() => void (async () => {
            const path = await open({ multiple: false, filters: [{ name: 'Java executable', extensions: ['exe'] }] });
            if (typeof path === 'string') setForgeJavaPath(path);
          })().catch(e => setError(backendError(e)))}>Chọn Java thủ công</button>
        </div>
        <p className="mono" role="status">{forgeJavaPath ? 'Java: ' + forgeJavaPath : 'Java: tự động tìm đúng Java 8 hoặc 17 theo modpack.'}</p>
        <label><input type="checkbox" checked={forgeHostConsent} onChange={e => setForgeHostConsent(e.target.checked)}/>
          Tôi đồng ý chạy Java Forge và các mod từ thư mục đã cài trên tài khoản Windows này. Hash local không chứng thực nguồn phát hành; Windows Job Object không phải sandbox bảo vệ file/mạng.
        </label>
        <label><input type="checkbox" checked={lanProperties} disabled={busy} onChange={e => setLanProperties(e.target.checked)}/>
          LAN Properties (thử nghiệm, chỉ Forge 1.20.1/47.4.0): tải bản cố định từ Modrinth, xác minh hash và thêm vào instance mới. Không thay đổi xác thực tài khoản. Không bật mặc định.
        </label>
        <label><input type="checkbox" checked={customSkin} disabled={busy} onChange={e => setCustomSkin(e.target.checked)}/>
          Custom Skin (Forge 1.16.5 / 1.20.1): tải CustomSkinLoader chính thức, kiểm tra hash, thêm vào instance mới. Skin chỉ dùng local; chưa nghiệm thu LAN.
        </label>
        <div className="form-row">
          <button className="primary" disabled={busy || !caps.features.pack_inspection || !installedCurseForgeFolder || !selectedProfile || !forgeHostConsent} onClick={importInstalledFolder}>Import Folder → Install Forge → Play</button>
        </div>
      </div>
      <h3>Optional: Inspect a modpack archive</h3><p className="muted">ZIP/MRPACK là tùy chọn dành cho việc kiểm tra manifest và quy trình tải mod qua API. Nếu CurseForge đã cài đủ mod, chỉ cần Import Folder bên trên.</p><div className="dropzone"><FolderArchive size={38}/><strong>Choose an exported modpack archive</strong><p>Only a native file picker passes a path to the backend. Drag-and-drop opens this screen without reading a browser-provided path.</p><button className="primary" disabled={!caps.features.pack_inspection || busy} onClick={() => void importFile()}>Inspect ZIP / MRPACK</button></div>{packPreview && <div className="panel"><h3>Read-only inspection result</h3><div className="item"><strong>{packPreview.name}</strong><span>{packPreview.game_version} · {packPreview.loader.kind}</span></div><div className="item">Archive hash (SHA-256) <span className="mono">{packPreview.archive_sha256}</span></div><div className="item">Entries <span>{packPreview.archive_entries}</span></div>{packPreview.warnings.map((warning, i) => <Notice key={i}>{warning}</Notice>)}<Notice>Inspected only: the modpack has NOT been installed, verified for play, or launched.</Notice></div>}{packPreview?.source === 'curseforge_zip' && packArchivePath && <div className="panel">
          <h3>CurseForge — verified file staging</h3>
          {curseForgeKeyConfigured === false && <Notice><strong>Chưa cấu hình CurseForge API key.</strong> Không có tác vụ tải mod nào được bắt đầu. Thiết lập biến môi trường <code>CURSEFORGE_API_KEY</code>, thoát hoàn toàn launcher và mở lại rồi mới Resolve/Download.</Notice>}
          {curseForgeKeyConfigured === null && <p className="muted" role="status">Đang kiểm tra cấu hình CurseForge API key...</p>}
          {curseForgeKeyConfigured === true && <p className="muted">Đã tìm thấy CurseForge API key trong môi trường backend (chưa kiểm tra quyền truy cập API).</p>}
          <Notice>Staging is not an installed game. Only the explicitly approved, release-pinned Forge adapter can run Java and publish a hash-verified instance. Minecraft gameplay remains NOT_VERIFIED until tested.</Notice>
          <div className="form-row">
            <button disabled={busy || curseForgeKeyConfigured !== true} onClick={resolvePackFiles}>Resolve official dependencies</button>
            <button className="primary" disabled={busy || curseForgeKeyConfigured !== true} onClick={stagePackFiles}>Download / Verify to private staging</button>
            <button disabled={!packOperationId || !busy} onClick={() => void operations.cancelPackStage(caps, packOperationId!).catch(e => setError(backendError(e)))}>Cancel staging</button>
          </div>
          {packResolution && <div className="item"><strong>Official API plan</strong><span>{packResolution.resolved_count} downloadable · {packResolution.needs_action_count} need action</span></div>}
          {packStaging && <div className="panel">
            <div className="item"><strong>Staging: {packStaging.status}</strong><span>Verified {packStaging.installed.length} · Blocked {packStaging.blocked.length} · Failed {packStaging.failures.length} · Manually verified {packManualImports.length}</span></div>
            <div className="form-row"><button disabled={busy} onClick={verifyPackInventory}>Rehash all installed mods / review receipt</button></div>
            {packReceipt && <div className="panel">
              <div className="item"><strong>Installed-mod inventory: {packReceipt.complete_mod_inventory ? 'COMPLETE (mods only)' : 'INCOMPLETE'}</strong>
              <span>SHA-1 verified {packReceipt.verified.length} / {packReceipt.required_total + packReceipt.optional_total} · Missing required {packReceipt.blocked_required.length} · Errors {packReceipt.failures.length}</span></div>
              {packReceipt.blocked_required.map((e, i) => <Notice key={'required'+i}>{e}</Notice>)}
              {packReceipt.failures.map((e, i) => <Notice key={'error'+i}>{e}</Notice>)}
              <Notice>Staged mods only: no Forge processor execution, no published instance, no gameplay evidence yet.</Notice>
              {(packPreview.game_version === '1.16.5' && ['36.2.29', '36.2.34', '36.2.42'].includes(packPreview.loader.version ?? '')) ||
                (packPreview.game_version === '1.20.1' && packPreview.loader.version === '47.4.0')
                ? <div className="panel">
                    <h3>Install reviewed official Forge {packPreview.game_version} / {packPreview.loader.version}</h3>
                    <p className="muted">The backend independently downloads and verifies official Forge/Mojang/Maven inputs, runs exactly the reviewed processors in a bounded Windows Job Object, then independently audits and publishes the modpack. The program runs under your Windows account; a Job Object DOES NOT block access to your files or network. Windows Sandbox offers stronger isolation.</p>
                    <div className="form-row">
                      <select aria-label="Offline identity for Forge modpack" value={selectedProfile} onChange={e => setSelectedProfile(e.target.value)}>
                        <option value="">Choose an offline profile</option>
                        {profiles.map(p => <option key={p.id} value={p.id}>{p.username}</option>)}
                      </select>
                      <button disabled={busy} onClick={() => void (async () => {
                        const path = await open({ multiple: false, filters: [{ name: 'Java executable', extensions: ['exe'] }] });
                        if (typeof path === 'string') setForgeJavaPath(path);
                      })().catch(e => setError(backendError(e)))}>Select installed Java .exe</button>
                      <button disabled={busy} onClick={() => setForgeJavaPath(null)}>Auto Java</button>
                      <span className="mono">{forgeJavaPath ?? 'Auto: required Java ' + (packPreview.game_version === '1.20.1' ? '17' : '8')}</span>
                    </div>
                    <label><input type="checkbox" checked={forgeHostConsent} onChange={e => setForgeHostConsent(e.target.checked)}/>
                      I approve running verified official Forge processor Java on this Windows account, understanding that a Job Object is NOT a file/network sandbox.
                    </label>
                    <div className="form-row">
                      <button className="primary" disabled={
                        busy || !packOperationId || !packReceipt.complete_mod_inventory ||
                        packReceipt.blocked_required.length !== 0 || packReceipt.blocked_optional.length !== 0 ||
                        packReceipt.failures.length !== 0 || !selectedProfile || !forgeHostConsent
                      } onClick={installVerifiedForge}>Install verified Forge instance</button>
                    </div>
                    <p className="muted">Only a complete source-verified mod inventory is eligible. Any missing or modified file refuses installation; no existing world is overwritten.</p>
                  </div>
                : <Notice>This Forge version has no reviewed host-execution adapter yet. The launcher version selector is generic, but installation for this particular version remains disabled.</Notice>}
            </div>}
            <Notice>Instance READY: NO. Existing Vanilla worlds are unchanged.</Notice>
            {packStaging.blocked.map((message, index) => <Notice key={index}>{message}</Notice>)}
            {packStaging.failures.map((message, index) => <Notice key={'failed-'+index}>{message}</Notice>)}
            {packResolution?.files.filter(file => file.availability !== 'available').map(file => <div className="item" key={file.project_id+'-'+file.file_id}>
              <span>{file.project_id}/{file.file_id}: {file.reason}</span>
              <button disabled={busy || !file.sha1 || !file.expected_size || packManualImports.some(m => m.project_id===file.project_id && m.file_id===file.file_id)} onClick={() => importManualMod(file.project_id, file.file_id)}>Select and verify local JAR</button>
            </div>)}
          </div>}
        </div>}{!caps.features.pack_inspection && <Unavailable feature="pack_inspection"/>}{!caps.features.modpacks && <Notice>Full modpack installation and Play still require trusted Forge execution, instance audit and atomic publication.</Notice>}</div>}
        {page === 'skins' && <div className="panel">
          <h3>Skin studio</h3>
          {!caps.features.skins ? <Unavailable feature="skins"/> : <>
            <p className="muted">PNG 64×64 / 64×32 · Classic / Slim · local-only. In-game and LAN visibility require separate verification.</p>
            <div className="form-row">
              <select aria-label="Skin profile" value={selectedProfile} onChange={e => setSelectedProfile(e.target.value)}><option value="">Select profile</option>{profiles.map(p => <option key={p.id} value={p.id}>{p.username}</option>)}</select>
              <input aria-label="Skin name" maxLength={80} value={skinName} onChange={e=>setSkinName(e.target.value)}/>
              <select aria-label="Skin model" value={skinModel} onChange={e=>setSkinModel(e.target.value as SkinModel)}><option value="classic">Classic</option><option value="slim">Slim</option></select>
              <button disabled={busy} onClick={()=>void run('Refresh skins',async()=>setSkins(await operations.listSkins(caps)))}>Refresh skins</button>
              <button disabled={busy || !skinName.trim()} onClick={()=>void run('Import skin',async()=>{
                const path=await open({multiple:false,filters:[{name:'Minecraft skin',extensions:['png']}]});
                if(typeof path!=='string') return;
                const skin=await operations.importSkin(caps,path,skinName.trim(),skinModel);
                setSkins(await operations.listSkins(caps));setSkinId(skin.id);
                const bytes=await operations.previewSkin(caps,skin.id);
                setSkinPreview('data:image/png;base64,'+btoa(String.fromCharCode(...bytes)));
              })}>Import PNG</button>
            </div>
            <div className="form-row">
              <select aria-label="Skin library" value={skinId} disabled={busy} onChange={e=>{
                const id=e.target.value;setSkinId(id);setSkinPreview('');
                const record=skins.find(s=>s.id===id);if(record)setSkinModel(record.model);
                if(id)void run('Preview skin',async()=>{const bytes=await operations.previewSkin(caps,id);setSkinPreview('data:image/png;base64,'+btoa(String.fromCharCode(...bytes)));});
              }}><option value="">Choose skin</option>{skins.map(s=><option key={s.id} value={s.id}>{s.name} · {s.model}</option>)}</select>
              <button disabled={busy || !selectedProfile || !skinId} onClick={()=>void run('Select profile skin',async()=>{
                await operations.selectSkin(caps,selectedProfile,skinId,skinModel);await refresh(caps);
                setSkinMessage('Profile skin saved — PreviewOnly until provider is prepared at Play.');
              })}>Use for profile</button>
              <select aria-label="Skin instance override" value={selectedInstance} onChange={e=>setSelectedInstance(e.target.value)}><option value="">Choose instance</option>{instances.filter(i=>i.profile_id===selectedProfile).map(i=><option key={i.id} value={i.id}>{i.display_name}</option>)}</select>
              <button disabled={busy || !selectedProfile || !selectedInstance || !skinId} onClick={()=>void run('Select instance skin',async()=>{
                await operations.selectSkin(caps,selectedProfile,skinId,skinModel,selectedInstance);
                setSkinMessage('Instance override saved — in-game NOT_VERIFIED.');
              })}>Override instance</button>
            </div>
            {skinPreview && <img alt="Minecraft skin texture atlas" src={skinPreview} style={{width:192,imageRendering:'pixelated'}}/>}
            <div role="status" className="item">{skinMessage}</div>
            <p>Provider: {playReady?.skin_provider_status ?? 'PreviewOnly / select a verified instance'}</p>
            <p>Local rendering: NOT_VERIFIED · LAN peer visibility: NOT_VERIFIED.</p>
          </>}
        </div>}
        {page === 'lan' && <div className="panel"><h3>Trusted LAN multiplayer</h3><p className="muted">Host / Join will require modpack and loader compatibility checks, stable offline identity and trusted LAN binding.</p><Notice>Never expose an offline-mode dedicated server to the public Internet. Verify world and inventory persistence on two physical machines.</Notice><div className="form-row"><button disabled={!caps.features.lan || busy} onClick={() => void run('LAN status', async () => { const value = await operations.lanStatus(caps); log(JSON.stringify(value)); })}>Check LAN status</button><button disabled>Host server · awaiting contract</button><button disabled>Join server · awaiting contract</button></div>{!caps.features.lan && <Unavailable feature="lan"/>}</div>}
        {page === 'downloads' && <div className="panel"><h3>Operation progress</h3><p className="muted">Live backend phases and polled SQLite state. No artificial percentages: the downloader has no verified byte-count callback yet.</p>{Object.values(gameStatusById).map(status => <div className="item" key={status.instance_id}><strong>{status.install_state}</strong><span>{status.instance_id}</span><span>{isInstallPending(status) ? 'Worker active / pending' : status.process_state}</span>{status.last_error && <span role="alert">{status.last_error}</span>}</div>)}{events.length ? visibleEvents.map((event, index) => <div className="event" key={event.operation_id + event.timestamp + index}><div className="event-title"><strong>{event.phase}</strong><span>{event.message_key}</span><small>{event.timestamp}</small>{typeof event.details.downloaded_files === 'number' && <small>Verified downloads: {event.details.downloaded_files}</small>}{typeof event.details.verified_files === 'number' && <small>SHA-1 verified files: {event.details.verified_files}</small>}{typeof event.details.native_dll_count === 'number' && <small>Extracted DLLs: {event.details.native_dll_count}</small>}</div>{safePercent(event.percent) !== undefined && <progress max={100} value={safePercent(event.percent)} aria-label={event.message_key}/>}<button disabled={!caps.features.game || busy || !event.instance_id || !canCancelInstall(gameStatusById[event.instance_id])}
          onClick={() => void run('Cancel actual Vanilla install', () => operations.cancelInstall(caps, event.instance_id!))}>Cancel Vanilla install</button></div>) : <Empty title="No events received" description="Download and install progress will appear here when the backend publishes events."/>}{events.length > 24 && <button onClick={() => setShowAllEvents(old => !old)}>{showAllEvents ? 'Show latest 24' : `Show all ${events.length} events`}</button>}{!caps.features.downloads && <Unavailable feature="downloads"/>}</div>}
        {page === 'settings' && <div className="panel">
          <h3>Java & Memory · per instance</h3>
          <p className="muted">Saved per instance. Default maximum heap is 8192 MB (8 GiB) on machines with sufficient physical RAM; JVM starts at Xms 512 MB and grows as needed. Free RAM is informative, not a launch blocker.</p>
          <div className="form-row">
            <select aria-label="Java settings instance" value={selectedInstance} onChange={e => setSelectedInstance(e.target.value)}>
              <option value="">Choose instance</option>{instances.map(i=><option key={i.id} value={i.id}>{i.display_name} · # {i.id.slice(0,8)} · {i.minecraft_version} · {i.loader.kind}</option>)}
            </select>
          </div>
          {memoryInfo && <div className="item"><HardDrive size={18}/> System RAM <span>Total {memoryInfo.total_mb} MB · Available {memoryInfo.available_mb} MB (not a limit)</span></div>}
          {javaSettings && <div>
            <div className="form-row">
              <label>Initial RAM (Xms MB)
                <input aria-label="Initial RAM MB" type="number" min={256} step={128} value={javaSettings.xms_mb}
                  onChange={e=>setJavaSettings({...javaSettings,xms_mb:Number(e.target.value)})}/>
              </label>
              <label>Maximum RAM (Xmx MB)
                <input aria-label="Maximum RAM MB" type="number" min={256} step={128} value={javaSettings.xmx_mb}
                  onChange={e=>setJavaSettings({...javaSettings,xmx_mb:Number(e.target.value)})}/>
              </label>
            </div>
            <div className="form-row">
              <input aria-label="Java executable" value={javaSettings.java_path} placeholder="System java.exe (or select an absolute Java path)"
                onChange={e=>setJavaSettings({...javaSettings,java_path:e.target.value})}/>
              <button onClick={()=>void run('Browse Java',async()=>{
                const path=await open({multiple:false,filters:[{name:'Java executable',extensions:['exe']}]});
                if(typeof path==='string')setJavaSettings(old=>old?{...old,java_path:path}:old);
              })}>Browse Java</button>
            </div>
            <p className="muted">Java major version is checked against official Mojang version metadata on Play. Select a real installed Java .exe or use system java.exe.</p>
            <button className="primary" disabled={!caps.features.settings || busy || Boolean(validateJavaForm(javaSettings,memoryInfo))}
              onClick={()=>void run('Save Java settings',async()=>{
                const saved=await operations.javaSettingsSave(caps,javaSettings);setJavaSettings(saved);
              })}>Save Java & Memory</button>
            {validateJavaForm(javaSettings,memoryInfo) && <Notice>{validateJavaForm(javaSettings,memoryInfo)}</Notice>}
            <div className="item">Current settings revision <span>{javaSettings.revision}</span></div>
          </div>}
          {!selectedInstance && <Notice>Create/select an instance first. Existing profiles are not modified.</Notice>}
          {!caps.features.settings && <Unavailable feature="settings"/>}
          <div className="item"><Activity size={18}/> Core provider <span>{caps.provider}</span></div>
          <div className="item"><ShieldAlert size={18}/> Update policy <span>Manual, no silent updates</span></div>
        </div>}
        {page === 'logs' && <div className="panel"><h3>Session diagnostics</h3><p className="muted">Local UI errors and received backend messages. This is not a substitute for Minecraft crash logs.</p><div>{Object.values(gameStatusById).map(status=><div className="item" key={status.instance_id}><strong>Rust / SQLite: {status.install_state}</strong><span>{status.instance_id}</span><span>{status.last_error || status.process_state}</span></div>)}</div><pre className="logbox" aria-label="Activity logs">{logs.length ? logs.join('\n') : 'No session activity yet.'}</pre><button onClick={() => setLogs([])}>Clear displayed session log</button></div>}
        <footer><CircleHelp size={14}/> UI scaffold · No game files or credentials included · GitHub Control Room owns verification</footer>
      </div>
    </main>
  </div>;
}
