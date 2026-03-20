import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type {
  BoardBuildStatusPayload,
  ChartLiveUpdatePayload,
  SyncStatusPayload,
} from '../types/contracts';
import { isTauriRuntimeAvailable, recordRuntimeSignal, serializeError } from './runtimeDiagnostics';

export interface EventHandlers {
  onSyncStatus?: (payload: SyncStatusPayload) => void;
  onBoardBuildStatus?: (payload: BoardBuildStatusPayload) => void;
  onChartLiveUpdate?: (payload: ChartLiveUpdatePayload) => void;
  onSettingsSaved?: () => void;
}

export async function registerCoreListeners(handlers: EventHandlers): Promise<UnlistenFn[]> {
  const unlisten: UnlistenFn[] = [];

  try {
    if (handlers.onSyncStatus) {
      unlisten.push(await listen<SyncStatusPayload>('sync-status', (event) => handlers.onSyncStatus?.(event.payload)));
    }

    if (handlers.onBoardBuildStatus) {
      unlisten.push(
        await listen<BoardBuildStatusPayload>('board-build-status', (event) =>
          handlers.onBoardBuildStatus?.(event.payload),
        ),
      );
    }

    if (handlers.onChartLiveUpdate) {
      unlisten.push(
        await listen<ChartLiveUpdatePayload>('chart-live-update', (event) =>
          handlers.onChartLiveUpdate?.(event.payload),
        ),
      );
    }

    if (handlers.onSettingsSaved) {
      unlisten.push(await listen('settings-saved', () => handlers.onSettingsSaved?.()));
    }
  } catch (error) {
    recordRuntimeSignal('events.registration-failed', {
      error: serializeError(error),
    });

    if (isTauriRuntimeAvailable()) {
      throw error;
    }

    return [];
  }

  recordRuntimeSignal('events.registration-complete', {
    listenerCount: unlisten.length,
  });
  return unlisten;
}
