import '../styles/tokens.css';
import '../styles/app.css';
import { get } from 'svelte/store';
import { mount } from 'svelte';
import { installEntryDiagnostics, markBootStage } from './bootstrapDiagnostics';
import SettingsWindow from '../windows/settings/SettingsWindow.svelte';
import { bootstrap } from '../services/commands';
import { registerCoreListeners } from '../services/events';
import { formatErrorMessage } from '../services/errors';
import { installRuntimeMetadata, recordRuntimeSignal, serializeError } from '../services/runtimeDiagnostics';
import {
  applySettingsCatalog,
  clearBoardDraft,
  setBoardSaving,
  setBoardDeleting,
  setCredentialsSaving,
  settingsStore,
  syncSettingsBoard,
} from '../stores/settingsStore';
import { registerWindowCleanup, resolveMountTarget } from './shared';

export function mountSettingsWindow(): void {
  installEntryDiagnostics('settings');
  installRuntimeMetadata({
    windowName: 'settings',
    localFallbackEnabled: import.meta.env.VITE_ENABLE_LOCAL_FALLBACK === 'true',
  });
  recordRuntimeSignal('window.settings.mount-requested');
  markBootStage('settings', 'mount-start');
  mount(SettingsWindow, {
    target: resolveMountTarget('Settings window'),
  });
  markBootStage('settings', 'mount-complete');
  recordRuntimeSignal('window.settings.mount-complete');

  void initializeSettingsWindow();
}

async function initializeSettingsWindow(): Promise<void> {
  markBootStage('settings', 'bootstrap-start');
  recordRuntimeSignal('window.settings.bootstrap-start');

  try {
    await hydrateSettingsCatalog({ preferBootstrapTarget: true });
    const unlisten = await registerCoreListeners({
      onSettingsSaved: () => {
        recordRuntimeSignal('window.settings.settings-saved-received');
        void hydrateSettingsCatalog().catch((error) => {
          recordRuntimeSignal('window.settings.catalog-refresh-failed', {
            error: serializeError(error),
          });
          const message = `设置目录刷新失败：${formatErrorMessage(error, '未知错误')}`;
          setCredentialsSaving(false, message);
          setBoardSaving(false, message);
          setBoardDeleting(false, message);
        });
      },
    });
    registerWindowCleanup(() => {
      unlisten.forEach((dispose) => dispose());
    });
    markBootStage('settings', 'bootstrap-complete');
    recordRuntimeSignal('window.settings.bootstrap-complete', {
      activeBoardId: get(settingsStore).activeBoardId || null,
    });
  } catch (error) {
    const message = `设置窗口初始化失败：${formatErrorMessage(error, '未知错误')}`;
    markBootStage('settings', 'bootstrap-failed', { message });
    recordRuntimeSignal('window.settings.bootstrap-failed', {
      message,
      error: serializeError(error),
    });
    setCredentialsSaving(false, message);
    setBoardSaving(false, message);
    setBoardDeleting(false, message);
  }
}

async function hydrateSettingsCatalog(options?: { preferBootstrapTarget?: boolean }): Promise<void> {
  const previousDraft = get(settingsStore);
  const payload = await bootstrap();
  applySettingsCatalog(payload.boards, payload.membersByBoard);

  const currentDraft = get(settingsStore);
  const currentActiveBoard = currentDraft.activeBoardId
    ? payload.boards.find((item) => item.boardId === currentDraft.activeBoardId)
    : undefined;

  if (!options?.preferBootstrapTarget && currentDraft.boardEditorMode === 'create') {
    if (payload.boards.length === 0) {
      clearBoardDraft();
    }
    return;
  }

  if (!options?.preferBootstrapTarget && currentActiveBoard) {
    return;
  }

  const preferredBoardId =
    options?.preferBootstrapTarget && payload.activeTargetNote.targetType === 'board'
      ? payload.activeTargetNote.targetId
      : currentDraft.activeBoardId;
  const board = resolvePreferredBoard(payload.boards, preferredBoardId, previousDraft.boardCatalog);

  if (!board) {
    clearBoardDraft();
    return;
  }

  syncSettingsBoard(
    board.boardId,
    board.name,
    board.compositionAlgorithm,
    (payload.membersByBoard[board.boardId] ?? []).map((member) => member.symbol),
  );
}

function resolvePreferredBoard(
  boards: Awaited<ReturnType<typeof bootstrap>>['boards'],
  preferredBoardId: string,
  previousBoards: Awaited<ReturnType<typeof bootstrap>>['boards'] = [],
) {
  const directMatch = boards.find((item) => item.boardId === preferredBoardId);

  if (directMatch) {
    return directMatch;
  }

  const previousIndex = previousBoards.findIndex((item) => item.boardId === preferredBoardId);

  if (previousIndex === -1) {
    return boards[0];
  }

  return boards[Math.min(previousIndex, boards.length - 1)] ?? boards[0];
}
