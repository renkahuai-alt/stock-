import { writable } from 'svelte/store';
import { findBoardIdByMemberSymbol } from './appStore';
import type { AppState } from './appStore';
import type {
  BoardAlgorithm,
  BoardSummary,
  GetChartRequest,
  GetTargetNoteRequest,
  Granularity,
  RangeKey,
  TargetType,
} from '../types/contracts';

type SelectionCollections = Pick<AppState, 'boards' | 'membersByBoard'>;
type SelectionCatalogCollections = Pick<AppState, 'indexes' | 'boards' | 'membersByBoard'>;

export interface SelectionCatalogRefreshInput {
  current: SelectionState;
  previousBoards: BoardSummary[];
  nextCollections: SelectionCatalogCollections;
}

export interface SelectionState {
  targetType: TargetType;
  targetId: string;
  activeIndexId: string;
  activeBoardId: string;
  activeSymbol: string;
  range: RangeKey;
  granularity: Granularity;
  boardAlgorithm: BoardAlgorithm;
}

const initialState: SelectionState = {
  targetType: 'index',
  targetId: '',
  activeIndexId: '',
  activeBoardId: '',
  activeSymbol: '',
  range: 'all',
  granularity: 'day',
  boardAlgorithm: 'equal_weight_v1',
};

export const selectionStore = writable<SelectionState>(initialState);

export function selectIndex(indexId: string): void {
  selectionStore.update((current) => ({
    ...current,
    targetType: 'index',
    targetId: indexId,
    activeIndexId: indexId,
    activeSymbol: '',
  }));
}

export function selectBoard(boardId: string): void {
  selectionStore.update((current) => ({
    ...current,
    targetType: 'board',
    targetId: boardId,
    activeBoardId: boardId,
    activeSymbol: '',
  }));
}

export function selectSymbol(symbol: string): void {
  selectionStore.update((current) => ({
    ...current,
    targetType: 'symbol',
    targetId: symbol,
    activeSymbol: symbol,
  }));
}

export function setGranularity(granularity: Granularity): void {
  selectionStore.update((current) => ({ ...current, granularity }));
}

export function setBoardAlgorithm(boardAlgorithm: BoardAlgorithm): void {
  selectionStore.update((current) => ({ ...current, boardAlgorithm }));
}

export function resolveSelectionAfterCatalogRefresh({
  current,
  previousBoards,
  nextCollections,
}: SelectionCatalogRefreshInput): SelectionState {
  if (current.targetType === 'index') {
    const nextTargetId = resolveFallbackIndexId(current.targetId, current.activeIndexId, nextCollections.indexes);
    const nextActiveBoardId = resolveRetainedActiveBoardId(current.activeBoardId, previousBoards, nextCollections.boards);

    return {
      ...current,
      targetType: 'index',
      targetId: nextTargetId,
      activeIndexId: nextTargetId,
      activeBoardId: nextActiveBoardId,
      activeSymbol: '',
      boardAlgorithm:
        nextActiveBoardId && nextActiveBoardId !== current.activeBoardId
          ? resolveBoardAlgorithm(nextActiveBoardId, nextCollections.boards)
          : nextActiveBoardId
            ? current.boardAlgorithm
            : 'equal_weight_v1',
    };
  }

  if (current.targetType === 'board' && hasBoard(current.targetId, nextCollections.boards)) {
    return {
      ...current,
      targetType: 'board',
      targetId: current.targetId,
      activeBoardId: current.targetId,
      activeSymbol: '',
    };
  }

  if (current.targetType === 'symbol' && hasBoard(current.activeBoardId, nextCollections.boards)) {
    return {
      ...current,
      targetType: 'symbol',
      targetId: current.targetId,
      activeBoardId: current.activeBoardId,
      activeSymbol: current.targetId,
    };
  }

  const fallbackBoard = resolveFallbackBoard(
    current.targetType === 'board' ? current.targetId : current.activeBoardId,
    previousBoards,
    nextCollections.boards,
  );

  if (fallbackBoard) {
    return {
      ...current,
      targetType: 'board',
      targetId: fallbackBoard.boardId,
      activeBoardId: fallbackBoard.boardId,
      activeSymbol: '',
      boardAlgorithm: fallbackBoard.compositionAlgorithm,
    };
  }

  const fallbackIndexId = resolveFallbackIndexId(current.activeIndexId, current.targetId, nextCollections.indexes);

  return {
    ...current,
    targetType: 'index',
    targetId: fallbackIndexId,
    activeIndexId: fallbackIndexId,
    activeBoardId: '',
    activeSymbol: '',
    boardAlgorithm: 'equal_weight_v1',
  };
}

export function reconcileSelection(appState: SelectionCollections): void {
  selectionStore.update((current) => {
    const activeBoardId = appState.boards.find((board) => board.boardId === current.activeBoardId)?.boardId
      ?? appState.boards[0]?.boardId
      ?? current.activeBoardId;

    if (current.targetType === 'board' && current.targetId !== activeBoardId) {
      return {
        ...current,
        targetId: activeBoardId,
        activeBoardId,
      };
    }

    return {
      ...current,
      activeBoardId,
    };
  });
}

export function syncSelectionToTarget(target: GetTargetNoteRequest, appState: SelectionCollections): void {
  selectionStore.update((current) => {
    if (target.targetType === 'index') {
      return {
        ...current,
        targetType: 'index',
        targetId: target.targetId,
        activeIndexId: target.targetId,
        activeSymbol: '',
      };
    }

    if (target.targetType === 'board') {
      const board = appState.boards.find((item) => item.boardId === target.targetId) ?? appState.boards[0];
      const boardId = board?.boardId ?? current.activeBoardId;

      return {
        ...current,
        targetType: 'board',
        targetId: boardId,
        activeBoardId: boardId,
        activeSymbol: '',
        boardAlgorithm: board?.compositionAlgorithm ?? current.boardAlgorithm,
      };
    }

    const ownerBoard = findBoardIdByMemberSymbol(appState.membersByBoard, target.targetId) ?? current.activeBoardId;

    return {
      ...current,
      targetType: 'symbol',
      targetId: target.targetId,
      activeBoardId: ownerBoard,
      activeSymbol: target.targetId,
    };
  });
}

export function toGetChartRequest(selection: SelectionState): GetChartRequest {
  return selection.targetType === 'board'
    ? {
        targetType: selection.targetType,
        targetId: selection.targetId,
        granularity: selection.granularity,
        range: 'all',
        boardAlgorithm: selection.boardAlgorithm,
      }
    : {
        targetType: selection.targetType,
        targetId: selection.targetId,
        granularity: selection.granularity,
        range: 'all',
      };
}

function hasBoard(boardId: string, boards: BoardSummary[]): boolean {
  return Boolean(boardId) && boards.some((board) => board.boardId === boardId);
}

function resolveFallbackBoard(
  preferredBoardId: string,
  previousBoards: BoardSummary[],
  nextBoards: BoardSummary[],
): BoardSummary | null {
  if (nextBoards.length === 0) {
    return null;
  }

  const previousIndex = previousBoards.findIndex((board) => board.boardId === preferredBoardId);

  if (previousIndex === -1) {
    return nextBoards[0] ?? null;
  }

  return nextBoards[Math.min(previousIndex, nextBoards.length - 1)] ?? null;
}

function resolveRetainedActiveBoardId(
  activeBoardId: string,
  previousBoards: BoardSummary[],
  nextBoards: BoardSummary[],
): string {
  if (hasBoard(activeBoardId, nextBoards)) {
    return activeBoardId;
  }

  return resolveFallbackBoard(activeBoardId, previousBoards, nextBoards)?.boardId ?? '';
}

function resolveFallbackIndexId(preferredIndexId: string, secondaryIndexId: string, indexes: AppState['indexes']): string {
  if (indexes.some((index) => index.id === preferredIndexId)) {
    return preferredIndexId;
  }

  if (indexes.some((index) => index.id === secondaryIndexId)) {
    return secondaryIndexId;
  }

  return indexes[0]?.id ?? 'DJI';
}

function resolveBoardAlgorithm(boardId: string, boards: BoardSummary[]): BoardAlgorithm {
  return boards.find((board) => board.boardId === boardId)?.compositionAlgorithm ?? 'equal_weight_v1';
}
