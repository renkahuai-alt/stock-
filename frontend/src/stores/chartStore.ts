import { get, writable } from 'svelte/store';
import type {
  BoardAlgorithm,
  BoardBuildStatusPayload,
  ChartLiveUpdatePayload,
  ChartPayload,
  GetChartRequest,
  MarketState,
  TargetType,
} from '../types/contracts';
import { getChart } from '../services/commands';
import { bumpRuntimeCounter } from '../services/runtimeDiagnostics';
import { chartViewportStore } from './chartViewportStore';

export type ChartViewStatus = 'idle' | 'loading' | 'ready' | 'building' | 'empty' | 'failed';

export interface LiveOverlayContext {
  watchId: string;
  targetType: TargetType;
  targetId: string;
  granularity: 'day';
  boardAlgorithm: BoardAlgorithm | null;
  updatedAt: string;
  marketState: MarketState;
  sourceStatus: string;
}

export interface ChartState extends ChartPayload {
  status: ChartViewStatus;
  errorMessage: string;
  currentRequest: GetChartRequest | null;
  currentRequestToken: number;
  lastGoodPayload: ChartPayload | null;
  liveOverlayContext: LiveOverlayContext | null;
  deferredLiveUpdate: ChartLiveUpdatePayload | null;
}

let chartRequestToken = 0;

const initialPayload: ChartPayload = {
  meta: {
    targetType: 'index',
    targetId: '',
    title: '',
    sourceStatus: '',
  },
  bars: [],
  latestTradeDate: null,
  sourceStatus: '',
};

export const chartStore = writable<ChartState>({
  ...initialPayload,
  status: 'idle',
  errorMessage: '',
  currentRequest: null,
  currentRequestToken: 0,
  lastGoodPayload: null,
  liveOverlayContext: null,
  deferredLiveUpdate: null,
});

chartViewportStore.subscribe((viewportState) => {
  if (!viewportState.autoFollowLatest) {
    return;
  }

  chartStore.update((current) => resumeDeferredLiveUpdate(current));
});

export function setChart(payload: ChartPayload): void {
  bumpRuntimeCounter('store.chart.payload-applied', {
    targetType: payload.meta.targetType,
    targetId: payload.meta.targetId,
    bars: payload.bars.length,
  });
  chartStore.update((current) => ({
    ...payload,
    status: payload.bars.length === 0 ? 'empty' : 'ready',
    errorMessage: '',
    currentRequest: current.currentRequest,
    currentRequestToken: current.currentRequestToken,
    lastGoodPayload: payload.bars.length === 0 ? current.lastGoodPayload : payload,
    liveOverlayContext: payload.activeOverlay ? current.liveOverlayContext : null,
    deferredLiveUpdate: null,
  }));
}

export function setChartBuildingState(
  request: GetChartRequest,
  title: string,
  buildStatus: BoardBuildStatusPayload,
): void {
  const requestToken = issueChartRequestToken();

  chartStore.update((current) => {
    const retainedPayload = resolveRetainedPayload(current, request, title);

    return {
      ...retainedPayload,
      status: 'building',
      errorMessage: buildStatus.buildMessage?.trim() ?? '',
      currentRequest: request,
      currentRequestToken: requestToken,
      lastGoodPayload: retainedPayload.bars.length > 0 ? retainedPayload : current.lastGoodPayload,
      liveOverlayContext: resolveRetainedLiveOverlayContext(current, request),
      deferredLiveUpdate: resolveRetainedDeferredLiveUpdate(current, request),
    };
  });
}

export function setChartBuildFailedState(
  request: GetChartRequest,
  title: string,
  buildStatus: BoardBuildStatusPayload,
): void {
  const requestToken = issueChartRequestToken();

  chartStore.update((current) => {
    const retainedPayload = resolveRetainedPayload(current, request, title);

    return {
      ...retainedPayload,
      status: 'failed',
      errorMessage: buildStatus.buildMessage?.trim() ?? '板块构建失败',
      currentRequest: request,
      currentRequestToken: requestToken,
      lastGoodPayload: retainedPayload.bars.length > 0 ? retainedPayload : current.lastGoodPayload,
      liveOverlayContext: resolveRetainedLiveOverlayContext(current, request),
      deferredLiveUpdate: resolveRetainedDeferredLiveUpdate(current, request),
    };
  });
}

export async function loadChart(payload: GetChartRequest): Promise<void> {
  const requestToken = issueChartRequestToken();

  chartStore.update((current) => ({
    ...current,
    status: 'loading',
    errorMessage: '',
    currentRequest: payload,
    currentRequestToken: requestToken,
    deferredLiveUpdate: resolveRetainedDeferredLiveUpdate(current, payload),
  }));

  try {
    const nextPayload = await getChart(payload);
    chartStore.update((current) => ({
      ...(shouldApplyChartResponse(current, payload, requestToken)
        ? buildChartResponseState(current, payload, requestToken, nextPayload)
        : current),
    }));
  } catch (error) {
    chartStore.update((current) => ({
      ...(shouldApplyChartResponse(current, payload, requestToken)
        ? {
            ...(current.lastGoodPayload ?? current),
            status: 'failed',
            errorMessage: error instanceof Error ? error.message : '图表加载失败',
            currentRequest: payload,
            currentRequestToken: requestToken,
            lastGoodPayload: current.lastGoodPayload,
            liveOverlayContext: resolveRetainedLiveOverlayContext(current, payload),
            deferredLiveUpdate: resolveRetainedDeferredLiveUpdate(current, payload),
          }
        : current),
    }));
  }
}

export function applyChartLiveUpdate(payload: ChartLiveUpdatePayload): void {
  bumpRuntimeCounter('store.chart.live-patch', {
    targetType: payload.targetType,
    targetId: payload.targetId,
    watchId: payload.watchId,
    updatedAt: payload.updatedAt,
  });
  chartStore.update((current) => {
    if (current.meta.targetType !== payload.targetType || current.meta.targetId !== payload.targetId) {
      return current;
    }

    if (
      current.currentRequest
      && (
        current.currentRequest.granularity !== payload.granularity
        || (current.currentRequest.targetType === 'board'
          && current.currentRequest.boardAlgorithm !== payload.boardAlgorithm)
      )
    ) {
      return current;
    }

    return buildChartStateForLiveUpdate(
      current,
      payload,
      get(chartViewportStore).autoFollowLatest,
    );
  });
}

function upsertOverlayBar(bars: ChartPayload['bars'], overlayBar: ChartPayload['bars'][number]): ChartPayload['bars'] {
  if (bars.length === 0) {
    return [overlayBar];
  }

  const lastBar = bars[bars.length - 1];
  if (lastBar.time === overlayBar.time) {
    return [...bars.slice(0, -1), overlayBar];
  }

  return [...bars, overlayBar];
}

function resolveRetainedPayload(
  current: ChartState,
  request: GetChartRequest,
  title: string,
): ChartPayload {
  if (matchesChartRequest(current, request)) {
    return stripChartState(current);
  }

  if (current.lastGoodPayload && matchesChartPayload(current.lastGoodPayload, current.currentRequest, request)) {
    return current.lastGoodPayload;
  }

  return {
    meta: {
      targetType: request.targetType,
      targetId: request.targetId,
      title,
      sourceStatus: '',
      granularity: request.granularity,
      range: request.range,
    },
    bars: [],
    latestTradeDate: null,
    sourceStatus: '',
  };
}

function stripChartState(current: ChartState): ChartPayload {
  return {
    meta: current.meta,
    bars: current.bars,
    latestTradeDate: current.latestTradeDate,
    sourceStatus: current.sourceStatus,
    activeOverlay: current.activeOverlay,
  };
}

function matchesChartRequest(current: ChartState, request: GetChartRequest): boolean {
  return matchesChartPayload(current, current.currentRequest, request);
}

function matchesChartPayload(
  payload: Pick<ChartPayload, 'meta'>,
  currentRequest: GetChartRequest | null,
  request: GetChartRequest,
): boolean {
  if (payload.meta.targetType !== request.targetType || payload.meta.targetId !== request.targetId) {
    return false;
  }

  if (payload.meta.granularity !== request.granularity || payload.meta.range !== request.range) {
    return false;
  }

  if (request.targetType !== 'board') {
    return true;
  }

  return currentRequest?.targetType === 'board' && currentRequest.boardAlgorithm === request.boardAlgorithm;
}

function shouldApplyChartResponse(current: ChartState, request: GetChartRequest, requestToken: number): boolean {
  return current.currentRequestToken === requestToken && isSameChartRequest(current.currentRequest, request);
}

function buildChartResponseState(
  current: ChartState,
  request: GetChartRequest,
  requestToken: number,
  nextPayload: ChartPayload,
): ChartState {
  const mergedPayload = mergeLiveOverlayIntoPayload(current, request, nextPayload);
  const liveOverlayContext = resolveLiveOverlayContextForRequest(current, request);
  const nextState: ChartState = {
    ...mergedPayload,
    status: mergedPayload.bars.length === 0 ? 'empty' : 'ready',
    errorMessage: '',
    currentRequest: request,
    currentRequestToken: requestToken,
    lastGoodPayload: mergedPayload.bars.length === 0 ? current.lastGoodPayload : mergedPayload,
    liveOverlayContext,
    deferredLiveUpdate: resolveRetainedDeferredLiveUpdate(current, request),
  };

  return resumeDeferredLiveUpdate(nextState);
}

function mergeLiveOverlayIntoPayload(
  current: ChartState,
  request: GetChartRequest,
  payload: ChartPayload,
): ChartPayload {
  if (!get(chartViewportStore).autoFollowLatest) {
    return {
      ...payload,
      activeOverlay: request.granularity === 'day' ? payload.activeOverlay : undefined,
    };
  }

  if (!current.activeOverlay || !current.liveOverlayContext || !matchesLiveOverlayContext(current.liveOverlayContext, request)) {
    return {
      ...payload,
      activeOverlay: request.granularity === 'day' ? payload.activeOverlay : undefined,
    };
  }

  const mergedBars = upsertOverlayBar(payload.bars, current.activeOverlay.bar);
  const nextSourceStatus = current.liveOverlayContext.sourceStatus || payload.sourceStatus;

  return {
    ...payload,
    bars: mergedBars,
    latestTradeDate: current.activeOverlay.bar.time,
    sourceStatus: nextSourceStatus,
    activeOverlay: current.activeOverlay,
    meta: {
      ...payload.meta,
      sourceStatus: nextSourceStatus,
    },
  };
}

function resolveRetainedLiveOverlayContext(current: ChartState, request: GetChartRequest): LiveOverlayContext | null {
  return matchesLiveOverlayContext(current.liveOverlayContext, request) ? current.liveOverlayContext : null;
}

function resolveRetainedDeferredLiveUpdate(current: ChartState, request: GetChartRequest): ChartLiveUpdatePayload | null {
  return matchesLiveUpdatePayload(current.deferredLiveUpdate, request) ? current.deferredLiveUpdate : null;
}

function resolveLiveOverlayContextForRequest(current: ChartState, request: GetChartRequest): LiveOverlayContext | null {
  return matchesLiveOverlayContext(current.liveOverlayContext, request) ? current.liveOverlayContext : null;
}

function buildChartStateForLiveUpdate(
  current: ChartState,
  payload: ChartLiveUpdatePayload,
  applyToVisibleChart: boolean,
): ChartState {
  const overlayBar = {
    time: payload.overlay.tradeDate,
    open: payload.overlay.open,
    high: payload.overlay.high,
    low: payload.overlay.low,
    close: payload.overlay.close,
    volume: payload.overlay.volume,
  };
  const nextLiveOverlayContext: LiveOverlayContext = {
    watchId: payload.watchId,
    targetType: payload.targetType,
    targetId: payload.targetId,
    granularity: 'day',
    boardAlgorithm: normalizeBoardAlgorithm(payload.targetType, payload.boardAlgorithm),
    updatedAt: payload.updatedAt,
    marketState: payload.marketState,
    sourceStatus: payload.sourceStatus,
  };

  if (!applyToVisibleChart) {
    return {
      ...current,
      liveOverlayContext: nextLiveOverlayContext,
      deferredLiveUpdate: payload,
    };
  }

  const activeOverlay: ChartPayload['activeOverlay'] = {
    kind: 'current_day',
    bar: overlayBar,
  };
  const nextBars = upsertOverlayBar(current.bars, overlayBar);
  const nextMeta = {
    ...current.meta,
    title: payload.meta.title ?? current.meta.title,
    sourceStatus: payload.meta.sourceStatus ?? payload.sourceStatus,
    providerSymbol: payload.meta.providerSymbol ?? current.meta.providerSymbol,
    providerKind: payload.meta.providerKind ?? current.meta.providerKind,
    valueMode: payload.meta.valueMode ?? current.meta.valueMode,
  };
  const nextPayload: ChartPayload = {
    meta: nextMeta,
    bars: nextBars,
    latestTradeDate: payload.overlay.tradeDate,
    sourceStatus: payload.sourceStatus,
    activeOverlay,
  };

  return {
    ...nextPayload,
    status: nextBars.length === 0 ? current.status : 'ready',
    errorMessage: '',
    currentRequest: current.currentRequest,
    currentRequestToken: current.currentRequestToken,
    lastGoodPayload: nextBars.length === 0 ? current.lastGoodPayload : nextPayload,
    liveOverlayContext: nextLiveOverlayContext,
    deferredLiveUpdate: null,
  };
}

function resumeDeferredLiveUpdate(current: ChartState): ChartState {
  if (!current.deferredLiveUpdate || !get(chartViewportStore).autoFollowLatest) {
    return current;
  }

  if (!current.currentRequest || !matchesLiveUpdatePayload(current.deferredLiveUpdate, current.currentRequest)) {
    return {
      ...current,
      deferredLiveUpdate: null,
    };
  }

  return buildChartStateForLiveUpdate(current, current.deferredLiveUpdate, true);
}

function matchesLiveOverlayContext(context: LiveOverlayContext | null, request: GetChartRequest): boolean {
  if (!context || request.granularity !== 'day') {
    return false;
  }

  return (
    context.targetType === request.targetType
    && context.targetId === request.targetId
    && context.granularity === 'day'
    && context.boardAlgorithm === normalizeBoardAlgorithm(request.targetType, request.boardAlgorithm)
  );
}

function matchesLiveUpdatePayload(payload: ChartLiveUpdatePayload | null, request: GetChartRequest): boolean {
  if (!payload || request.granularity !== 'day') {
    return false;
  }

  return (
    payload.targetType === request.targetType
    && payload.targetId === request.targetId
    && payload.granularity === 'day'
    && normalizeBoardAlgorithm(payload.targetType, payload.boardAlgorithm) === normalizeBoardAlgorithm(request.targetType, request.boardAlgorithm)
  );
}

function isSameChartRequest(left: GetChartRequest | null, right: GetChartRequest): boolean {
  if (!left) {
    return false;
  }

  return (
    left.targetType === right.targetType
    && left.targetId === right.targetId
    && left.granularity === right.granularity
    && left.range === right.range
    && normalizeBoardAlgorithm(left.targetType, left.boardAlgorithm) === normalizeBoardAlgorithm(right.targetType, right.boardAlgorithm)
  );
}

function normalizeBoardAlgorithm(targetType: TargetType, boardAlgorithm?: BoardAlgorithm): BoardAlgorithm | null {
  if (targetType !== 'board') {
    return null;
  }

  return boardAlgorithm ?? 'equal_weight_v1';
}

function issueChartRequestToken(): number {
  chartRequestToken += 1;
  return chartRequestToken;
}
