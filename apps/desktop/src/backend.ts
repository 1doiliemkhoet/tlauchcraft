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
export type LaunchProcess = { pid: number; started_at: string; log_path: string; stderr_path: string; latest_log_path: string; termination_state: 'running' | 'exited' | 'crashed' };
export type JavaSettings = { instance_id: string; xms_mb: number; xmx_mb: number; java_path: string; revision: number };
export type MemoryInfo = { total_mb: number; available_mb: number; reserved_mb: number };
export type LaunchPreflight = { instance_id: string; java_path: string; required_java: number; xms_mb: number; xmx_mb: number };
export type GameStatus = { instance_id: string; install_state: InstanceState; installing: boolean; pid: number | null; process_state: string; last_error: string | null };
export type NewVanillaInstance = { name: string; version: string; profile_id: string };
export type PackMetadata = { name: string; source: 'curseforge_zip' | 'modrinth_mrpack' | 'prism_zip'; game_version: string; loader: { kind: string; version: string | null }; archive_sha256: string; archive_entries: number; uncompressed_bytes: number; warnings: string[]; files: unknown[]; overrides: string[] };
export type CurseForgeResolvedFile = {
  project_id: number; file_id: number; required: boolean; file_name: string | null;
  download_url: string | null; sha1: string | null; expected_size: number | null;
  availability: 'available' | 'needs_manual_download' | 'permission_denied' | 'not_found' | 'invalid_metadata' | 'network_failure';
  reason: string;
};
export type CurseForgeResolveReport = {
  files: CurseForgeResolvedFile[]; resolved_count: number; needs_action_count: number; ready: false;
};
export type CurseForgeStageReport = {
  status: 'needs_action' | 'failed' | 'cancelled';
  installed: { project_id: number; file_id: number; relative_path: string; actual_sha256: string; bytes: number }[];
  blocked: string[]; failures: string[]; staging_path: string; ready: false;
};
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
export type StagedPackReceipt = {
  source: string; minecraft_version: string; loader_kind: string; loader_version: string;
  archive_sha256: string; required_total: number; optional_total: number;
  verified: { project_id: number; file_id: number; required: boolean; relative_path: string; bytes: number; upstream_sha1: string; observed_sha256: string }[];
  blocked_required: string[]; blocked_optional: string[]; failures: string[];
  complete_mod_inventory: boolean; forge_runtime_verified: false; instance_ready: false;
};
export type InstalledPackMod = { project_id: number; file_id: number; relative_path: string; actual_sha256: string; bytes: number };
export const operations = {
  listProfiles: (c: Capabilities) => call<OfflineProfile[]>(c, 'profiles', 'profile_list'),
  createProfile: (c: Capabilities, username: string) => call<OfflineProfile>(c, 'profiles', 'profile_create', { username }),
  listInstances: (c: Capabilities) => call<GameInstance[]>(c, 'instances', 'instance_list'),
  createVanillaInstance: (c: Capabilities, req: NewVanillaInstance) => call<GameInstance>(c, 'game', 'instance_create_vanilla', { req }),
  installVanilla: (c: Capabilities, instanceId: string) => call<string>(c, 'game', 'game_install', { instanceId }),
  gameStatus: (c: Capabilities, instanceId: string) => call<GameStatus>(c, 'game', 'game_status', { instanceId }),
  gamePreflight: (c: Capabilities, instanceId: string, profileId: string) => call<LaunchPreflight>(c, 'game', 'game_preflight', { instanceId, profileId }),
  stopGame: (c: Capabilities, instanceId: string) => call<void>(c, 'game', 'game_stop', { instanceId }),
  cancelInstall: (c: Capabilities, instanceId: string) => call<void>(c, 'game', 'game_cancel', { instanceId }),
  javaSettingsGet: (c: Capabilities, instanceId: string) => call<JavaSettings>(c, 'settings', 'java_settings_get', { instanceId }),
  javaSettingsSave: (c: Capabilities, value: JavaSettings) => call<JavaSettings>(c, 'settings', 'java_settings_save', {
    instanceId: value.instance_id, xmsMb: value.xms_mb, xmxMb: value.xmx_mb, javaPath: value.java_path, expectedRevision: value.revision
  }),
  javaMemoryInfo: (c: Capabilities) => call<MemoryInfo>(c, 'settings', 'java_memory_info'),
  listVersions: (c: Capabilities) => call<string[]>(c, 'versions', 'version_list'),
  inspectPack: (c: Capabilities, archivePath: string) => call<PackMetadata>(c, 'pack_inspection', 'pack_inspect', { archivePath }),
  importPack: (c: Capabilities, archivePath: string) => call<unknown>(c, 'modpacks', 'pack_import', { archivePath }),
  resolveCurseForge: (c: Capabilities, archivePath: string) => call<CurseForgeResolveReport>(c, 'pack_inspection', 'pack_resolve', { archivePath }),
  stageCurseForge: (c: Capabilities, archivePath: string, operationId: string) => call<CurseForgeStageReport>(c, 'pack_inspection', 'pack_stage_curseforge', { archivePath, operationId }),
  stagedPackReceipt: (c: Capabilities, archivePath: string, operationId: string) => call<StagedPackReceipt>(c, 'pack_inspection', 'pack_staged_receipt', { archivePath, operationId }),
  importRestrictedPackMod: (c: Capabilities, archivePath: string, operationId: string, projectId: number, fileId: number, localPath: string) => call<InstalledPackMod>(c, 'pack_inspection', 'pack_import_restricted', { archivePath, operationId, projectId, fileId, localPath }),
  cancelPackStage: (c: Capabilities, operationId: string) => call<void>(c, 'pack_inspection', 'pack_cancel', { operationId }),
  launch: (c: Capabilities, request: LaunchRequest) => call<LaunchProcess>(c, 'game', 'game_launch', { request }),
  cancel: (c: Capabilities, operationId: string) => call<void>(c, 'downloads', 'operation_cancel', { operationId }),
  listSkins: (c: Capabilities) => call<unknown[]>(c, 'skins', 'skin_list'),
  lanStatus: (c: Capabilities) => call<unknown>(c, 'lan', 'lan_status'),
};
export const EVENT_NAMES = ['profile.changed','instance.changed','download.progress','download.verified','pack.inspect','pack.needs_action','pack.ready','loader.installed','java.resolved','game.started','game.exited','lan.server_started','lan.peer_joined','skin.selected','skin.local_verified','skin.peer_verified'] as const;
export async function subscribeProgress(handler: (event: ProgressEvent) => void): Promise<() => void> {
  // Subscribe concurrently: sequential registrations made the initial "resolving" event
  // easy to miss. All listeners are installed before the UI negotiates/starts work.
  const subscriptions = await Promise.allSettled(EVENT_NAMES.map(name =>
    listen<ProgressEvent>(name, e => {
      const data = e.payload;
      if (data && data.contract_version === 1 && typeof data.operation_id === 'string' &&
          typeof data.phase === 'string' && typeof data.message_key === 'string') handler(data);
    })
  ));
  const cleanups: UnlistenFn[] = subscriptions.flatMap(result =>
    result.status === 'fulfilled' ? [result.value] : []);
  const rejected = subscriptions.find(result => result.status === 'rejected');
  if (rejected && rejected.status === 'rejected') {
    cleanups.forEach(fn => fn());
    throw rejected.reason;
  }
  return () => cleanups.forEach(fn => fn());
}
export function safeArchive(path: string): boolean { return /\.(zip|mrpack)$/i.test(path.trim()) && path.trim().length > 7; }
export function safePercent(value?: number | null): number | undefined { return typeof value === 'number' && Number.isFinite(value) ? Math.max(0, Math.min(100, value)) : undefined; }
