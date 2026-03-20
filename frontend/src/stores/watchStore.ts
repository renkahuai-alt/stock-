import { writable } from 'svelte/store';
import type {
  BoardAlgorithm,
  ChartLiveUpdatePayload,
  ChartWatchStatusPayload,
  MarketState,
  StartChartWatchRequest,
  StopChartWatchPayload,
  TargetType,
} from '../types/contracts';

export type WatchPhase = 'idle' | 'starting' | 'active' | 'inactive' | 'stopping' | 'error';

export interface WatchKey {
  targetType: TargetType;
  targetId: string;
  granularity: 'day';
  boardAlgorithm: BoardAlgorithm | null;
}

export interface WatchState {
  runtimeReady: boolean;
  currentToken: number;
  phase: WatchPhase;
  desiredKey: WatchKey | null;
  activeKey: WatchKey | null;
  watchId: string | null;
  intervalSec: number | null;
  marketState: MarketState | null;
  sourceStatus: string;
  updatedAt: string | null;
  lastEventUpdatedAt: string | null;
  lastMessage: string;
  documentVisible: boolean;
  appForeground: boolean;
  windowVisible: boolean;
  windowMinimized: boolean;
}

const initialState: WatchState = {
  runtimeReady: false,
  currentToken: 0,
  phase: 'idle',
  desiredKey: null,
  activeKey: null,
  watchId: null,
  intervalSec: null,
  marketState: null,
  sourceStatus: '',
  updatedAt: null,
  lastEventUpdatedAt: null,
  lastMessage: '',
  documentVisible: true,
  appForeground: true,
  windowVisible: true,
  windowMinimized: false,
};

export const watchStore = writable<WatchState>(initialState);

export function normalizeWatchBoardAlgorithm(
  targetType: TargetType,
  boardAlgorithm?: BoardAlgorithm,
): BoardAlgorithm | null {
  if (targetType !== 'board') {
    return null;
  }

  return boardAlgorithm ?? 'equal_weight_v1';
}

export function buildWatchKey(payload: StartChartWatchRequest): WatchKey {
  return {
    targetType: payload.targetType,
    targetId: payload.targetId,
    granularity: 'day',
    boardAlgorithm: normalizeWatchBoardAlgorithm(payload.targetType, payload.boardAlgorithm),
  };
}

export function buildWatchKeyFromLiveUpdate(payload: ChartLiveUpdatePayload): WatchKey {
  return {
    targetType: payload.targetType,
    targetId: payload.targetId,
    granularity: 'day',
    boardAlgorithm: normalizeWatchBoardAlgorithm(payload.targetType, payload.boardAlgorithm),
  };
}

export function isSameWatchKey(left: WatchKey | null, right: WatchKey | null): boolean {
  if (!left || !right) {
    return left === right;
  }

  return (
    left.targetType === right.targetType
    && left.targetId === right.targetId
    && left.granularity === right.granularity
    && left.boardAlgorithm === right.boardAlgorithm
  );
}

export function bumpWatchToken(): number {
  let nextToken = 0;

  watchStore.update((current) => {
    nextToken = current.currentToken + 1;
    return {
      ...current,
      currentToken: nextToken,
    };
  });

  return nextToken;
}

export function setWatchRuntimeReady(runtimeReady: boolean): void {
  watchStore.update((current) => ({
    ...current,
    runtimeReady,
  }));
}

export function setWatchWindowState(
  nextState: Partial<Pick<WatchState, 'documentVisible' | 'appForeground' | 'windowVisible' | 'windowMinimized'>>,
): void {
  watchStore.update((current) => ({
    ...current,
    ...nextState,
  }));
}

export function setWatchStarting(token: number, desiredKey: WatchKey): boolean {
  return applyIfCurrentToken(token, (current) => ({
    ...current,
    phase: 'starting',
    desiredKey,
    activeKey: current.activeKey,
    sourceStatus: '',
    lastEventUpdatedAt: null,
    lastMessage: '',
  }));
}

export function applyStartWatchResult(token: number, payload: ChartWatchStatusPayload): boolean {
  let applied = false;

  watchStore.update((current) => {
    if (current.currentToken !== token) {
      return current;
    }

    const nextKey = buildWatchKey({
      targetType: payload.targetType,
      targetId: payload.targetId,
      granularity: 'day',
      boardAlgorithm: payload.boardAlgorithm,
    });

    if (!isSameWatchKey(current.desiredKey, nextKey)) {
      return current;
    }

    applied = true;

    if (!payload.started) {
      return {
        ...current,
        phase: 'inactive',
        desiredKey: nextKey,
        activeKey: null,
        watchId: null,
        intervalSec: payload.intervalSec,
        marketState: payload.marketState,
        sourceStatus: '',
        updatedAt: payload.updatedAt,
        lastEventUpdatedAt: null,
        lastMessage: payload.message ?? '',
      };
    }

    return {
      ...current,
      phase: 'active',
      desiredKey: nextKey,
      activeKey: nextKey,
      watchId: payload.watchId,
      intervalSec: payload.intervalSec,
      marketState: payload.marketState,
      sourceStatus: '',
      updatedAt: payload.updatedAt,
      lastEventUpdatedAt: null,
      lastMessage: payload.message ?? '',
    };
  });

  return applied;
}

export function setWatchStopping(token: number, desiredKey: WatchKey | null): boolean {
  return applyIfCurrentToken(token, (current) => ({
    ...current,
    phase: 'stopping',
    desiredKey,
    lastMessage: current.lastMessage,
  }));
}

export function applyStopWatchResult(token: number, payload: StopChartWatchPayload, desiredKey: WatchKey | null): boolean {
  return applyIfCurrentToken(token, (current) => ({
    ...current,
    phase: desiredKey ? 'inactive' : 'idle',
    desiredKey,
    activeKey: null,
    watchId: null,
    intervalSec: null,
    updatedAt: payload.updatedAt,
    lastEventUpdatedAt: null,
    marketState: null,
    sourceStatus: '',
    lastMessage: '',
  }));
}

export function setWatchError(token: number, desiredKey: WatchKey | null, message: string): boolean {
  return applyIfCurrentToken(token, (current) => ({
    ...current,
    phase: 'error',
    desiredKey,
    activeKey: null,
    watchId: null,
    intervalSec: null,
    marketState: null,
    sourceStatus: '',
    lastEventUpdatedAt: null,
    lastMessage: message,
  }));
}

export function clearWatchStateForNoDemand(token: number): boolean {
  return applyIfCurrentToken(token, (current) => ({
    ...current,
    phase: 'idle',
    desiredKey: null,
    activeKey: null,
    watchId: null,
    intervalSec: null,
    marketState: null,
    sourceStatus: '',
    updatedAt: null,
    lastEventUpdatedAt: null,
    lastMessage: '',
  }));
}

export function applyLiveUpdateToWatch(payload: ChartLiveUpdatePayload): boolean {
  let applied = false;

  watchStore.update((current) => {
    if (current.watchId !== payload.watchId || !isSameWatchKey(current.activeKey, buildWatchKeyFromLiveUpdate(payload))) {
      return current;
    }

    if (!isIncomingTimestampNewer(current.lastEventUpdatedAt, payload.updatedAt)) {
      return current;
    }

    applied = true;

    const marketClosed = payload.marketState === 'closed' || payload.sourceStatus === 'market_closed';

    return {
      ...current,
      phase: marketClosed ? 'inactive' : 'active',
      activeKey: marketClosed ? null : current.activeKey,
      watchId: marketClosed ? null : current.watchId,
      marketState: payload.marketState,
      sourceStatus: payload.sourceStatus,
      updatedAt: payload.updatedAt,
      lastEventUpdatedAt: payload.updatedAt,
      lastMessage: payload.meta.message ?? current.lastMessage,
    };
  });

  return applied;
}

export function resetWatchState(): void {
  watchStore.set(initialState);
}

export function isIncomingTimestampNewer(currentValue: string | null, incomingValue: string): boolean {
  if (!currentValue) {
    return true;
  }

  const currentTime = Date.parse(currentValue);
  const incomingTime = Date.parse(incomingValue);

  if (Number.isNaN(currentTime) || Number.isNaN(incomingTime)) {
    return incomingValue > currentValue;
  }

  return incomingTime > currentTime;
}

export function shouldHoldTerminalWatchState(
  state: Pick<WatchState, 'phase' | 'desiredKey' | 'marketState'>,
  desiredKey: WatchKey,
): boolean {
  if (!isSameWatchKey(state.desiredKey, desiredKey)) {
    return false;
  }

  return state.phase === 'error' || (state.phase === 'inactive' && state.marketState === 'closed');
}

function applyIfCurrentToken(token: number, updater: (current: WatchState) => WatchState): boolean {
  let applied = false;

  watchStore.update((current) => {
    if (current.currentToken !== token) {
      return current;
    }

    applied = true;
    return updater(current);
  });

  return applied;
}
