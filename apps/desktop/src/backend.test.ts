import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { BackendUnavailable, EMPTY_CAPABILITIES, negotiate, operations, requireFeature, safeArchive, safePercent } from './backend';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
const mockedInvoke = vi.mocked(invoke);
const withFeature = (feature: keyof typeof EMPTY_CAPABILITIES.features) => ({
  ...EMPTY_CAPABILITIES,
  features: { ...EMPTY_CAPABILITIES.features, [feature]: true }
});
beforeEach(() => mockedInvoke.mockReset());

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
    await operations.launch(c,{instance_id:'uuid',profile_id:'profile'});
    expect(mockedInvoke).toHaveBeenNthCalledWith(1,'game_cancel',{instanceId:'uuid'});
    expect(mockedInvoke).toHaveBeenNthCalledWith(2,'game_status',{instanceId:'uuid'});
    expect(mockedInvoke).toHaveBeenNthCalledWith(3,'game_launch',{request:{instance_id:'uuid',profile_id:'profile'}});
  });
});
