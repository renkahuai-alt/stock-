import { get } from 'svelte/store';
import type { BoardSummary, SaveBoardRequest, SaveCredentialsPayload } from '../types/contracts';
import { closeSettingsWindow, deleteBoard, getBoardBuildStatus, saveBoard, saveCredentials } from './commands';
import { formatErrorMessage } from './errors';
import { recordRuntimeSignal, serializeError } from './runtimeDiagnostics';
import { upsertBoardBuildStatus } from '../stores/boardBuildStore';
import {
  armDeleteBoard,
  clearBoardDraft,
  clearDeleteBoardIntent,
  removeSettingsCatalogBoard,
  settingsStore,
  setBoardDeleting,
  setBoardSaving,
  setCredentialsSaving,
  startCreateBoardDraft,
  syncSettingsBoard,
  upsertSettingsCatalogBoard,
} from '../stores/settingsStore';

export function selectBoardDraftFlow(boardId: string): void {
  const board = get(settingsStore).boardCatalog.find((item) => item.boardId === boardId);

  if (!board) {
    return;
  }

  const members = get(settingsStore).membersByBoard[board.boardId] ?? [];
  syncSettingsBoard(
    board.boardId,
    board.name,
    board.compositionAlgorithm,
    members.map((member) => member.symbol),
  );
}

export function startCreateBoardFlow(): void {
  startCreateBoardDraft();
}

export function requestDeleteBoardFlow(boardId: string): void {
  const draft = get(settingsStore);

  if (draft.pendingDeleteBoardId === boardId) {
    void deleteBoardFlow(boardId);
    return;
  }

  armDeleteBoard(boardId);
}

export async function saveCredentialsFlow(): Promise<void> {
  const draft = get(settingsStore);
  const payload: SaveCredentialsPayload = {
    appKey: draft.appKey.trim(),
    appSecret: draft.appSecret.trim(),
    accessToken: draft.accessToken.trim(),
  };

  setCredentialsSaving(true);
  recordRuntimeSignal('settings.credentials.save-requested');

  try {
    await saveCredentials(payload);
    recordRuntimeSignal('settings.credentials.save-succeeded');
    setCredentialsSaving(false, '鉴权已保存');
  } catch (error) {
    recordRuntimeSignal('settings.credentials.save-failed', {
      error: serializeError(error),
    });
    setCredentialsSaving(false, `鉴权保存失败：${formatErrorMessage(error, '请重试')}`);
  }
}

export async function saveBoardFlow(): Promise<void> {
  const draft = get(settingsStore);
  const members = parseMembers(draft.boardMembersInput);

  if (!draft.boardName.trim()) {
    setBoardSaving(false, '请输入板块名称');
    return;
  }

  if (members.length === 0) {
    setBoardSaving(false, '请至少输入一个股票代码');
    return;
  }

  const payload: SaveBoardRequest = {
    boardId: draft.boardEditorMode === 'edit' && draft.activeBoardId ? draft.activeBoardId : undefined,
    name: draft.boardName.trim(),
    members,
    compositionAlgorithm: draft.boardAlgorithm,
  };

  setBoardSaving(true);
  recordRuntimeSignal('settings.board.save-requested', {
    mode: draft.boardEditorMode,
    memberCount: members.length,
  });

  try {
    const response = await saveBoard(payload);
    const boardStatus = await getBoardBuildStatus(response.boardId).catch(() => null);

    if (boardStatus) {
      upsertBoardBuildStatus(boardStatus);
      upsertSettingsCatalogBoard(
        toBoardSummary(
          boardStatus,
          response.compositionAlgorithm,
        ),
        members.map((symbol) => ({ symbol })),
      );
    }

    syncSettingsBoard(
      response.boardId,
      boardStatus?.name ?? payload.name,
      response.compositionAlgorithm,
      members,
    );
    recordRuntimeSignal('settings.board.save-succeeded', {
      boardId: response.boardId,
      backgroundSyncStarted: response.backgroundSyncStarted,
      rebuildStarted: response.rebuildStarted,
    });
    setBoardSaving(false, response.backgroundSyncStarted ? '板块已创建，后台构建中' : '板块已保存');
  } catch (error) {
    recordRuntimeSignal('settings.board.save-failed', {
      error: serializeError(error),
    });
    setBoardSaving(false, `板块保存失败：${formatErrorMessage(error, '请重试')}`);
  }
}

export async function closeSettingsWindowFlow(): Promise<void> {
  recordRuntimeSignal('window.settings.close-requested');
  await closeSettingsWindow();
}

async function deleteBoardFlow(boardId: string): Promise<void> {
  const draft = get(settingsStore);
  const deletingBoard = draft.boardCatalog.find((board) => board.boardId === boardId);

  if (!deletingBoard) {
    clearDeleteBoardIntent();
    setBoardDeleting(false, '板块目录已更新，请重新确认');
    return;
  }

  setBoardDeleting(true);
  recordRuntimeSignal('settings.board.delete-requested', {
    boardId,
  });

  try {
    await deleteBoard(boardId);
    applyDeletedBoardLocally(boardId, draft.boardCatalog, draft.activeBoardId);
    clearDeleteBoardIntent();
    setBoardDeleting(false);
    recordRuntimeSignal('settings.board.delete-succeeded', {
      boardId,
    });
  } catch (error) {
    recordRuntimeSignal('settings.board.delete-failed', {
      boardId,
      error: serializeError(error),
    });
    setBoardDeleting(false, `板块删除失败：${formatErrorMessage(error, '请重试')}`);
  }
}

function parseMembers(input: string): string[] {
  return Array.from(
    new Set(
      input
        .split(/[\s,，]+/u)
        .map((value) => value.trim().toUpperCase())
        .filter(Boolean),
      ),
  );
}

function toBoardSummary(
  payload: Awaited<ReturnType<typeof getBoardBuildStatus>>,
  compositionAlgorithm: SaveBoardRequest['compositionAlgorithm'],
): BoardSummary {
  return {
    boardId: payload.boardId,
    name: payload.name,
    compositionAlgorithm,
    buildStatus: payload.buildStatus,
    buildPhase: payload.buildPhase,
    buildTotal: payload.buildTotal,
    buildCompleted: payload.buildCompleted,
    buildFailed: payload.buildFailed,
    buildJobId: payload.buildJobId,
    buildMessage: payload.buildMessage,
    updatedAt: payload.updatedAt,
  };
}

function applyDeletedBoardLocally(
  boardId: string,
  boardCatalog: BoardSummary[],
  activeBoardId: string,
): void {
  removeSettingsCatalogBoard(boardId);

  if (activeBoardId !== boardId) {
    return;
  }

  const replacementBoard = resolveReplacementBoard(boardCatalog, boardId);

  if (!replacementBoard) {
    clearBoardDraft();
    return;
  }

  const members = get(settingsStore).membersByBoard[replacementBoard.boardId] ?? [];
  syncSettingsBoard(
    replacementBoard.boardId,
    replacementBoard.name,
    replacementBoard.compositionAlgorithm,
    members.map((member) => member.symbol),
  );
}

function resolveReplacementBoard(boardCatalog: BoardSummary[], boardId: string): BoardSummary | null {
  const nextBoardCatalog = boardCatalog.filter((board) => board.boardId !== boardId);
  const currentIndex = boardCatalog.findIndex((board) => board.boardId === boardId);

  if (nextBoardCatalog.length === 0) {
    return null;
  }

  if (currentIndex === -1) {
    return nextBoardCatalog[0] ?? null;
  }

  return nextBoardCatalog[Math.min(currentIndex, nextBoardCatalog.length - 1)] ?? null;
}
