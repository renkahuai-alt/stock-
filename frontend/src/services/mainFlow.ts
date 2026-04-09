import { get } from 'svelte/store';
import type { BoardAlgorithm, BoardBuildStatusPayload, BoardSummary, SyncStatusPayload } from '../types/contracts';
import { bootstrap, getBoardBuildStatus, getBoardMemberSummaries, openSettingsWindow } from './commands';
import { scheduleChartWatchReconcile } from './chartWatchFlow';
import { formatErrorMessage } from './errors';
import { recordRuntimeSignal, serializeError, setRuntimeSnapshot } from './runtimeDiagnostics';
import {
  appStore,
  applyBoardMemberSummaries,
  applyBoardBuildStatus,
  applyBootstrapCatalogPayload,
  applyBootstrapPayload,
  getBoardMembersByBoardAndAlgorithm,
  setActiveTargetSummary,
} from '../stores/appStore';
import {
  boardBuildStore,
  isBoardBuildActive,
  resolveBoardBuild,
  shouldHydrateBoardBuild,
  syncBoardBuildState,
  upsertBoardBuildStatus,
} from '../stores/boardBuildStore';
import { loadChart, setChartBuildFailedState, setChartBuildingState } from '../stores/chartStore';
import { loadTargetNote, noteStore, persistNote, setTargetNote } from '../stores/noteStore';
import {
  reconcileSelection,
  resolveSelectionAfterCatalogRefresh,
  selectBoard,
  selectIndex,
  selectSymbol,
  selectionStore,
  type SelectionState,
  setBoardAlgorithm,
  setGranularity,
  syncSelectionToTarget,
  toGetChartRequest,
} from '../stores/selectionStore';
import { syncSettingsBoard } from '../stores/settingsStore';
import {
  refreshSyncStatus,
  setSyncStatus,
  setSyncStatusFailure,
  syncStore,
  triggerSync,
} from '../stores/syncStore';

export async function syncBootstrapState(): Promise<void> {
  recordRuntimeSignal('main-flow.bootstrap-sync-requested');
  const payload = await bootstrap();

  applyBootstrapPayload(payload);
  const nextAppState = get(appStore);
  syncBoardBuildState(payload.boards);
  syncSelectionToTarget(
    {
      targetType: payload.activeTargetNote.targetType,
      targetId: payload.activeTargetNote.targetId,
    },
    nextAppState,
  );
  syncActiveTargetSummary();
  setTargetNote(payload.activeTargetNote);
  setSyncStatus(payload.syncStatus);
  await syncRuntimeSyncStatus();
  syncBoardDraftFromSelection();
  await hydrateRecoveringBoardBuilds(payload.boards);
  await loadSelectionData();
  setRuntimeSnapshot('main-flow.bootstrap-sync', {
    boards: payload.boards.length,
    indexes: payload.indexes.length,
    activeTargetType: payload.activeTargetNote.targetType,
    activeTargetId: payload.activeTargetNote.targetId,
  });
}

export async function handleSyncStatusEvent(payload: SyncStatusPayload): Promise<void> {
  const previous = get(syncStore);
  setSyncStatus(payload);

  if (!shouldRefreshChartAfterSync(previous, payload)) {
    return;
  }

  await syncSelectionChartState();
}

export async function refreshBootstrapCatalogState(): Promise<void> {
  recordRuntimeSignal('main-flow.catalog-refresh-requested');

  try {
    const previousSelection = get(selectionStore);
    const previousAppState = get(appStore);
    const payload = await bootstrap();
    const nextSelection = resolveSelectionAfterCatalogRefresh({
      current: previousSelection,
      previousBoards: previousAppState.boards,
      nextCollections: {
        indexes: payload.indexes,
        boards: payload.boards,
        membersByBoard: payload.membersByBoard,
      },
    });

    applyBootstrapCatalogPayload(payload);
    syncBoardBuildState(payload.boards);
    selectionStore.set(nextSelection);
    syncActiveTargetSummary();
    setSyncStatus(payload.syncStatus);
    await syncRuntimeSyncStatus();
    if (didSelectionTargetChange(previousSelection, nextSelection)) {
      await loadSelectionData();
    } else {
      syncBoardDraftFromSelection();
      await scheduleChartWatchReconcile();
    }
    setRuntimeSnapshot('main-flow.catalog-refresh', {
      boards: payload.boards.length,
      membersByBoard: Object.keys(payload.membersByBoard).length,
      syncStatus: payload.syncStatus.status,
      targetType: nextSelection.targetType,
      targetId: nextSelection.targetId,
    });
    recordRuntimeSignal('main-flow.catalog-refresh-complete', {
      boards: payload.boards.length,
      targetType: nextSelection.targetType,
      targetId: nextSelection.targetId,
    });
  } catch (error) {
    recordRuntimeSignal('main-flow.catalog-refresh-failed', {
      error: serializeError(error),
    });
    throw error;
  }
}

export async function handleBoardBuildStatusEvent(payload: BoardBuildStatusPayload): Promise<void> {
  const currentAppState = get(appStore);
  const latestStatus = resolveBoardBuild(currentAppState.boards, get(boardBuildStore), payload.boardId);

  if (!latestStatus || !matchesBoardBuildPayload(latestStatus, payload)) {
    return;
  }

  const selection = get(selectionStore);

  if (selection.targetType !== 'board' || selection.targetId !== payload.boardId) {
    return;
  }

  await syncSelectionChartState(selection, latestStatus);
}

export async function selectIndexFlow(indexId: string): Promise<void> {
  selectIndex(indexId);
  syncActiveTargetSummary();
  await loadSelectionData();
}

export async function selectBoardFlow(boardId: string): Promise<void> {
  const board = get(appStore).boards.find((item) => item.boardId === boardId);

  if (board) {
    setBoardAlgorithm(board.compositionAlgorithm);
  }

  selectBoard(boardId);
  syncActiveTargetSummary();
  syncBoardDraftFromSelection(boardId);
  await loadSelectionData();
}

export async function selectSymbolFlow(symbol: string): Promise<void> {
  selectSymbol(symbol);
  syncActiveTargetSummary();
  await loadSelectionData();
}

export async function changeGranularityFlow(granularity: 'day' | 'week'): Promise<void> {
  setGranularity(granularity);
  await syncSelectionChartState();
}

export async function changeBoardAlgorithmFlow(boardAlgorithm: BoardAlgorithm): Promise<void> {
  setBoardAlgorithm(boardAlgorithm);
  syncBoardDraftFromSelection();

  const selection = get(selectionStore);

  if (selection.targetType === 'board') {
    const memberSummaries = await getBoardMemberSummaries({
      boardId: selection.targetId,
      compositionAlgorithm: boardAlgorithm,
    });

    applyBoardMemberSummaries(memberSummaries);
    await syncSelectionChartState(selection);
  }
}

export async function saveCurrentTargetNoteFlow(content: string): Promise<void> {
  const selection = get(selectionStore);
  const currentNote = get(noteStore);

  await persistNote({
    ...currentNote,
    targetType: selection.targetType,
    targetId: selection.targetId,
    content,
  });
}

export async function runManualSyncFlow(): Promise<void> {
  await triggerSync('manual');
}

export async function openSettingsWindowFlow(): Promise<void> {
  recordRuntimeSignal('window.settings.open-requested');
  await openSettingsWindow();
}

async function loadSelectionData(): Promise<void> {
  const selection = get(selectionStore);

  await Promise.all([
    syncSelectionChartState(selection),
    loadTargetNote({
      targetType: selection.targetType,
      targetId: selection.targetId,
    }),
  ]);

  syncBoardDraftFromSelection();
}

function syncBoardDraftFromSelection(explicitBoardId?: string): void {
  const currentAppState = get(appStore);
  const currentSelection = get(selectionStore);
  const targetBoardId =
    explicitBoardId
    ?? (currentSelection.targetType === 'board' ? currentSelection.targetId : currentSelection.activeBoardId);
  const board = currentAppState.boards.find((item) => item.boardId === targetBoardId);

  if (!board) {
    return;
  }

  syncBoardDraft(
    board,
    getBoardMembersByBoardAndAlgorithm(
      currentAppState.membersByBoard,
      board.boardId,
      board.compositionAlgorithm,
    ),
  );
}

function syncBoardDraft(board: BoardSummary, members: { symbol: string }[]): void {
  syncSettingsBoard(
    board.boardId,
    board.name,
    board.compositionAlgorithm,
    members.map((member) => member.symbol),
  );
}

function syncActiveTargetSummary(): void {
  const currentAppState = get(appStore);
  const currentSelection = get(selectionStore);

  if (currentSelection.targetType === 'index') {
    setActiveTargetSummary({
      targetType: 'index',
      targetId: currentSelection.targetId,
      title: currentAppState.indexes.find((item) => item.id === currentSelection.targetId)?.label ?? currentSelection.targetId,
    });
    return;
  }

  if (currentSelection.targetType === 'board') {
    setActiveTargetSummary({
      targetType: 'board',
      targetId: currentSelection.targetId,
      title: currentAppState.boards.find((item) => item.boardId === currentSelection.targetId)?.name ?? currentSelection.targetId,
    });
    return;
  }

  setActiveTargetSummary({
    targetType: 'symbol',
    targetId: currentSelection.targetId,
    title: currentSelection.targetId,
  });
}

async function syncSelectionChartState(
  selection = get(selectionStore),
  explicitBoardBuild?: BoardBuildStatusPayload,
): Promise<void> {
  const request = toGetChartRequest(selection);

  try {
    if (selection.targetType !== 'board') {
      await loadChart(request);
      return;
    }

    const currentAppState = get(appStore);
    const board =
      currentAppState.boards.find((item) => item.boardId === selection.targetId);
    const boardBuild =
      explicitBoardBuild
      ?? resolveBoardBuild(currentAppState.boards, get(boardBuildStore), selection.targetId);
    const boardTitle = board?.name ?? boardBuild?.name ?? selection.targetId;

    if (!boardBuild || boardBuild.buildStatus === 'succeeded') {
      await loadChart(request);
      return;
    }

    if (boardBuild.buildStatus === 'failed') {
      setChartBuildFailedState(request, boardTitle, boardBuild);
      return;
    }

    if (isBoardBuildActive(boardBuild)) {
      setChartBuildingState(request, boardTitle, boardBuild);
      return;
    }

    await loadChart(request);
  } finally {
    await scheduleChartWatchReconcile();
  }
}

async function hydrateRecoveringBoardBuilds(boards: BoardSummary[]): Promise<void> {
  const recoveringBoards = boards.filter(shouldHydrateBoardBuild);

  await Promise.all(
    recoveringBoards.map(async (board) => {
      try {
        const status = await getBoardBuildStatus(board.boardId);
        applyBoardBuildStatus(status);
        upsertBoardBuildStatus(status);
      } catch {
        return undefined;
      }

      return undefined;
    }),
  );
}

async function syncRuntimeSyncStatus(): Promise<void> {
  try {
    await refreshSyncStatus();
  } catch (error) {
    setSyncStatusFailure(`同步状态刷新失败：${formatErrorMessage(error, '未知错误')}`);
  }
}

function didSelectionTargetChange(
  previousSelection: SelectionState,
  nextSelection: SelectionState,
): boolean {
  return (
    previousSelection.targetType !== nextSelection.targetType
    || previousSelection.targetId !== nextSelection.targetId
    || (nextSelection.targetType === 'board' && previousSelection.boardAlgorithm !== nextSelection.boardAlgorithm)
  );
}

function matchesBoardBuildPayload(
  current: BoardBuildStatusPayload,
  incoming: BoardBuildStatusPayload,
): boolean {
  return (
    current.boardId === incoming.boardId
    && current.buildStatus === incoming.buildStatus
    && current.buildPhase === incoming.buildPhase
    && current.updatedAt === incoming.updatedAt
    && (current.buildJobId ?? '') === (incoming.buildJobId ?? '')
  );
}

function shouldRefreshChartAfterSync(previous: SyncStatusPayload, next: SyncStatusPayload): boolean {
  if (next.status === 'first_sync_running' || next.status === 'incremental_sync_running') {
    return false;
  }

  if (previous.lastSyncAt !== next.lastSyncAt) {
    return true;
  }

  if (previous.latestTradeDate !== next.latestTradeDate) {
    return true;
  }

  return previous.status !== next.status && next.status === 'ready';
}
