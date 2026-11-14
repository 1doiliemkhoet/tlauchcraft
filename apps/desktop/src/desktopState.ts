import type { GameInstance, GameStatus, JavaSettings, MemoryInfo, ProgressEvent, LaunchPreflight } from './backend';

// Deterministic UI state rules, independent of Tauri I/O.
export function backendError(error: unknown): string {
  if (typeof error === 'string' && error.trim()) return error;
  if (error instanceof Error && error.message.trim()) return error.message;
  return String(error);
}
export function installFailure(event: ProgressEvent): string | undefined {
  return event.phase === 'failed' ? event.message_key : undefined;
}
export function terminalInstallEvent(event: ProgressEvent): boolean {
  return ['ready', 'failed', 'cancelled'].includes(event.phase) && Boolean(event.instance_id);
}
export function canCancelInstall(status: GameStatus | undefined): boolean {
  return status?.installing === true &&
    ['planned','downloading','verifying'].includes(status.install_state);
}
export function canPlayInstance(instance: GameInstance | undefined, profileId: string,
  ready: LaunchPreflight | null, status?: GameStatus): boolean {
  return Boolean(instance?.state === 'ready' && instance.loader.kind === 'vanilla' &&
    instance.profile_id === profileId && ready?.instance_id === instance.id &&
    (!status || (!status.installing && status.process_state !== 'running' &&
      status.process_state !== 'crashed')));
}
export function validateJavaForm(prefs: JavaSettings | null, mem: MemoryInfo | null): string | undefined {
  if (!prefs || !mem) return 'Select an instance and load real Windows memory data.';
  const {xms_mb: xms, xmx_mb: xmx} = prefs;
  if (!Number.isInteger(xms) || !Number.isInteger(xmx) || xms < 256 || xmx < xms || xmx > 65536)
    return 'RAM must satisfy 256 MB <= Xms <= Xmx <= 65536 MB.';
  if (xmx > mem.total_mb - mem.reserved_mb)
    return 'Xmx exceeds physical RAM after the Windows system reserve.';
  if (xmx > mem.available_mb - 512)
    return 'Xmx exceeds currently available memory after 512 MB reserve.';
  return undefined;
}

/** Snapshot from Rust, never an optimistic "request accepted" completion. */
export function backendStatusLine(status: GameStatus): string {
  const message = status.last_error ? ` · error: ${status.last_error}` : '';
  const job = status.installing ? ' · worker running' : '';
  return `instance ${status.instance_id}: ${status.install_state}${job}${message}`;
}
export function backendStatusSignature(status: GameStatus): string {
  return [status.install_state, status.installing, status.process_state, status.last_error ?? ''].join('|');
}
export function isInstallPending(status: GameStatus): boolean {
  return status.installing || ['planned', 'downloading', 'verifying'].includes(status.install_state);
}
