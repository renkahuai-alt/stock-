import { invoke } from '@tauri-apps/api/core';
import type {
  BoardBuildStatusPayload,
  BoardMemberSummariesPayload,
  BootstrapPayload,
  ChartWatchStatusPayload,
  GetBoardMemberSummariesRequest,
  GetChartRequest,
  GetTargetNoteRequest,
  RunSyncMode,
  SaveBoardRequest,
  SaveBoardResponse,
  SaveCredentialsPayload,
  StartChartWatchRequest,
  StopChartWatchPayload,
  SyncStatusPayload,
  TargetNotePayload,
} from '../types/contracts';
import {
  closeSettingsWindowFallback,
  openSettingsWindowFallback,
} from './browserWindows';
import { createCommandError } from './errors';
import {
  getBoardBuildStatusFallback,
  getBoardMemberSummariesFallback,
  getChartFallback,
  getSyncStatusFallback,
  getTargetNoteFallback,
  readBootstrapFallback,
  runSyncFallback,
  saveBoardFallback,
  saveCredentialsFallback,
  saveTargetNoteFallback,
} from './localDataSource';
import { recordRuntimeSignal, serializeError, setRuntimeSnapshot } from './runtimeDiagnostics';

const LOCAL_FALLBACK_ENABLED = import.meta.env.VITE_ENABLE_LOCAL_FALLBACK === 'true';
setRuntimeSnapshot('commands.mode', {
  localFallbackEnabled: LOCAL_FALLBACK_ENABLED,
});

async function invokeOrConfiguredFallback<T>(
  command: string,
  args: Record<string, unknown>,
  fallback: () => T | Promise<T>,
): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    if (LOCAL_FALLBACK_ENABLED) {
      recordRuntimeSignal('commands.fallback-used', {
        command,
        error: serializeError(error),
      });
      return fallback();
    }

    recordRuntimeSignal('commands.failed', {
      command,
      error: serializeError(error),
    });
    throw createCommandError(command, error);
  }
}

async function invokeRequiredCommand<T>(command: string, args: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    recordRuntimeSignal('commands.failed', {
      command,
      error: serializeError(error),
    });
    throw createCommandError(command, error);
  }
}

export function bootstrap(): Promise<BootstrapPayload> {
  return invokeOrConfiguredFallback('bootstrap', {}, () => readBootstrapFallback());
}

export function saveCredentials(payload: SaveCredentialsPayload): Promise<void> {
  return invokeOrConfiguredFallback('save_credentials', { payload }, () => saveCredentialsFallback(payload));
}

export function getSyncStatus(): Promise<SyncStatusPayload> {
  return invokeOrConfiguredFallback('get_sync_status', {}, () => getSyncStatusFallback());
}

export function runSync(mode: RunSyncMode): Promise<SyncStatusPayload> {
  return invokeOrConfiguredFallback('run_sync', { mode }, () => runSyncFallback(mode));
}

export function getChart(payload: GetChartRequest): Promise<import('../types/contracts').ChartPayload> {
  return invokeOrConfiguredFallback('get_chart', { payload }, () => getChartFallback(payload));
}

export function saveBoard(payload: SaveBoardRequest): Promise<SaveBoardResponse> {
  return invokeOrConfiguredFallback('save_board', { payload }, () => saveBoardFallback(payload));
}

export function deleteBoard(boardId: string): Promise<void> {
  return invokeRequiredCommand('delete_board', { boardId });
}

export function getBoardBuildStatus(boardId: string): Promise<BoardBuildStatusPayload> {
  return invokeOrConfiguredFallback('get_board_build_status', { boardId }, () => getBoardBuildStatusFallback(boardId));
}

export function getBoardMemberSummaries(
  payload: GetBoardMemberSummariesRequest,
): Promise<BoardMemberSummariesPayload> {
  return invokeOrConfiguredFallback(
    'get_board_member_summaries',
    { payload },
    () => getBoardMemberSummariesFallback(payload),
  );
}

export function getTargetNote(payload: GetTargetNoteRequest): Promise<TargetNotePayload> {
  return invokeOrConfiguredFallback('get_target_note', { payload }, () => getTargetNoteFallback(payload));
}

export function saveTargetNote(payload: TargetNotePayload): Promise<TargetNotePayload> {
  return invokeOrConfiguredFallback('save_target_note', { payload }, () => saveTargetNoteFallback(payload));
}

export function openSettingsWindow(): Promise<void> {
  return invokeOrConfiguredFallback('open_settings_window', {}, () => openSettingsWindowFallback());
}

export function closeSettingsWindow(): Promise<void> {
  return invokeOrConfiguredFallback('close_settings_window', {}, () => closeSettingsWindowFallback());
}

export function startChartWatch(payload: StartChartWatchRequest): Promise<ChartWatchStatusPayload> {
  return invokeRequiredCommand('start_chart_watch', { payload });
}

export function stopChartWatch(): Promise<StopChartWatchPayload> {
  return invokeRequiredCommand('stop_chart_watch', {});
}
