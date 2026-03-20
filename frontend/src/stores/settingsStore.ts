import { writable } from 'svelte/store';
import type { BoardAlgorithm, BoardSummary, MemberSummary, SettingsSection } from '../types/contracts';

export interface SettingsDraft {
  activeSection: SettingsSection;
  appKey: string;
  appSecret: string;
  accessToken: string;
  boardEditorMode: 'create' | 'edit';
  activeBoardId: string;
  boardName: string;
  boardMembersInput: string;
  boardAlgorithm: BoardAlgorithm;
  isSavingCredentials: boolean;
  credentialsFeedback: string;
  isSavingBoard: boolean;
  isDeletingBoard: boolean;
  pendingDeleteBoardId: string;
  boardFeedback: string;
  boardCatalog: BoardSummary[];
  membersByBoard: Record<string, MemberSummary[]>;
}

export const settingsStore = writable<SettingsDraft>({
  activeSection: 'credentials',
  appKey: '',
  appSecret: '',
  accessToken: '',
  boardEditorMode: 'create',
  activeBoardId: '',
  boardName: '',
  boardMembersInput: '',
  boardAlgorithm: 'equal_weight_v1',
  isSavingCredentials: false,
  credentialsFeedback: '',
  isSavingBoard: false,
  isDeletingBoard: false,
  pendingDeleteBoardId: '',
  boardFeedback: '',
  boardCatalog: [],
  membersByBoard: {},
});

export function setActiveSettingsSection(activeSection: SettingsSection): void {
  settingsStore.update((current) => ({ ...current, activeSection }));
}

export function syncSettingsBoard(
  boardId: string,
  boardName: string,
  boardAlgorithm: BoardAlgorithm,
  memberSymbols: string[] = [],
): void {
  settingsStore.update((current) => ({
    ...current,
    boardEditorMode: 'edit',
    activeBoardId: boardId,
    boardName,
    boardMembersInput: memberSymbols.join(', '),
    boardAlgorithm,
    isDeletingBoard: false,
    pendingDeleteBoardId: '',
    boardFeedback: '',
  }));
}

export function applySettingsCatalog(
  boardCatalog: BoardSummary[],
  membersByBoard: Record<string, MemberSummary[]>,
): void {
  settingsStore.update((current) => ({
    ...current,
    boardCatalog,
    membersByBoard,
    pendingDeleteBoardId:
      current.pendingDeleteBoardId && boardCatalog.some((board) => board.boardId === current.pendingDeleteBoardId)
        ? current.pendingDeleteBoardId
        : '',
  }));
}

export function upsertSettingsCatalogBoard(board: BoardSummary, members: MemberSummary[]): void {
  settingsStore.update((current) => {
    const boardIndex = current.boardCatalog.findIndex((item) => item.boardId === board.boardId);
    const nextBoardCatalog = [...current.boardCatalog];

    if (boardIndex >= 0) {
      nextBoardCatalog.splice(boardIndex, 1, board);
    } else {
      nextBoardCatalog.unshift(board);
    }

    return {
      ...current,
      boardCatalog: nextBoardCatalog,
      membersByBoard: {
        ...current.membersByBoard,
        [board.boardId]: members,
      },
      pendingDeleteBoardId: '',
    };
  });
}

export function startCreateBoardDraft(): void {
  settingsStore.update((current) => ({
    ...current,
    activeSection: 'boards',
    ...buildEmptyBoardDraft(),
  }));
}

export function clearBoardDraft(): void {
  settingsStore.update((current) => ({
    ...current,
    ...buildEmptyBoardDraft(),
  }));
}

export function removeSettingsCatalogBoard(boardId: string): void {
  settingsStore.update((current) => {
    const { [boardId]: _removedMembers, ...nextMembersByBoard } = current.membersByBoard;

    return {
      ...current,
      boardCatalog: current.boardCatalog.filter((board) => board.boardId !== boardId),
      membersByBoard: nextMembersByBoard,
      pendingDeleteBoardId: current.pendingDeleteBoardId === boardId ? '' : current.pendingDeleteBoardId,
    };
  });
}

export function armDeleteBoard(boardId: string): void {
  settingsStore.update((current) => ({
    ...current,
    pendingDeleteBoardId: boardId,
    boardFeedback: '',
  }));
}

export function clearDeleteBoardIntent(): void {
  settingsStore.update((current) => ({
    ...current,
    pendingDeleteBoardId: '',
  }));
}

export function setBoardDeleting(isDeletingBoard: boolean, boardFeedback = ''): void {
  settingsStore.update((current) => ({
    ...current,
    isDeletingBoard,
    boardFeedback,
  }));
}

export function setCredentialsSaving(isSavingCredentials: boolean, credentialsFeedback = ''): void {
  settingsStore.update((current) => ({
    ...current,
    isSavingCredentials,
    credentialsFeedback,
  }));
}

export function setBoardSaving(isSavingBoard: boolean, boardFeedback = ''): void {
  settingsStore.update((current) => ({
    ...current,
    isSavingBoard,
    boardFeedback,
  }));
}

export function setBoardAlgorithmDraft(boardAlgorithm: BoardAlgorithm): void {
  settingsStore.update((current) => ({
    ...current,
    boardAlgorithm,
    pendingDeleteBoardId: '',
  }));
}

function buildEmptyBoardDraft() {
  return {
    boardEditorMode: 'create',
    activeBoardId: '',
    boardName: '',
    boardMembersInput: '',
    boardAlgorithm: 'equal_weight_v1',
    isDeletingBoard: false,
    pendingDeleteBoardId: '',
    boardFeedback: '',
  } satisfies Pick<
    SettingsDraft,
    | 'boardEditorMode'
    | 'activeBoardId'
    | 'boardName'
    | 'boardMembersInput'
    | 'boardAlgorithm'
    | 'isDeletingBoard'
    | 'pendingDeleteBoardId'
    | 'boardFeedback'
  >;
}
