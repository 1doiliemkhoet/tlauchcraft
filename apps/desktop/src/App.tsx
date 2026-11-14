import { useCallback, useEffect, useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { Activity, Box, CircleHelp, Download, FolderArchive, Gamepad2, HardDrive, LayoutDashboard, Monitor, Palette, Play, Settings2, ShieldAlert, Terminal, Users, Wifi, XCircle } from 'lucide-react';
import { backendError, installFailure, terminalInstallEvent, canPlayInstance, canCancelInstall, validateJavaForm } from './desktopState';
import { EMPTY_CAPABILITIES, negotiate, operations, safeArchive, safePercent, subscribeProgress, type Capabilities, type Feature, type GameInstance, type OfflineProfile, type ProgressEvent, type PackMetadata, type JavaSettings, type MemoryInfo, type GameStatus, type LaunchPreflight } from './backend';

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
  const [packPreview, setPackPreview] = useState<PackMetadata | null>(null);
  const [logs, setLogs] = useState<string[]>([]);
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);
  const [playReady, setPlayReady] = useState<LaunchPreflight | null>(null);
  const [playReadinessError, setPlayReadinessError] = useState<string | null>(null);
  const [newInstanceName, setNewInstanceName] = useState('Vanilla');
  const [newVersion, setNewVersion] = useState('1.21.1');
  const [javaSettings, setJavaSettings] = useState<JavaSettings | null>(null);
  const [memoryInfo, setMemoryInfo] = useState<MemoryInfo | null>(null);
  const [gameStatusById, setGameStatusById] = useState<Record<string, GameStatus>>({});
  const log = useCallback((line: string) => setLogs(old => [`${new Date().toLocaleTimeString()} · ${line}`, ...old].slice(0, 250)), []);
  const run = useCallback(async (label: string, fn: () => Promise<unknown>) => {
    setBusy(true); setError(undefined);
    try { await fn(); log(`${label}: request accepted (not E2E verified)`); }
    catch (e) { const message = backendError(e); setError(message); log(`${label} failed: ${message}`); }
    finally { setBusy(false); }
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
    let active = true; let unsubscribe: (() => void) | undefined;
    negotiate().then(c => { if (!active) return; setCaps(c); void refresh(c); log(`Backend provider: ${c.provider}`); });
    subscribeProgress(event => { if (active) { setEvents(old => [event, ...old].slice(0, 100));
        const failure = installFailure(event);
        if (failure) setError(failure);
        log(`${event.phase}: ${event.message_key}`); } })
      .then(fn => { if (active) unsubscribe = fn; else fn(); })
      .catch(() => log('Event channel unavailable outside Tauri or before backend installation.'));
    return () => { active = false; unsubscribe?.(); };
  }, [refresh, log]);
  const importFile = async (path?: string) => {
    if (!caps.features.pack_inspection) { setError('Archive inspector unavailable.'); return; }
    let input = path;
    if (!input) { const selected = await open({ multiple: false, filters: [{ name: 'Modpack archives', extensions: ['zip', 'mrpack'] }] }); if (typeof selected === 'string') input = selected; }
    if (!input) return;
    if (!safeArchive(input)) { setError('Chỉ nhận .zip hoặc .mrpack. Backend sẽ kiểm tra nội dung archive.'); return; }
    setPackPreview(null);
    await run('Inspect modpack', async () => { const metadata = await operations.inspectPack(caps, input!); setPackPreview(metadata); });
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
  useEffect(() => {
    let active = true;
    if (!caps.features.game || !instances.length) return;
    Promise.allSettled(instances.filter(i=>i.loader.kind==='vanilla').map(i =>
      operations.gameStatus(caps,i.id).then(status => ({id:i.id,status}))
    )).then(results => {
      if (!active) return;
      const updated: Record<string,GameStatus> = {};
      for (const item of results) if (item.status==='fulfilled') updated[item.value.id]=item.value.status;
      setGameStatusById(old=>({...old,...updated}));
      const errors=Object.entries(updated).filter(([,status])=>status.last_error);
      if (errors.length) setError(errors.map(([id,status])=>`${id}: ${status.last_error}`).join('; '));
      // Actual status might reconcile a crashed/interrupted install on restart.
      if (Object.values(updated).some(status=>status.install_state==='failed')) void refresh(caps);
    });
    return () => { active=false; };
  }, [caps,refresh,instances.length]);
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
  const current = navigation.find(n => n.id === page)!;
  const selected = instances.find(i => i.id === selectedInstance);
  return <div className="shell" onDragOver={e => { if (e.dataTransfer.types.includes('Files')) e.preventDefault(); }} onDrop={e => {
    // Browser File.path is intentionally not trusted; native OS dialog is the approved source for paths.
    e.preventDefault(); setPage('modpacks'); setError('Kéo thả đã nhận diện. Hãy chọn ZIP bằng hộp thoại hệ thống để lấy đường dẫn an toàn.');
  }}>
    <aside className="sidebar">
      <div className="brand"><div className="brand-icon"><Gamepad2 size={24}/></div><div><b>tlauchcraft</b><small>OFFLINE / LAN LAUNCHER</small></div></div>
      <div className="side-heading">WORKSPACE</div>
      <nav aria-label="Main navigation">{navigation.map(item => <button type="button" key={item.id} className={page === item.id ? 'nav active' : 'nav'} onClick={() => { setPage(item.id); setError(undefined); }}><item.icon size={18}/>{item.label}{featurePage[item.id as Page] && caps.features[featurePage[item.id as Page]!] && <span className="ready-dot"/>}</button>)}</nav>
      <div className="sidebar-foot"><div className="tiny-status"><span className={Object.values(caps.features).some(Boolean) ? 'status-dot green' : 'status-dot'}/> {caps.provider === 'unavailable' ? 'Core not connected' : caps.provider}</div><small>Build 0.1.0 · Development preview</small></div>
    </aside>
    <main className="main">
      <header className="topbar"><div className="breadcrumb">Launcher <span>/</span> {current.label}</div><div className="top-actions"><span className="chip"><Monitor size={13}/> Windows Desktop</span><span className="chip subtle">LOCAL PROFILE ONLY</span></div></header>
      <div className="content">
        <div className="heading"><div><div className="eyebrow">TLAUCHCRAFT CONTROL CENTER</div><h1>{current.label}</h1><p>{page === 'home' ? 'Your Minecraft worlds, modpacks and LAN sessions in one place.' : 'Status and actions are backed by the real Rust facade when available.'}</p></div><span className="version-tag">PREVIEW · NOT E2E VERIFIED</span></div>
        {error && <div role="alert" className="error"><XCircle size={18}/><span>{error}</span><button aria-label="Dismiss error" onClick={() => setError(undefined)}>×</button></div>}
        {page === 'home' && <>
          <div className="hero"><div className="hero-text"><div className="eyebrow">YOUR NEXT ADVENTURE</div><h2>Make room for<br/><em>better adventures.</em></h2><p>Build an isolated modded instance, choose an offline profile and launch only when the core is ready.</p><button className="primary" disabled={!caps.features.game || busy || !canPlayInstance(selected, selectedProfile, playReady, gameStatusById[selectedInstance])} onClick={() => void run('Launch', () => operations.launch(caps, { instance_id: selectedInstance, profile_id: selectedProfile }))}><Play size={16} fill="currentColor"/> Launch Minecraft</button></div><div className="hero-art"><div className="sun"/><div className="mountain m1"/><div className="mountain m2"/><div className="mountain m3"/><div className="grid-art"/></div></div>
          <div className="metric-grid"><div className="panel metric"><Box/><strong>{instances.length}</strong><span>Known instances</span></div><div className="panel metric"><Users/><strong>{profiles.length}</strong><span>Offline profiles</span></div><div className="panel metric"><Download/><strong>{events.length}</strong><span>Received events</span></div></div>
          <div className="panel"><h3>Launch readiness</h3><p className="muted">Choose a real profile and ready instance. Actions remain unavailable while their service is missing.</p><div className="form-row"><select aria-label="Selected profile" value={selectedProfile} onChange={e => setSelectedProfile(e.target.value)}><option value="">Select profile</option>{profiles.map(p => <option value={p.id} key={p.id}>{p.username}</option>)}</select><select aria-label="Selected instance" value={selectedInstance} onChange={e => setSelectedInstance(e.target.value)}><option value="">Select instance</option>{instances.filter(i => i.state === 'ready').map(i => <option value={i.id} key={i.id}>{i.display_name} · {i.minecraft_version}</option>)}</select></div>{playReady && <div className="item">Java {playReady.required_java} · Xms {playReady.xms_mb} MB · Xmx {playReady.xmx_mb} MB <span>Verified for launch</span></div>}{playReadinessError && <Notice>Play unavailable: {playReadinessError}</Notice>}{!caps.features.game && <Unavailable feature="game"/>}</div>
        </>}
        {page === 'profiles' && <div className="panel"><h3>Offline identities</h3><p className="muted">A persistent UUID is provided by the core, not derived from the editable nickname.</p><div className="form-row"><input aria-label="Minecraft username" placeholder="Minecraft username" maxLength={16} value={username} onChange={e => setUsername(e.target.value)}/><button className="primary" disabled={!caps.features.profiles || !/^[A-Za-z0-9_]{3,16}$/.test(username) || busy} onClick={() => void run('Create profile', async () => { await operations.createProfile(caps, username); setUsername(''); await refresh(caps); })}>Create profile</button></div>{profiles.map(p => <div className="item" key={p.id}><Users size={18}/><strong>{p.username}</strong><span className="mono">{p.id}</span></div>)}{!profiles.length && <Empty title="No local profiles loaded" description="Create one when the profile service is connected."/>}{!caps.features.profiles && <Unavailable feature="profiles"/>}</div>}
        {page === 'instances' && <div className="panel">
          <h3>Vanilla instances</h3>
          <p className="muted">Only official Vanilla versions are supported here. Forge and modpack install remain disabled pending Chat 4 backend.</p>
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
            <Box size={18}/><strong>{i.display_name}</strong><span>{i.minecraft_version} · {i.loader.kind}</span>
            <span className="chip">{i.state}</span>
            {gameStatusById[i.id]?.process_state === 'running' && <span>PID {gameStatusById[i.id]?.pid}</span>}
            {i.loader.kind === 'vanilla' && <>
              <button disabled={!caps.features.game || busy || !['empty','failed','cancelled','needs_action'].includes(i.state)} onClick={() => void run('Install Vanilla', async () => {
                await operations.installVanilla(caps, i.id); await refresh(caps);
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
                await operations.cancelInstall(caps,i.id); await refresh(caps);
              })}>Cancel install</button>
              <button disabled={!caps.features.game || busy || gameStatusById[i.id]?.process_state !== 'running'} onClick={() => void run('Stop Minecraft', async () => {
                await operations.stopGame(caps,i.id);
                const status=await operations.gameStatus(caps,i.id); setGameStatusById(old=>({...old,[i.id]:status}));
              })}>Stop</button>
              <button disabled={!caps.features.settings || busy} onClick={() => { setSelectedInstance(i.id);setPage('settings'); }}>Java & Memory</button>
            </>}
          </div>)}
          {!instances.length && <Empty title="No instances" description="Create a real Vanilla instance linked to your offline profile."/>}
          <Notice>No fake install progress: use Status to query SQLite state. Download events come only from actual Rust install workers.</Notice>
          {instances.filter(i=>gameStatusById[i.id]?.last_error).map(i=><Notice key={i.id}>
            {i.display_name}: {gameStatusById[i.id]?.last_error}
          </Notice>)}
          {!caps.features.game && <Unavailable feature="game"/>}
        </div>}
        {page === 'modpacks' && <div className="panel"><h3>Inspect a modpack archive</h3><p className="muted">Rust tl-pack validates the ZIP and returns metadata without extraction. Installation remains disabled until a verified importer exists.</p><div className="dropzone"><FolderArchive size={38}/><strong>Choose an exported modpack archive</strong><p>Only a native file picker passes a path to the backend. Drag-and-drop opens this screen without reading a browser-provided path.</p><button className="primary" disabled={!caps.features.pack_inspection || busy} onClick={() => void importFile()}>Inspect ZIP / MRPACK</button></div>{packPreview && <div className="panel"><h3>Read-only inspection result</h3><div className="item"><strong>{packPreview.name}</strong><span>{packPreview.game_version} · {packPreview.loader.kind}</span></div><div className="item">Archive hash (SHA-256) <span className="mono">{packPreview.archive_sha256}</span></div><div className="item">Entries <span>{packPreview.archive_entries}</span></div>{packPreview.warnings.map((warning, i) => <Notice key={i}>{warning}</Notice>)}<Notice>Inspected only: the modpack has NOT been installed, verified for play, or launched.</Notice></div>}{!caps.features.pack_inspection && <Unavailable feature="pack_inspection"/>}{!caps.features.modpacks && <Notice>Full modpack installation is unavailable until a real verified backend is integrated.</Notice>}</div>}
        {page === 'skins' && <div className="panel"><h3>Skin studio</h3><p className="muted">Classic / Slim · PNG library · pixel editor · 3D preview · cape compatibility</p><div className="placeholder-grid"><div className="skin-silhouette"><Palette size={44}/><span>3D render waiting for SkinService</span></div><div><h3>Visibility matters</h3><p>Launcher preview, your in-game appearance and skins visible to LAN peers are separate verification states.</p><div className="item">Launcher preview <span>Unverified</span></div><div className="item">Singleplayer provider <span>Unverified</span></div><div className="item">LAN peer visibility <span>Unverified</span></div></div></div><Unavailable feature="skins"/></div>}
        {page === 'lan' && <div className="panel"><h3>Trusted LAN multiplayer</h3><p className="muted">Host / Join will require modpack and loader compatibility checks, stable offline identity and trusted LAN binding.</p><Notice>Never expose an offline-mode dedicated server to the public Internet. Verify world and inventory persistence on two physical machines.</Notice><div className="form-row"><button disabled={!caps.features.lan || busy} onClick={() => void run('LAN status', async () => { const value = await operations.lanStatus(caps); log(JSON.stringify(value)); })}>Check LAN status</button><button disabled>Host server · awaiting contract</button><button disabled>Join server · awaiting contract</button></div>{!caps.features.lan && <Unavailable feature="lan"/>}</div>}
        {page === 'downloads' && <div className="panel"><h3>Operation progress</h3><p className="muted">Incoming typed events only; no artificial timers or fake percentages.</p>{events.length ? events.map((event, index) => <div className="event" key={event.operation_id + event.timestamp + index}><div className="event-title"><strong>{event.phase}</strong><span>{event.message_key}</span><small>{event.timestamp}</small></div>{safePercent(event.percent) !== undefined && <progress max={100} value={safePercent(event.percent)} aria-label={event.message_key}/>}<button disabled={!caps.features.game || busy || !event.instance_id || !['resolving','downloading','verifying'].includes(event.phase)}
          onClick={() => void run('Cancel actual Vanilla install', () => operations.cancelInstall(caps, event.instance_id!))}>Cancel Vanilla install</button></div>) : <Empty title="No events received" description="Download and install progress will appear here when the backend publishes events."/>}{!caps.features.downloads && <Unavailable feature="downloads"/>}</div>}
        {page === 'settings' && <div className="panel">
          <h3>Java & Memory · per instance</h3>
          <p className="muted">Saved locally in SQLite with revision protection. JVM receives these values as actual -Xms/-Xmx on launch.</p>
          <div className="form-row">
            <select aria-label="Java settings instance" value={selectedInstance} onChange={e => setSelectedInstance(e.target.value)}>
              <option value="">Choose instance</option>{instances.map(i=><option key={i.id} value={i.id}>{i.display_name} · {i.minecraft_version} · {i.loader.kind}</option>)}
            </select>
          </div>
          {memoryInfo && <div className="item"><HardDrive size={18}/> System RAM <span>Total {memoryInfo.total_mb} MB · Available {memoryInfo.available_mb} MB · Reserve {memoryInfo.reserved_mb} MB</span></div>}
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
          {!selectedInstance && <Notice>Create/select a Vanilla instance first. Existing profiles are not modified.</Notice>}
          {!caps.features.settings && <Unavailable feature="settings"/>}
          <div className="item"><Activity size={18}/> Core provider <span>{caps.provider}</span></div>
          <div className="item"><ShieldAlert size={18}/> Update policy <span>Manual, no silent updates</span></div>
        </div>}
        {page === 'logs' && <div className="panel"><h3>Session diagnostics</h3><p className="muted">Local UI errors and received backend messages. This is not a substitute for Minecraft crash logs.</p><pre className="logbox" aria-label="Activity logs">{logs.length ? logs.join('\n') : 'No session activity yet.'}</pre><button onClick={() => setLogs([])}>Clear displayed session log</button></div>}
        <footer><CircleHelp size={14}/> UI scaffold · No game files or credentials included · GitHub Control Room owns verification</footer>
      </div>
    </main>
  </div>;
}
