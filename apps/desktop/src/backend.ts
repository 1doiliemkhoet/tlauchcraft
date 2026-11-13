import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export type Feature = 'profiles' | 'instances' | 'versions' | 'modpacks' | 'skins' | 'lan' | 'game' | 'settings' | 'downloads';
export type Capabilities = { schema_version: 1; features: Record<Feature, boolean>; provider: string };
export type OfflineProfile = { id: string; username: string; skin_id?: string; created_at: string; updated_at: string };
export type InstanceState = 'Empty' | 'Inspecting' | 'Planned' | 'Downloading' | 'Verifying' | 'Ready' | 'NeedsAction' | 'Failed' | 'Cancelled';
export type GameInstance = { id: string; display_name: string; minecraft_version: string; profile_id: string; state: InstanceState; loader: { kind: 'Vanilla' | 'Fabric' | 'Forge' | 'NeoForge'; version?: string } };
export type ProgressEvent = { operation_id: string; instance_id?: string; timestamp: string; phase: string; message_key: string; details?: Record<string, unknown>; percent?: number };
export type LaunchRequest = { instance_id: string; profile_id: string; java_path?: string; min_ram_mb?: number; max_ram_mb?: number };
export type LaunchProcess = { pid: number; started_at: string; log_path: string; termination_state: string };
export const FEATURE_NAMES: Feature[] = ['profiles', 'instances', 'versions', 'modpacks', 'skins', 'lan', 'game', 'settings', 'downloads'];
export const EMPTY_CAPABILITIES: Capabilities = { schema_version: 1, provider: 'unavailable', features: Object.fromEntries(FEATURE_NAMES.map(f => [f, false])) as Record<Feature, boolean> };

export class BackendUnavailable extends Error {
  constructor(public feature: Feature, detail = 'Backend feature is not installed or negotiated') {
    super(`${feature}: ${detail}`);
    this.name = 'BackendUnavailable';
  }
}
export function requireFeature(caps: Capabilities, feature: Feature): void {
  if (caps.schema_version !== 1 || !caps.features[feature]) throw new BackendUnavailable(feature);
}
export async function negotiate(): Promise<Capabilities> {
  try {
    const result = await invoke<Capabilities>('desktop_capabilities');
    if (result.schema_version !== 1 || !result.features || FEATURE_NAMES.some(f => typeof result.features[f] !== 'boolean')) return EMPTY_CAPABILITIES;
    return result;
  } catch { return EMPTY_CAPABILITIES; }
}
export async function call<T>(caps: Capabilities, feature: Feature, command: string, args?: Record<string, unknown>): Promise<T> {
  requireFeature(caps, feature);
  // A missing backend command MUST reject, not return a mock success.
  return invoke<T>(command, args);
}
export const operations = {
  listProfiles: (c: Capabilities) => call<OfflineProfile[]>(c, 'profiles', 'profile_list'),
  createProfile: (c: Capabilities, username: string) => call<OfflineProfile>(c, 'profiles', 'profile_create', { username }),
  listInstances: (c: Capabilities) => call<GameInstance[]>(c, 'instances', 'instance_list'),
  listVersions: (c: Capabilities) => call<string[]>(c, 'versions', 'version_list'),
  importPack: (c: Capabilities, archivePath: string) => call<unknown>(c, 'modpacks', 'pack_import', { archivePath }),
  launch: (c: Capabilities, request: LaunchRequest) => call<LaunchProcess>(c, 'game', 'game_launch', { request }),
  cancel: (c: Capabilities, operationId: string) => call<void>(c, 'downloads', 'operation_cancel', { operationId }),
  listSkins: (c: Capabilities) => call<unknown[]>(c, 'skins', 'skin_list'),
  lanStatus: (c: Capabilities) => call<unknown>(c, 'lan', 'lan_status'),
};
export const EVENT_NAMES = ['profile.changed','instance.changed','download.progress','download.verified','pack.inspect','pack.needs_action','pack.ready','loader.installed','java.resolved','game.started','game.exited','lan.server_started','lan.peer_joined','skin.selected','skin.local_verified','skin.peer_verified'] as const;
export async function subscribeProgress(handler: (event: ProgressEvent) => void): Promise<() => void> {
  const cleanups: UnlistenFn[] = [];
  try {
    for (const name of EVENT_NAMES) {
      cleanups.push(await listen<ProgressEvent>(name, e => {
        const data = e.payload;
        if (data && typeof data.operation_id === 'string' && typeof data.phase === 'string') handler(data);
      }));
    }
  } catch (error) {
    cleanups.forEach(fn => fn());
    throw error;
  }
  return () => cleanups.forEach(fn => fn());
}
export function safeArchive(path: string): boolean { return /\.(zip|mrpack)$/i.test(path.trim()) && path.trim().length > 7; }
export function safePercent(value?: number): number | undefined { return Number.isFinite(value) ? Math.max(0, Math.min(100, value!)) : undefined; }
