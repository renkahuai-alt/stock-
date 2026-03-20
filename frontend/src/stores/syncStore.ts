import { writable } from 'svelte/store';
import type { RunSyncMode, SyncStatusPayload } from '../types/contracts';
import { getSyncStatus, runSync } from '../services/commands';

export const syncStore = writable<SyncStatusPayload>({
  status: 'ready',
  message: '状态加载中',
  lastSyncAt: null,
  latestTradeDate: null,
});

export function setSyncStatus(payload: SyncStatusPayload): void {
  syncStore.set(payload);
}

export function setSyncStatusFailure(message: string): void {
  syncStore.update((current) => ({
    ...current,
    status: 'sync_failed',
    message,
  }));
}

export async function refreshSyncStatus(): Promise<void> {
  syncStore.set(await getSyncStatus());
}

export async function triggerSync(mode: RunSyncMode): Promise<void> {
  syncStore.set(await runSync(mode));
}
