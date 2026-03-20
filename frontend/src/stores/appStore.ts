import { writable } from 'svelte/store';
import type {
  BoardAlgorithm,
  BoardBuildStatusPayload,
  BoardMemberSummariesPayload,
  BoardSummary,
  BootstrapPayload,
  IndexItem,
  MemberSummary,
  TargetType,
} from '../types/contracts';
import { bumpRuntimeCounter, recordRuntimeSignal } from '../services/runtimeDiagnostics';
import { isIncomingBoardBuildNewer } from './boardBuildStore';

export interface ActiveTargetSummary {
  targetType: TargetType;
  targetId: string;
  title: string;
}

export interface AppState {
  indexes: IndexItem[];
  boards: BootstrapPayload['boards'];
  membersByBoard: Record<string, MemberSummary[]>;
  activeTargetSummary: ActiveTargetSummary;
}

const BOARD_MEMBERS_CACHE_KEY_SEPARATOR = '::';

const initialState: AppState = {
  indexes: [],
  boards: [],
  membersByBoard: {},
  activeTargetSummary: {
    targetType: 'index',
    targetId: '',
    title: '',
  },
};

export const appStore = writable<AppState>(initialState);

export function applyBootstrapPayload(payload: BootstrapPayload): void {
  recordRuntimeSignal('store.app.bootstrap-full-applied', {
    boards: payload.boards.length,
    indexes: payload.indexes.length,
  });
  appStore.set({
    indexes: payload.indexes,
    boards: payload.boards,
    membersByBoard: normalizeBoardMembersCache(payload.boards, payload.membersByBoard),
    activeTargetSummary: buildActiveTargetSummary(
      payload.activeTargetNote.targetType,
      payload.activeTargetNote.targetId,
      payload,
    ),
  });
}

export function applyBootstrapCatalogPayload(
  payload: Pick<BootstrapPayload, 'boards' | 'membersByBoard'>,
): void {
  bumpRuntimeCounter('store.app.catalog-patch', {
    boards: payload.boards.length,
  });
  appStore.update((current) => ({
    ...current,
    boards: payload.boards,
    membersByBoard: normalizeBoardMembersCache(payload.boards, payload.membersByBoard),
  }));
}

export function applyBoardBuildStatus(payload: BoardBuildStatusPayload): void {
  bumpRuntimeCounter('store.app.board-build-patch', {
    boardId: payload.boardId,
    buildStatus: payload.buildStatus,
    buildPhase: payload.buildPhase,
  });
  appStore.update((current) => ({
    ...current,
    boards: current.boards.map((board) =>
      board.boardId === payload.boardId && isIncomingBoardBuildNewer(board, payload)
        ? {
            ...board,
            name: payload.name,
            buildStatus: payload.buildStatus,
            buildPhase: payload.buildPhase,
            buildTotal: payload.buildTotal,
            buildCompleted: payload.buildCompleted,
            buildFailed: payload.buildFailed,
            buildJobId: payload.buildJobId,
            buildMessage: payload.buildMessage,
            updatedAt: payload.updatedAt,
          }
        : board,
    ),
  }));
}

export function upsertBoardSummary(board: BoardSummary, members: MemberSummary[]): void {
  appStore.update((current) => {
    const boardIndex = current.boards.findIndex((item) => item.boardId === board.boardId);
    const nextBoards = [...current.boards];

    if (boardIndex >= 0) {
      nextBoards.splice(boardIndex, 1, board);
    } else {
      nextBoards.unshift(board);
    }

    return {
      ...current,
      boards: nextBoards,
      membersByBoard: writeBoardMembersCacheEntry(current.membersByBoard, board.boardId, board.compositionAlgorithm, members),
    };
  });
}

export function applyBoardMemberSummaries(payload: BoardMemberSummariesPayload): void {
  appStore.update((current) => ({
    ...current,
    membersByBoard: {
      ...current.membersByBoard,
      [buildBoardMembersCacheKey(payload.boardId, payload.compositionAlgorithm)]: payload.members,
    },
  }));
}

export function setActiveTargetSummary(summary: ActiveTargetSummary): void {
  appStore.update((current) => ({
    ...current,
    activeTargetSummary: summary,
  }));
}

export function buildBoardMembersCacheKey(boardId: string, boardAlgorithm: BoardAlgorithm): string {
  return `${boardId}${BOARD_MEMBERS_CACHE_KEY_SEPARATOR}${boardAlgorithm}`;
}

export function getBoardMembersByBoardAndAlgorithm(
  membersByBoard: AppState['membersByBoard'],
  boardId: string,
  boardAlgorithm: BoardAlgorithm,
): MemberSummary[] {
  return membersByBoard[buildBoardMembersCacheKey(boardId, boardAlgorithm)] ?? [];
}

export function findBoardIdByMemberSymbol(
  membersByBoard: AppState['membersByBoard'],
  symbol: string,
): string | null {
  for (const [cacheKey, members] of Object.entries(membersByBoard)) {
    if (members.some((member) => member.symbol === symbol)) {
      return getBoardIdFromMembersCacheKey(cacheKey);
    }
  }

  return null;
}

function buildActiveTargetSummary(
  targetType: TargetType,
  targetId: string,
  payload: Pick<BootstrapPayload, 'indexes' | 'boards'>,
): ActiveTargetSummary {
  if (targetType === 'index') {
    return {
      targetType,
      targetId,
      title: payload.indexes.find((item) => item.id === targetId)?.label ?? targetId,
    };
  }

  if (targetType === 'board') {
    return {
      targetType,
      targetId,
      title: payload.boards.find((item) => item.boardId === targetId)?.name ?? targetId,
    };
  }

  return {
    targetType,
    targetId,
    title: targetId.toUpperCase(),
  };
}

function normalizeBoardMembersCache(
  boards: BoardSummary[],
  membersByBoard: BootstrapPayload['membersByBoard'],
): AppState['membersByBoard'] {
  return boards.reduce<AppState['membersByBoard']>((cache, board) => {
    cache[buildBoardMembersCacheKey(board.boardId, board.compositionAlgorithm)] = membersByBoard[board.boardId] ?? [];
    return cache;
  }, {});
}

function writeBoardMembersCacheEntry(
  membersByBoard: AppState['membersByBoard'],
  boardId: string,
  boardAlgorithm: BoardAlgorithm,
  members: MemberSummary[],
): AppState['membersByBoard'] {
  const nextMembersByBoard = Object.fromEntries(
    Object.entries(membersByBoard).filter(([cacheKey]) => getBoardIdFromMembersCacheKey(cacheKey) !== boardId),
  );

  nextMembersByBoard[buildBoardMembersCacheKey(boardId, boardAlgorithm)] = members;

  return nextMembersByBoard;
}

function getBoardIdFromMembersCacheKey(cacheKey: string): string {
  const separatorIndex = cacheKey.lastIndexOf(BOARD_MEMBERS_CACHE_KEY_SEPARATOR);

  return separatorIndex === -1 ? cacheKey : cacheKey.slice(0, separatorIndex);
}
