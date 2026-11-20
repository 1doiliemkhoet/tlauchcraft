import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { BackendUnavailable, EMPTY_CAPABILITIES, negotiate, operations, requireFeature, safeArchive, safePercent, subscribeProgress, EVENT_NAMES } from './backend';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }));
const mockedInvoke = vi.mocked(invoke);
const mockedListen = vi.mocked(listen);
const withFeature = (feature: keyof typeof EMPTY_CAPABILITIES.features) => ({
  ...EMPTY_CAPABILITIES,
  features: { ...EMPTY_CAPABILITIES.features, [feature]: true }
});
beforeEach(() => { mockedInvoke.mockReset(); mockedListen.mockReset(); });

describe('capability negotiation', () => {
  it('fails closed on malformed response', async () => {
    mockedInvoke.mockResolvedValue({ schema_version: 1, provider: 'incomplete', features: { profiles: true } });
    expect(await negotiate()).toEqual(EMPTY_CAPABILITIES);
  });
  it('accepts complete v1 feature map only', async () => {
    const available = withFeature('profiles');
    mockedInvoke.mockResolvedValue(available);
    expect(await negotiate()).toEqual(available);
    expect(mockedInvoke).toHaveBeenCalledWith('desktop_capabilities');
  });
});
describe('CurseForge API key preflight', () => {
  it('returns only a boolean; the frontend never receives the secret', async () => {
    mockedInvoke.mockResolvedValue(true);
    await expect(operations.curseForgeKeyConfigured(withFeature('pack_inspection'))).resolves.toBe(true);
    expect(mockedInvoke).toHaveBeenCalledWith('pack_api_key_configured', undefined);
  });
  it('refuses to query when the backend capability is unavailable', async () => {
    await expect(operations.curseForgeKeyConfigured(EMPTY_CAPABILITIES)).rejects.toThrow(BackendUnavailable);
    expect(mockedInvoke).not.toHaveBeenCalled();
  });
});
describe('folder-only installed CurseForge import', () => {
  it('accepts a local installed folder without ZIP or API key', async () => {
    mockedInvoke.mockResolvedValue({id: 'forged-instance', state: 'ready'});
    await operations.importInstalledFolder(
      withFeature('pack_inspection'),
      'C:/Users/admin/curseforge/minecraft/Instances/Rustic Waters II',
      'local-folder-123456', 'offline-id', 'C:/Java8/bin/java.exe', true
    );
    expect(mockedInvoke).toHaveBeenCalledWith('pack_import_installed_folder', {
      installedProfilePath: 'C:/Users/admin/curseforge/minecraft/Instances/Rustic Waters II',
      operationId: 'local-folder-123456',
      profileId: 'offline-id',
      javaPath: 'C:/Java8/bin/java.exe',
      hostExecutionConsent: true,
      lanProperties: false,
      customSkin: false
    });
  });
  it('passes explicit LAN Properties opt-in', async () => {
    mockedInvoke.mockResolvedValue({id: 'new-instance', state: 'ready'});
    await operations.importInstalledFolder(
      withFeature('pack_inspection'), 'C:/SkyFactory5', 'local-folder-987654',
      'offline-id', 'C:/Java17/bin/java.exe', true, true
    );
    expect(mockedInvoke).toHaveBeenCalledWith('pack_import_installed_folder', {
      installedProfilePath: 'C:/SkyFactory5',
      operationId: 'local-folder-987654',
      profileId: 'offline-id',
      javaPath: 'C:/Java17/bin/java.exe',
      hostExecutionConsent: true,
      lanProperties: true,
      customSkin: false
    });
  });
  it('passes independent Custom Skin and LAN consent', async () => {
    mockedInvoke.mockResolvedValue({id: 'skin-instance', state: 'ready'});
    await operations.importInstalledFolder(withFeature('pack_inspection'), 'C:/SkyFactory5', 'skin-folder-1234', 'profile', '', true, true, true);
    expect(mockedInvoke).toHaveBeenCalledWith('pack_import_installed_folder', {
      installedProfilePath: 'C:/SkyFactory5', operationId: 'skin-folder-1234', profileId: 'profile',
      javaPath: '', hostExecutionConsent: true, lanProperties: true, customSkin: true
    });
  });
  it('cannot run on a backend without modpack import support', async () => {
    await expect(operations.importInstalledFolder(
      EMPTY_CAPABILITIES, 'C:/CurseForge/Instances/Rustic', 'local-123456',
      'offline-id', 'C:/Java8/bin/java.exe', true
    )).rejects.toThrow(BackendUnavailable);
    expect(mockedInvoke).not.toHaveBeenCalled();
  });
});
describe('installed CurseForge profile import', () => {
  it('calls native no-API import with source folder and explicit consent', async () => {
    mockedInvoke.mockResolvedValue({id: 'new-forge-instance', state: 'ready'});
    await operations.importInstalledCurseForge(
      withFeature('pack_inspection'), 'C:/Rustic.zip',
      'C:/curseforge/minecraft/Instances/Rustic Waters II',
      'local-run-12345', 'offline-profile-id', 'C:/Java8/bin/javaw.exe', true
    );
    expect(mockedInvoke).toHaveBeenCalledWith('pack_import_installed_curseforge', {
      archivePath: 'C:/Rustic.zip',
      installedProfilePath: 'C:/curseforge/minecraft/Instances/Rustic Waters II',
      operationId: 'local-run-12345',
      profileId: 'offline-profile-id',
      javaPath: 'C:/Java8/bin/javaw.exe',
      hostExecutionConsent: true
    });
  });
});
describe('Windows Java 8 runtime discovery transport', () => {
  it('asks backend for a verified local Java 8 runtime, never a guessed path', async () => {
    const found = {path: 'C:/Program Files/Java/jre1.8.0_441/bin/java.exe', major: 8, source: 'Installed Java'};
    mockedInvoke.mockResolvedValue(found);
    await expect(operations.detectJava8(withFeature('settings'))).resolves.toEqual(found);
    expect(mockedInvoke).toHaveBeenCalledWith('java_detect_8', undefined);
  });
  it('returns null if no Java 8 was detected', async () => {
    mockedInvoke.mockResolvedValue(null);
    await expect(operations.detectJava8(withFeature('settings'))).resolves.toBeNull();
  });
  it('does not probe Java from an unavailable backend', async () => {
    await expect(operations.detectJava8(EMPTY_CAPABILITIES)).rejects.toThrow(BackendUnavailable);
    expect(mockedInvoke).not.toHaveBeenCalled();
  });
});

describe('typed backend transport', () => {
  it('rejects disabled operations without invoking Rust', async () => {
    expect(() => requireFeature(EMPTY_CAPABILITIES, 'game')).toThrow(BackendUnavailable);
    await expect(operations.listProfiles(EMPTY_CAPABILITIES)).rejects.toThrow(BackendUnavailable);
    expect(mockedInvoke).not.toHaveBeenCalled();
  });
  it('invokes real read-only pack inspector with file path', async () => {
    mockedInvoke.mockResolvedValue({ name: 'Example', files: [] });
    await expect(operations.inspectPack(withFeature('pack_inspection'), 'C:/pack.zip')).resolves.toMatchObject({ name: 'Example' });
    expect(mockedInvoke).toHaveBeenCalledWith('pack_inspect', { archivePath: 'C:/pack.zip' });
  });
  it('invokes profile command with exact payload', async () => {
    const profile = { id: 'id', username: 'ValidPlayer', skin_id: null, created_at: '', updated_at: '' };
    mockedInvoke.mockResolvedValue(profile);
    await expect(operations.createProfile(withFeature('profiles'), 'ValidPlayer')).resolves.toEqual(profile);
    expect(mockedInvoke).toHaveBeenCalledWith('profile_create', { username: 'ValidPlayer' });
  });
  it('rejects unsupported schema even with feature bit', () => {
    expect(() => requireFeature({ ...withFeature('game'), schema_version: 2 }, 'game')).toThrow(BackendUnavailable);
  });
});
describe('UX validation', () => {
  it('accepts supported extensions only (backend must inspect bytes)', () => {
    expect(safeArchive('pack.zip')).toBe(true);
    expect(safeArchive('pack.mrpack')).toBe(true);
    expect(safeArchive('PACK.ZIP')).toBe(true);
    expect(safeArchive('pack.exe')).toBe(false);
    expect(safeArchive('')).toBe(false);
  });
  it('clamps percentages and rejects non-finite numbers', () => {
    expect(safePercent(-50)).toBe(0);
    expect(safePercent(135)).toBe(100);
    expect(safePercent(NaN)).toBeUndefined();
    expect(safePercent(Infinity)).toBeUndefined();
    expect(safePercent(null)).toBeUndefined();
  });
  it('uses actual lowercase serde wire labels', () => {
    const state: import('./backend').InstanceState = 'ready';
    const loader: import('./backend').LoaderKind = 'neo_forge';
    expect([state,loader]).toEqual(['ready','neo_forge']);
  });
});

describe('real Vanilla and Java settings command contracts', () => {
  it('creates Vanilla instance only through a registered game command', async () => {
    mockedInvoke.mockResolvedValue({ id: 'instance-uuid', state: 'empty' });
    await operations.createVanillaInstance(withFeature('game'), {
      name: 'Vanilla', version: '1.21.1', profile_id: 'profile-uuid'
    });
    expect(mockedInvoke).toHaveBeenCalledWith('instance_create_vanilla', {
      req: { name: 'Vanilla', version: '1.21.1', profile_id: 'profile-uuid' }
    });
  });
  it('requires the exact selected UUID when removing an instance', async () => {
    const removed = { id: 'aaa-bbb', archived_path: 'C:/TLaunchCraft/deleted-instances/aaa-bbb' };
    mockedInvoke.mockResolvedValue(removed);
    await expect(operations.removeInstance(withFeature('instances'), 'aaa-bbb', 'aaa-bbb')).resolves.toEqual(removed);
    expect(mockedInvoke).toHaveBeenCalledWith('instance_remove', {
      instanceId: 'aaa-bbb', confirmInstanceId: 'aaa-bbb'
    });
  });
  it('sends persisted per-instance RAM and CAS revision without fake success', async () => {
    const value = {instance_id:'uuid',xms_mb:512,xmx_mb:4096,java_path:'C:/Java/bin/java.exe',revision:4};
    mockedInvoke.mockResolvedValue({...value,revision:5});
    expect(await operations.javaSettingsSave(withFeature('settings'), value)).toMatchObject({revision:5});
    expect(mockedInvoke).toHaveBeenCalledWith('java_settings_save', {
      instanceId:'uuid',xmsMb:512,xmxMb:4096,javaPath:'C:/Java/bin/java.exe',expectedRevision:4
    });
  });
  it('does not send installation or stop actions to a disabled backend', async () => {
    await expect(operations.installVanilla(EMPTY_CAPABILITIES,'uuid')).rejects.toThrow(BackendUnavailable);
    await expect(operations.stopGame(EMPTY_CAPABILITIES,'uuid')).rejects.toThrow(BackendUnavailable);
    expect(mockedInvoke).not.toHaveBeenCalled();
  });
  it('uses the real Rust command for cancellation, process status, and launch', async () => {
    mockedInvoke.mockResolvedValue(undefined);
    const c = withFeature('game');
    await operations.cancelInstall(c,'uuid');
    await operations.gameStatus(c,'uuid');
    await operations.gamePreflight(c,'uuid','profile');
    await operations.launch(c,{instance_id:'uuid',profile_id:'profile'});
    expect(mockedInvoke).toHaveBeenNthCalledWith(1,'game_cancel',{instanceId:'uuid'});
    expect(mockedInvoke).toHaveBeenNthCalledWith(2,'game_status',{instanceId:'uuid'});
    expect(mockedInvoke).toHaveBeenNthCalledWith(3,'game_preflight',{instanceId:'uuid',profileId:'profile'});
    expect(mockedInvoke).toHaveBeenNthCalledWith(4,'game_launch',{request:{instance_id:'uuid',profile_id:'profile'}});
  });
});

describe('Tauri event names are valid for Windows runtime', () => {
  it('uses only supported Tauri event characters and matches native emit names', () => {
    expect(EVENT_NAMES.every(name => /^[a-zA-Z0-9_:/-]+$/.test(name))).toBe(true);
    expect(new Set(EVENT_NAMES).size).toBe(EVENT_NAMES.length);
    expect(EVENT_NAMES).toContain('download-progress');
    expect(EVENT_NAMES).toContain('game-started');
    expect(EVENT_NAMES).toContain('game-exited');
  });
});

describe('event listener startup race and cleanup', () => {
  it('registers every event listener concurrently instead of 15 serial handshakes', async () => {
    const cleanups = Array.from({length: EVENT_NAMES.length}, () => vi.fn());
    const resolvers: Array<(value: () => void) => void> = [];
    mockedListen.mockImplementation(() => new Promise(resolve => { resolvers.push(resolve); }));
    const events: string[] = [];
    const waiting = subscribeProgress(e => events.push(e.phase));
    await Promise.resolve();
    expect(mockedListen).toHaveBeenCalledTimes(EVENT_NAMES.length);
    expect(resolvers).toHaveLength(EVENT_NAMES.length);
    resolvers.forEach((done,i) => done(cleanups[i]));
    const stop = await waiting;
    const eventCallback = mockedListen.mock.calls[0][1];
    eventCallback?.({payload:{
      contract_version:1,operation_id:'real-install',instance_id:'abc',
      phase:'downloading',message_key:'Verified Mojang files',timestamp:'',details:{downloaded_files:3970},percent:null
    }} as never);
    expect(events).toEqual(['downloading']);
    stop();
    expect(cleanups.every(fn => fn.mock.calls.length === 1)).toBe(true);
  });
  it('unsubscribes successful listeners when any registration fails', async () => {
    const cleanup = vi.fn();
    mockedListen.mockImplementation((name) => name === EVENT_NAMES[0]
      ? Promise.reject(new Error('listener unavailable'))
      : Promise.resolve(cleanup));
    await expect(subscribeProgress(() => {})).rejects.toThrow('listener unavailable');
    expect(cleanup).toHaveBeenCalledTimes(EVENT_NAMES.length - 1);
  });
});
