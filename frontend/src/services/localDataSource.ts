import type {
  BoardAlgorithm,
  BoardBuildStatusPayload,
  BoardMemberSummariesPayload,
  BoardSummary,
  BootstrapPayload,
  ChartPayload,
  GetBoardMemberSummariesRequest,
  GetChartRequest,
  GetTargetNoteRequest,
  IndexItem,
  MemberSummary,
  RunSyncMode,
  SaveBoardRequest,
  SaveBoardResponse,
  SaveCredentialsPayload,
  SyncStatusPayload,
  TargetNotePayload,
  TargetType,
} from '../types/contracts';
import { makeBoardBuildStatus, mockBootstrap, mockSyncStatus } from './mockData';

interface LocalDataState {
  credentials: SaveCredentialsPayload;
  syncStatus: SyncStatusPayload;
  indexes: IndexItem[];
  boards: BoardSummary[];
  membersByBoard: Record<string, MemberSummary[]>;
  notesByTarget: Record<string, TargetNotePayload>;
  activeTarget: GetTargetNoteRequest;
}

const STORAGE_KEY = 'new_stock.frontend.localData.v1';
const INDEX_PROVIDER_SYMBOLS: Record<string, string> = {
  DJI: 'DIA.US',
  IXIC: 'ONEQ.US',
  GSPC: 'SPY.US',
  RUT: 'IWM.US',
};

let memoryState: LocalDataState | null = null;
const boardBuildTimers = new Map<string, ReturnType<typeof setTimeout>[]>();

export function readBootstrapFallback(): BootstrapPayload {
  const state = readState();
  const noteKey = makeTargetKey(state.activeTarget.targetType, state.activeTarget.targetId);
  const hadNote = Boolean(state.notesByTarget[noteKey]);
  const activeTargetNote = clone(ensureTargetNote(state, state.activeTarget));

  if (!hadNote) {
    writeState(state);
  }

  return {
    indexes: clone(state.indexes),
    boards: clone(state.boards),
    membersByBoard: clone(state.membersByBoard),
    activeTargetNote,
    syncStatus: clone(state.syncStatus),
  };
}

export function saveCredentialsFallback(payload: SaveCredentialsPayload): void {
  mutateState((state) => {
    state.credentials = clone(payload);
    state.syncStatus = {
      ...state.syncStatus,
      status: 'ready',
      message: '鉴权已保存',
    };
  });
}

export function getSyncStatusFallback(): SyncStatusPayload {
  return readStateValue((state) => clone(state.syncStatus));
}

export function runSyncFallback(mode: RunSyncMode): SyncStatusPayload {
  return mutateState((state) => {
    state.syncStatus = {
      ...mockSyncStatus,
      message: mode === 'manual' ? '同步完成' : '启动同步完成',
      lastSyncAt: new Date().toISOString(),
    };

    return clone(state.syncStatus);
  });
}

export function getChartFallback(payload: GetChartRequest): ChartPayload {
  return mutateState((state) => {
    state.activeTarget = {
      targetType: payload.targetType,
      targetId: payload.targetId,
    };

    return buildChartPayload(state, payload);
  });
}

export function saveBoardFallback(payload: SaveBoardRequest): SaveBoardResponse {
  return mutateState((state) => {
    const members = normalizeMembers(payload.members);
    const boardId = payload.boardId ?? buildBoardId(payload.name);
    const existingIndex = state.boards.findIndex((board) => board.boardId === boardId);
    const buildTotal = members.length;
    const backgroundSyncStarted = members.length > 20;
    const updatedAt = new Date().toISOString();
    const buildJobId = backgroundSyncStarted ? `job-${boardId}-${Date.now()}` : undefined;

    const nextBoard: BoardSummary = {
      boardId,
      name: payload.name.trim(),
      compositionAlgorithm: payload.compositionAlgorithm,
      buildStatus: backgroundSyncStarted ? 'queued' : 'succeeded',
      buildPhase: backgroundSyncStarted ? 'queued' : 'completed',
      buildTotal,
      buildCompleted: backgroundSyncStarted ? 0 : buildTotal,
      buildFailed: 0,
      buildJobId,
      buildMessage: backgroundSyncStarted ? '等待后台构建' : undefined,
      updatedAt,
    };

    if (existingIndex >= 0) {
      state.boards.splice(existingIndex, 1, nextBoard);
    } else {
      state.boards = [nextBoard, ...state.boards];
    }

    state.membersByBoard[boardId] = assignWeights(members, payload.compositionAlgorithm);
    state.activeTarget = {
      targetType: 'board',
      targetId: boardId,
    };
    ensureTargetNote(state, state.activeTarget);

    if (backgroundSyncStarted) {
      scheduleFallbackBoardBuild(boardId, payload.compositionAlgorithm, buildTotal, buildJobId);
    } else {
      clearFallbackBoardBuild(boardId);
    }

    return {
      boardId,
      rebuildStarted: true,
      backgroundSyncStarted,
      buildStatus: nextBoard.buildStatus,
      buildPhase: nextBoard.buildPhase,
      buildJobId,
      compositionAlgorithm: payload.compositionAlgorithm,
    };
  });
}

export function getBoardBuildStatusFallback(boardId: string): BoardBuildStatusPayload {
  return readStateValue((state) => {
    const board = state.boards.find((item) => item.boardId === boardId);

    if (!board) {
      return makeBoardBuildStatus(boardId);
    }

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
  });
}

export function getBoardMemberSummariesFallback(
  payload: GetBoardMemberSummariesRequest,
): BoardMemberSummariesPayload {
  return readStateValue((state) => {
    const board = state.boards.find((item) => item.boardId === payload.boardId);
    const symbols = (state.membersByBoard[payload.boardId] ?? []).map((member) => member.symbol);

    return {
      boardId: payload.boardId,
      compositionAlgorithm: payload.compositionAlgorithm,
      members: assignWeights(symbols, payload.compositionAlgorithm),
      updatedAt: board?.updatedAt ?? new Date().toISOString(),
    };
  });
}

export function getTargetNoteFallback(payload: GetTargetNoteRequest): TargetNotePayload {
  return mutateState((state) => {
    state.activeTarget = clone(payload);
    return clone(ensureTargetNote(state, payload));
  });
}

export function saveTargetNoteFallback(payload: TargetNotePayload): TargetNotePayload {
  return mutateState((state) => {
    const saved: TargetNotePayload = {
      ...payload,
      content: payload.content,
      updatedAt: new Date().toISOString(),
    };

    state.activeTarget = {
      targetType: payload.targetType,
      targetId: payload.targetId,
    };
    state.notesByTarget[makeTargetKey(payload.targetType, payload.targetId)] = saved;

    return clone(saved);
  });
}

export function registerLocalDataStorageListener(listener: () => void): () => void {
  if (typeof window === 'undefined') {
    return () => undefined;
  }

  const handleStorage = (event: StorageEvent) => {
    if (event.key === STORAGE_KEY) {
      listener();
    }
  };

  window.addEventListener('storage', handleStorage);

  return () => {
    window.removeEventListener('storage', handleStorage);
  };
}

function buildChartPayload(state: LocalDataState, payload: GetChartRequest): ChartPayload {
  const bars = buildBars(payload);
  const title = resolveTargetTitle(state, payload.targetType, payload.targetId);
  const isIndex = payload.targetType === 'index';

  return {
    meta: {
      targetType: payload.targetType,
      targetId: payload.targetId,
      title,
      sourceStatus: resolveSourceStatus(payload),
      providerSymbol: isIndex ? INDEX_PROVIDER_SYMBOLS[payload.targetId] : undefined,
      providerKind: isIndex ? 'proxy_etf' : undefined,
      valueMode: isIndex ? 'proxy_scaled_index_points' : undefined,
      granularity: payload.granularity,
      range: payload.range,
    },
    bars,
    latestTradeDate: bars.at(-1)?.time ?? null,
    sourceStatus: resolveSourceStatus(payload),
  };
}

function buildBars(payload: GetChartRequest) {
  const count = resolveBarCount(payload.range, payload.granularity);
  const baseSeed = Math.abs(hashString(`${payload.targetType}:${payload.targetId}`));
  const algorithmFactor =
    payload.targetType === 'board' && payload.boardAlgorithm === 'market_cap_weight_v1' ? 1.06 : 1;
  const volatility =
    payload.targetType === 'index' ? 1.2 : payload.targetType === 'symbol' ? 2.4 : 1.8;
  const bars: ChartPayload['bars'] = [];
  const latestDate = new Date('2026-03-18T00:00:00Z');
  let previousClose = 90 + (baseSeed % 80);

  for (let index = count - 1; index >= 0; index -= 1) {
    const dayOffset = payload.granularity === 'week' ? index * 7 : index;
    const barDate = new Date(latestDate);
    barDate.setUTCDate(latestDate.getUTCDate() - dayOffset);

    const wave = Math.sin((count - index + (baseSeed % 7)) / 3.4) * volatility;
    const drift = ((baseSeed % 11) - 5) * 0.08;
    const open = roundTo(previousClose + drift);
    const close = roundTo((open + wave) * algorithmFactor);
    const high = roundTo(Math.max(open, close) + 0.9 + ((baseSeed + index) % 3));
    const low = roundTo(Math.min(open, close) - 0.7 - ((baseSeed + index) % 2));
    const volumeBase = payload.targetType === 'index' ? 900_000 : payload.targetType === 'symbol' ? 520_000 : 260_000;

    bars.push({
      time: barDate.toISOString().slice(0, 10),
      open,
      high,
      low,
      close,
      volume: volumeBase + (count - index) * 4_000 + (baseSeed % 10) * 1_000,
    });

    previousClose = close;
  }

  return bars;
}

function resolveBarCount(range: GetChartRequest['range'], granularity: GetChartRequest['granularity']): number {
  const dayCountMap: Record<GetChartRequest['range'], number> = {
    '1m': 22,
    '3m': 66,
    '6m': 132,
    '1y': 264,
    '3y': 520,
    all: 780,
  };

  const count = dayCountMap[range];

  return granularity === 'week' ? Math.max(12, Math.round(count / 5)) : count;
}

function resolveSourceStatus(payload: GetChartRequest): string {
  if (payload.targetType === 'index') {
    return 'proxy_etf_cache';
  }

  if (payload.targetType === 'board') {
    return payload.boardAlgorithm === 'market_cap_weight_v1' ? 'board_market_cap_cache' : 'board_equal_weight_cache';
  }

  return 'local_cache';
}

function resolveTargetTitle(state: LocalDataState, targetType: TargetType, targetId: string): string {
  if (targetType === 'index') {
    return state.indexes.find((item) => item.id === targetId)?.label ?? targetId;
  }

  if (targetType === 'board') {
    return state.boards.find((item) => item.boardId === targetId)?.name ?? targetId;
  }

  return targetId.toUpperCase();
}

function assignWeights(symbols: string[], algorithm: BoardAlgorithm): MemberSummary[] {
  if (symbols.length === 0) {
    return [];
  }

  if (algorithm === 'equal_weight_v1') {
    const evenWeight = 100 / symbols.length;

    return symbols.map((symbol, index) => ({
      symbol,
      weightPercent: roundTo(index === symbols.length - 1 ? 100 - evenWeight * index : evenWeight, 1),
    }));
  }

  const rawWeights = symbols.map((symbol) => Math.abs(hashString(symbol)) % 100 + 20);
  const total = rawWeights.reduce((sum, value) => sum + value, 0);
  let allocated = 0;

  return symbols.map((symbol, index) => {
    const weight = index === symbols.length - 1 ? 100 - allocated : roundTo((rawWeights[index] / total) * 100, 1);
    allocated += index === symbols.length - 1 ? 0 : weight;

    return {
      symbol,
      weightPercent: weight,
    };
  });
}

function ensureTargetNote(state: LocalDataState, payload: GetTargetNoteRequest): TargetNotePayload {
  const key = makeTargetKey(payload.targetType, payload.targetId);

  if (!state.notesByTarget[key]) {
    state.notesByTarget[key] = {
      targetType: payload.targetType,
      targetId: payload.targetId,
      content: '',
      updatedAt: null,
    };
  }

  return state.notesByTarget[key];
}

function normalizeMembers(members: string[]): string[] {
  return Array.from(
    new Set(
      members
        .map((member) => member.trim().toUpperCase())
        .filter(Boolean),
    ),
  );
}

function buildBoardId(name: string): string {
  return `board-${name
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9\u4e00-\u9fa5]+/g, '-')
    .replace(/^-+|-+$/g, '')}`;
}

function scheduleFallbackBoardBuild(
  boardId: string,
  compositionAlgorithm: BoardAlgorithm,
  buildTotal: number,
  buildJobId: string | undefined,
): void {
  clearFallbackBoardBuild(boardId);

  const runningTimer = setTimeout(() => {
    mutateState((state) => {
      const board = state.boards.find((item) => item.boardId === boardId);

      if (!board || board.buildJobId !== buildJobId) {
        return;
      }

      board.compositionAlgorithm = compositionAlgorithm;
      board.buildStatus = 'running';
      board.buildPhase = 'fetching_history';
      board.buildCompleted = Math.max(1, Math.floor(buildTotal / 2));
      board.buildFailed = 0;
      board.buildMessage = '后台构建中';
      board.updatedAt = new Date().toISOString();
    });
  }, 500);

  const completedTimer = setTimeout(() => {
    mutateState((state) => {
      const board = state.boards.find((item) => item.boardId === boardId);

      if (!board || board.buildJobId !== buildJobId) {
        return;
      }

      board.compositionAlgorithm = compositionAlgorithm;
      board.buildStatus = 'succeeded';
      board.buildPhase = 'completed';
      board.buildCompleted = buildTotal;
      board.buildFailed = 0;
      board.buildJobId = undefined;
      board.buildMessage = undefined;
      board.updatedAt = new Date().toISOString();
    });
    clearFallbackBoardBuild(boardId);
  }, 1600);

  boardBuildTimers.set(boardId, [runningTimer, completedTimer]);
}

function clearFallbackBoardBuild(boardId: string): void {
  const timers = boardBuildTimers.get(boardId);

  if (!timers) {
    return;
  }

  for (const timer of timers) {
    clearTimeout(timer);
  }

  boardBuildTimers.delete(boardId);
}

function mutateState<T>(mutator: (state: LocalDataState) => T): T {
  const state = readState();
  const result = mutator(state);
  writeState(state);

  if (result === undefined) {
    return result;
  }

  return clone(result);
}

function readStateValue<T>(reader: (state: LocalDataState) => T): T {
  return reader(readState());
}

function readState(): LocalDataState {
  const storage = getStorage();

  if (!storage) {
    if (!memoryState) {
      memoryState = createInitialState();
    }

    return clone(memoryState);
  }

  const raw = storage.getItem(STORAGE_KEY);

  if (!raw) {
    const seededState = createInitialState();
    storage.setItem(STORAGE_KEY, JSON.stringify(seededState));
    memoryState = clone(seededState);

    return seededState;
  }

  try {
    const parsed = JSON.parse(raw) as LocalDataState;
    memoryState = clone(parsed);

    return parsed;
  } catch {
    const resetState = createInitialState();
    storage.setItem(STORAGE_KEY, JSON.stringify(resetState));
    memoryState = clone(resetState);

    return resetState;
  }
}

function writeState(state: LocalDataState): void {
  memoryState = clone(state);
  const storage = getStorage();

  if (!storage) {
    return;
  }

  storage.setItem(STORAGE_KEY, JSON.stringify(state));
}

function createInitialState(): LocalDataState {
  const notesByTarget: Record<string, TargetNotePayload> = {
    [makeTargetKey(mockBootstrap.activeTargetNote.targetType, mockBootstrap.activeTargetNote.targetId)]: clone(
      mockBootstrap.activeTargetNote,
    ),
  };

  return {
    credentials: {
      appKey: '',
      appSecret: '',
      accessToken: '',
    },
    syncStatus: clone(mockSyncStatus),
    indexes: clone(mockBootstrap.indexes),
    boards: clone(mockBootstrap.boards),
    membersByBoard: clone(mockBootstrap.membersByBoard),
    notesByTarget,
    activeTarget: {
      targetType: mockBootstrap.activeTargetNote.targetType,
      targetId: mockBootstrap.activeTargetNote.targetId,
    },
  };
}

function getStorage(): Storage | null {
  if (typeof window === 'undefined') {
    return null;
  }

  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

function makeTargetKey(targetType: TargetType, targetId: string): string {
  return `${targetType}:${targetId}`;
}

function hashString(value: string): number {
  let hash = 0;

  for (let index = 0; index < value.length; index += 1) {
    hash = (hash << 5) - hash + value.charCodeAt(index);
    hash |= 0;
  }

  return hash;
}

function roundTo(value: number, digits = 2): number {
  return Number(value.toFixed(digits));
}

function clone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}
