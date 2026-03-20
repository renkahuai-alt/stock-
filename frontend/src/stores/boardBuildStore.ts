import { writable } from 'svelte/store';
import type { BoardBuildStatusPayload, BoardSummary } from '../types/contracts';

export interface BoardBuildState {
  byBoardId: Record<string, BoardBuildStatusPayload>;
  lastHydratedAt: string | null;
}

export interface BoardListItemViewModel {
  boardId: string;
  name: string;
  buildStatusLabel: string;
  buildStatusVisible: boolean;
}

export const boardBuildStore = writable<BoardBuildState>({
  byBoardId: {},
  lastHydratedAt: null,
});

export function syncBoardBuildState(boards: BoardSummary[]): void {
  boardBuildStore.update((current) => {
    const activeBoardIds = new Set(boards.map((board) => board.boardId));
    const byBoardId = Object.fromEntries(
      Object.entries(current.byBoardId).filter(([boardId]) => activeBoardIds.has(boardId)),
    );

    for (const board of boards) {
      const payload = toBoardBuildPayload(board);

      byBoardId[board.boardId] = pickNewerBoardBuild(byBoardId[board.boardId], payload);
    }

    return {
      byBoardId,
      lastHydratedAt: new Date().toISOString(),
    };
  });
}

export function upsertBoardBuildStatus(payload: BoardBuildStatusPayload): void {
  boardBuildStore.update((current) => ({
    ...current,
    byBoardId: {
      ...current.byBoardId,
      [payload.boardId]: pickNewerBoardBuild(current.byBoardId[payload.boardId], payload),
    },
  }));
}

export function resolveBoardBuild(
  boards: BoardSummary[],
  state: BoardBuildState,
  boardId: string,
): BoardBuildStatusPayload | null {
  const board = boards.find((item) => item.boardId === boardId);
  const current = board ? toBoardBuildPayload(board) : undefined;
  const incoming = state.byBoardId[boardId];

  if (current && incoming) {
    return pickNewerBoardBuild(current, incoming);
  }

  return incoming ?? current ?? null;
}

export function buildBoardListItems(
  boards: BoardSummary[],
  state: BoardBuildState,
): BoardListItemViewModel[] {
  return boards.map((board) => {
    const status = resolveBoardBuild(boards, state, board.boardId);

    return {
      boardId: board.boardId,
      name: board.name,
      buildStatusLabel: formatBoardBuildStatus(status),
      buildStatusVisible: Boolean(status && status.buildStatus !== 'succeeded'),
    };
  });
}

export function summarizeBoardBuilds(
  boards: BoardSummary[],
  state: BoardBuildState,
): { buildingCount: number; failedCount: number } {
  return boards.reduce(
    (summary, board) => {
      const status = resolveBoardBuild(boards, state, board.boardId);

      if (status?.buildStatus === 'queued' || status?.buildStatus === 'running') {
        summary.buildingCount += 1;
      }

      if (status?.buildStatus === 'failed') {
        summary.failedCount += 1;
      }

      return summary;
    },
    { buildingCount: 0, failedCount: 0 },
  );
}

export function shouldHydrateBoardBuild(board: BoardSummary): boolean {
  return board.buildStatus === 'queued' || board.buildStatus === 'running';
}

export function isBoardBuildActive(status: BoardBuildStatusPayload | null): boolean {
  return status?.buildStatus === 'queued' || status?.buildStatus === 'running';
}

export function isIncomingBoardBuildNewer(
  current: BoardBuildStatusPayload | BoardSummary | undefined,
  incoming: BoardBuildStatusPayload | BoardSummary,
): boolean {
  if (!current) {
    return true;
  }

  const currentUpdatedAt = new Date(current.updatedAt).getTime();
  const incomingUpdatedAt = new Date(incoming.updatedAt).getTime();

  if (Number.isNaN(currentUpdatedAt) || Number.isNaN(incomingUpdatedAt)) {
    return true;
  }

  if (incomingUpdatedAt > currentUpdatedAt) {
    return true;
  }

  if (incomingUpdatedAt < currentUpdatedAt) {
    return false;
  }

  return current.buildJobId !== incoming.buildJobId;
}

export function formatBoardBuildStatus(status: BoardBuildStatusPayload | null): string {
  if (!status) {
    return '';
  }

  if (status.buildStatus === 'queued') {
    return '等待中';
  }

  if (status.buildStatus === 'running') {
    return buildPhaseLabel(status.buildPhase);
  }

  if (status.buildStatus === 'failed') {
    return '失败';
  }

  return '';
}

export function toBoardBuildPayload(board: BoardSummary): BoardBuildStatusPayload {
  return {
    boardId: board.boardId,
    name: board.name,
    buildStatus: board.buildStatus,
    buildPhase: board.buildPhase,
    buildTotal: board.buildTotal,
    buildCompleted: board.buildCompleted,
    buildFailed: board.buildFailed,
    buildJobId: board.buildJobId,
    buildMessage: board.buildMessage,
    updatedAt: board.updatedAt,
  };
}

function pickNewerBoardBuild(
  current: BoardBuildStatusPayload | undefined,
  incoming: BoardBuildStatusPayload,
): BoardBuildStatusPayload {
  if (!current) {
    return incoming;
  }

  if (isIncomingBoardBuildNewer(current, incoming)) {
    return incoming;
  }

  return current;
}

function buildPhaseLabel(phase: BoardBuildStatusPayload['buildPhase']): string {
  switch (phase) {
    case 'queued':
      return '等待中';
    case 'fetching_symbols':
      return '拉取成分股';
    case 'fetching_history':
      return '拉取历史';
    case 'recomputing_board':
      return '重算板块';
    case 'persisting':
      return '写入缓存';
    case 'failed':
      return '失败';
    default:
      return '处理中';
  }
}
