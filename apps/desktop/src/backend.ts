import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export type Feature = 'profiles' | 'instances' | 'versions' | 'modpacks' | 'pack_inspection' | 'skins' | 'lan' | 'game' | 'settings' | 'downloads';
export type Capabilities = { schema_version: number; features: Record<Feature, boolean>; provider: string };
// Matched to tl-core serde serialization. UUID and DateTime<Utc> serialize as strings.
export type OfflineProfile = { id: string; username: string; skin_id: string | null; created_at: string; updated_at: string };
export type LoaderKind = 'vanilla' | 'fabric' | 'forge' | 'neo_forge';
export type InstanceState = 'empty' | 'inspecting' | 'planned' | 'downloading' | 'verifying' | 'ready' | 'needs_action' | 'failed' | 'cancelled';
export type GameInstance = { id: string; display_name: string; root_path: string; minecraft_version: string; profile_id: string; state: InstanceState; loader: { kind: LoaderKind; version: string | null } };
export type ProgressEvent = { contract_version: number; operation_id: string; instance_id: string | null; timestamp: string; phase: string; message_key: string; details: Record<string, unknown>; percent: number | null };
export type LaunchRequest = { instance_id: string; profile_id: string; java_path?: string | null; min_ram_mb?: number | null; max_ram_mb?: number | null; extra_jvm_args?: string[] | null };
export type LaunchProcess = { pid: number; started_at: string; log_path: string; termination_state: 'running' | 'exited' | 'crashed' };
export type PackMetadata = { name: string; source: 'curseforge_zip' | 'modrinth_mrpack' | 'prism_zip'; game_version: string; loader: { kind: string; version: string | null }; archive_sha256: string; archive_entries: number; uncompressed_bytes: number; warnings: string[]; files: unknown[]; overrides: string[] };
export const FEATURE_NAMES: Feature[] = ['profiles', 'instances', 'versions', 'modpacks', 'pack_inspection', 'skins', 'lan', 'game', 'settings', 'downloads'];
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
    if (result.schema_version !== 1 || typeof result.provider !== 'string' || !result.features || FEATURE_NAMES.some(f => typeof result.features[f] !== 'boolean')) return EMPTY_CAPABILITIES;
    return result;
  } catch { return EMPTY_CAPABILITIES; }
}
export async function call<T>(caps: Capabilities, feature: Feature, command: string, args?: Record<string, unknown>): Promise<T> {
  requireFeature(caps, feature);
  // Missing backend command MUST reject, not return a mock success.
  return invoke<T>(command, args);
}
export const operations = {
  listProfiles: (c: Capabilities) => call<OfflineProfile[]>(c, 'profiles', 'profile_list'),
  createProfile: (c: Capabilities, username: string) => call<OfflineProfile>(c, 'profiles', 'profile_create', { username }),
  listInstances: (c: Capabilities) => call<GameInstance[]>(c, 'instances', 'instance_list'),
  listVersions: (c: Capabilities) => call<string[]>(c, 'versions', 'version_list'),
  inspectPack: (c: Capabilities, archivePath: string) => call<PackMetadata>(c, 'pack_inspection', 'pack_inspect', { archivePath }),
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
        if (data && data.contract_version === 1 && typeof data.operation_id === 'string' && typeof data.phase === 'string' && typeof data.message_key === 'string') handler(data);
      }));
    }
  } catch (error) {
    cleanups.forEach(fn => fn());
    throw error;
  }
  return () => cleanups.forEach(fn => fn());
}
export function safeArchive(path: string): boolean { return /\.(zip|mrpack)$/i.test(path.trim()) && path.trim().length > 7; }
export function safePercent(value?: number | null): number | undefined { return typeof value === 'number' && Number.isFinite(value) ? Math.max(0, Math.min(100, value)) : undefined; }
