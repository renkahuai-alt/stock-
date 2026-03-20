import { Window, getCurrentWindow } from '@tauri-apps/api/window';
import { get } from 'svelte/store';
import { startChartWatch, stopChartWatch } from './commands';
import { formatErrorMessage } from './errors';
import { bumpRuntimeCounter, recordRuntimeSignal, serializeError, setRuntimeSnapshot } from './runtimeDiagnostics';
import { applyChartLiveUpdate, chartStore, type ChartState } from '../stores/chartStore';
import { selectionStore, toGetChartRequest } from '../stores/selectionStore';
import type { ChartLiveUpdatePayload, GetChartRequest, StartChartWatchRequest } from '../types/contracts';
import {
  applyLiveUpdateToWatch,
  applyStartWatchResult,
  applyStopWatchResult,
  buildWatchKey,
  buildWatchKeyFromLiveUpdate,
  bumpWatchToken,
  clearWatchStateForNoDemand,
  isSameWatchKey,
  setWatchError,
  setWatchRuntimeReady,
  setWatchStarting,
  setWatchStopping,
  setWatchWindowState,
  shouldHoldTerminalWatchState,
  type WatchKey,
  type WatchState,
  watchStore,
} from '../stores/watchStore';

let reconcileQueue: Promise<void> = Promise.resolve();

export function scheduleChartWatchReconcile(): Promise<void> {
  bumpRuntimeCounter('watch.reconcile-requested');
  const token = bumpWatchToken();
  reconcileQueue = reconcileQueue
    .catch(() => undefined)
    .then(() => reconcileChartWatch(token));

  return reconcileQueue;
}

export async function initializeMainWindowWatchRuntime(): Promise<() => void> {
  const dispose: Array<() => void> = [];
  const handleRuntimeSignal = () => {
    void refreshMainWindowRuntimeState().then(() => scheduleChartWatchReconcile());
  };

  document.addEventListener('visibilitychange', handleRuntimeSignal);
  window.addEventListener('focus', handleRuntimeSignal);
  window.addEventListener('blur', handleRuntimeSignal);

  dispose.push(() => document.removeEventListener('visibilitychange', handleRuntimeSignal));
  dispose.push(() => window.removeEventListener('focus', handleRuntimeSignal));
  dispose.push(() => window.removeEventListener('blur', handleRuntimeSignal));

  try {
    const currentWindow = getCurrentWindow();
    dispose.push(await currentWindow.onFocusChanged(() => handleRuntimeSignal()));
  } catch {
    // 浏览器预览环境下忽略 Tauri 窗口监听失败
  }

  await refreshMainWindowRuntimeState();
  setWatchRuntimeReady(true);
  syncWatchDiagnosticsSnapshot('runtime-ready');
  await scheduleChartWatchReconcile();
  recordRuntimeSignal('watch.runtime-initialized');

  return () => {
    setWatchRuntimeReady(false);
    syncWatchDiagnosticsSnapshot('runtime-disposed');
    dispose.forEach((fn) => fn());
  };
}

export async function shutdownChartWatchRuntime(): Promise<void> {
  setWatchRuntimeReady(false);
  const token = bumpWatchToken();
  await stopWatchIfNeeded(token, null);
  syncWatchDiagnosticsSnapshot('runtime-shutdown');
}

export function handleChartLiveUpdateEvent(payload: ChartLiveUpdatePayload): void {
  const selection = get(selectionStore);
  const watch = get(watchStore);

  if (selection.granularity !== 'day') {
    return;
  }

  if (selection.targetType !== payload.targetType || selection.targetId !== payload.targetId) {
    return;
  }

  if (watch.watchId !== payload.watchId || !isSameWatchKey(watch.activeKey, buildWatchKeyFromLiveUpdate(payload))) {
    return;
  }

  if (!applyLiveUpdateToWatch(payload)) {
    bumpRuntimeCounter('watch.live-update-dropped', {
      watchId: payload.watchId,
      targetType: payload.targetType,
      targetId: payload.targetId,
      updatedAt: payload.updatedAt,
    });
    return;
  }

  applyChartLiveUpdate(payload);
  bumpRuntimeCounter('watch.live-update-applied', {
    watchId: payload.watchId,
    updatedAt: payload.updatedAt,
  });
  syncWatchDiagnosticsSnapshot('live-update-applied');
}

async function reconcileChartWatch(token: number): Promise<void> {
  if (!isCurrentToken(token)) {
    return;
  }

  const desiredRequest = resolveDesiredWatchRequest();
  const currentWatch = get(watchStore);

  if (!desiredRequest) {
    await releaseWatchDemand(token);
    return;
  }

  const desiredKey = buildWatchKey(desiredRequest);

  if (shouldHoldTerminalWatchState(currentWatch, desiredKey)) {
    return;
  }

  if (
    currentWatch.phase === 'active'
    && currentWatch.watchId
    && isSameWatchKey(currentWatch.activeKey, desiredKey)
  ) {
    return;
  }

  if (needsStopBeforeStart(currentWatch, desiredKey)) {
    const stopped = await stopWatchIfNeeded(token, desiredKey);

    if (!stopped || !isCurrentToken(token)) {
      return;
    }
  }

  if (!setWatchStarting(token, desiredKey)) {
    return;
  }

  try {
    recordRuntimeSignal('watch.start-requested', desiredRequest);
    const result = await startChartWatch(desiredRequest);

    if (!applyStartWatchResult(token, result)) {
      await stopStaleWatch(result.watchId);
      recordRuntimeSignal('watch.start-stale-result-stopped', {
        watchId: result.watchId,
      });
      return;
    }

    recordRuntimeSignal('watch.start-applied', {
      watchId: result.watchId,
      started: result.started,
      marketState: result.marketState,
      targetType: result.targetType,
      targetId: result.targetId,
      boardAlgorithm: result.boardAlgorithm ?? null,
    });
    syncWatchDiagnosticsSnapshot('start-applied');
  } catch (error) {
    recordRuntimeSignal('watch.start-failed', {
      desiredKey: serializeWatchKey(desiredKey),
      error: serializeError(error),
    });
    setWatchError(token, desiredKey, `盘中更新暂不可用：${formatErrorMessage(error, '未知错误')}`);
    syncWatchDiagnosticsSnapshot('start-failed');
  }
}

async function releaseWatchDemand(token: number): Promise<void> {
  const currentWatch = get(watchStore);

  if (hasManagedWatch(currentWatch)) {
    await stopWatchIfNeeded(token, null);
    return;
  }

  clearWatchStateForNoDemand(token);
}

async function stopWatchIfNeeded(token: number, desiredKey: WatchKey | null): Promise<boolean> {
  const currentWatch = get(watchStore);

  if (!hasManagedWatch(currentWatch)) {
    return true;
  }

  if (!setWatchStopping(token, desiredKey)) {
    return false;
  }

  try {
    recordRuntimeSignal('watch.stop-requested', {
      desiredKey: serializeWatchKey(desiredKey),
    });
    const result = await stopChartWatch();
    const applied = applyStopWatchResult(token, result, desiredKey);

    if (applied) {
      recordRuntimeSignal('watch.stop-applied', {
        watchId: result.watchId ?? null,
      });
      syncWatchDiagnosticsSnapshot('stop-applied');
    }

    return applied;
  } catch (error) {
    recordRuntimeSignal('watch.stop-failed', {
      desiredKey: serializeWatchKey(desiredKey),
      error: serializeError(error),
    });
    setWatchError(token, desiredKey, `停止盘中更新失败：${formatErrorMessage(error, '未知错误')}`);
    syncWatchDiagnosticsSnapshot('stop-failed');
    return false;
  }
}

function resolveDesiredWatchRequest(): StartChartWatchRequest | null {
  const selection = get(selectionStore);
  const chart = get(chartStore);
  const watch = get(watchStore);

  if (!watch.runtimeReady || !watch.documentVisible || !watch.windowVisible || watch.windowMinimized || !watch.appForeground) {
    return null;
  }

  if (!selection.targetId || selection.granularity !== 'day') {
    return null;
  }

  if (chart.status !== 'ready' && chart.status !== 'empty') {
    return null;
  }

  const expectedRequest = toGetChartRequest(selection);

  if (!isSameChartRequest(chart.currentRequest, expectedRequest)) {
    return null;
  }

  return selection.targetType === 'board'
    ? {
        targetType: selection.targetType,
        targetId: selection.targetId,
        granularity: 'day',
        boardAlgorithm: selection.boardAlgorithm,
      }
    : {
        targetType: selection.targetType,
        targetId: selection.targetId,
        granularity: 'day',
      };
}

async function refreshMainWindowRuntimeState(): Promise<void> {
  const documentVisible = document.visibilityState !== 'hidden';

  let windowVisible = true;
  let windowMinimized = false;
  let appForeground = documentVisible && document.hasFocus();

  try {
    const currentWindow = getCurrentWindow();
    const [visible, minimized, focusedWindow] = await Promise.all([
      currentWindow.isVisible(),
      currentWindow.isMinimized(),
      Window.getFocusedWindow(),
    ]);

    windowVisible = visible;
    windowMinimized = minimized;
    appForeground = documentVisible && focusedWindow !== null;
  } catch {
    appForeground = documentVisible && document.hasFocus();
  }

  setWatchWindowState({
    documentVisible,
    appForeground,
    windowVisible,
    windowMinimized,
  });
  syncWatchDiagnosticsSnapshot('window-state-refreshed');
}

function hasManagedWatch(state: WatchState): boolean {
  return Boolean(
    state.watchId
    || state.activeKey
    || state.phase === 'active'
    || state.phase === 'starting'
    || state.phase === 'stopping',
  );
}

function needsStopBeforeStart(
  state: WatchState,
  desiredKey: WatchKey,
): boolean {
  if (!hasManagedWatch(state)) {
    return false;
  }

  return !state.watchId || !isSameWatchKey(state.activeKey, desiredKey) || state.phase !== 'active';
}

function isCurrentToken(token: number): boolean {
  return get(watchStore).currentToken === token;
}

function isSameChartRequest(
  left: ChartState['currentRequest'],
  right: GetChartRequest,
): boolean {
  if (!left) {
    return false;
  }

  return (
    left.targetType === right.targetType
    && left.targetId === right.targetId
    && left.granularity === right.granularity
    && left.range === right.range
    && (left.targetType !== 'board' || left.boardAlgorithm === right.boardAlgorithm)
  );
}

async function stopStaleWatch(watchId: string): Promise<void> {
  if (!watchId) {
    return;
  }

  try {
    await stopChartWatch();
  } catch {
    return;
  }
}

function syncWatchDiagnosticsSnapshot(reason: string): void {
  const watch = get(watchStore);

  setRuntimeSnapshot('watch.active', {
    reason,
    phase: watch.phase,
    watchId: watch.watchId,
    desiredKey: serializeWatchKey(watch.desiredKey),
    activeKey: serializeWatchKey(watch.activeKey),
    marketState: watch.marketState,
    sourceStatus: watch.sourceStatus,
    updatedAt: watch.updatedAt,
    lastEventUpdatedAt: watch.lastEventUpdatedAt,
    runtimeReady: watch.runtimeReady,
    documentVisible: watch.documentVisible,
    appForeground: watch.appForeground,
    windowVisible: watch.windowVisible,
    windowMinimized: watch.windowMinimized,
  });
}

function serializeWatchKey(key: WatchKey | null): Record<string, unknown> | null {
  if (!key) {
    return null;
  }

  return {
    targetType: key.targetType,
    targetId: key.targetId,
    granularity: key.granularity,
    boardAlgorithm: key.boardAlgorithm,
  };
}
