type PrimitiveDetail = string | number | boolean | null | undefined;
type StructuredDetail = object;
export type RuntimeDetail = PrimitiveDetail | StructuredDetail;

interface RuntimeCounterEntry {
  count: number;
  lastAt: string;
  lastDetail: RuntimeDetail;
}

interface RuntimeTimelineEntry {
  channel: string;
  at: string;
  detail: RuntimeDetail;
}

interface RuntimeDiagnosticsState {
  metadata: Record<string, unknown>;
  counters: Record<string, RuntimeCounterEntry>;
  snapshots: Record<string, RuntimeDetail>;
  timeline: RuntimeTimelineEntry[];
}

declare global {
  interface Window {
    __NEW_STOCK_RUNTIME_DIAGNOSTICS__?: RuntimeDiagnosticsState;
    __TAURI_INTERNALS__?: unknown;
    __TAURI__?: unknown;
  }
}

const TIMELINE_LIMIT = 120;

function now(): string {
  return new Date().toISOString();
}

function ensureState(): RuntimeDiagnosticsState | null {
  if (typeof window === 'undefined') {
    return null;
  }

  if (!window.__NEW_STOCK_RUNTIME_DIAGNOSTICS__) {
    window.__NEW_STOCK_RUNTIME_DIAGNOSTICS__ = {
      metadata: {},
      counters: {},
      snapshots: {},
      timeline: [],
    };
  }

  return window.__NEW_STOCK_RUNTIME_DIAGNOSTICS__;
}

export function installRuntimeMetadata(metadata: Record<string, unknown>): void {
  const state = ensureState();

  if (!state) {
    return;
  }

  state.metadata = {
    ...state.metadata,
    ...metadata,
  };

  if (typeof document === 'undefined') {
    return;
  }

  const windowName = metadata.windowName;
  if (typeof windowName === 'string') {
    document.documentElement.dataset.runtimeWindow = windowName;
  }

  const localFallbackEnabled = metadata.localFallbackEnabled;
  if (typeof localFallbackEnabled === 'boolean') {
    document.documentElement.dataset.localFallback = localFallbackEnabled ? 'true' : 'false';
  }
}

export function bumpRuntimeCounter(channel: string, detail: RuntimeDetail = null): number {
  const state = ensureState();

  if (!state) {
    return 0;
  }

  const current = state.counters[channel];
  const count = (current?.count ?? 0) + 1;

  state.counters[channel] = {
    count,
    lastAt: now(),
    lastDetail: detail,
  };

  return count;
}

export function setRuntimeSnapshot(name: string, detail: RuntimeDetail): void {
  const state = ensureState();

  if (!state) {
    return;
  }

  state.snapshots[name] = detail;
}

export function recordRuntimeSignal(channel: string, detail: RuntimeDetail = null): number {
  const count = bumpRuntimeCounter(channel, detail);
  const state = ensureState();

  if (state) {
    state.timeline.push({
      channel,
      at: now(),
      detail,
    });

    if (state.timeline.length > TIMELINE_LIMIT) {
      state.timeline.splice(0, state.timeline.length - TIMELINE_LIMIT);
    }
  }

  console.info(`[runtime] ${channel}#${count}`, detail ?? '');
  return count;
}

export function serializeError(error: unknown): Record<string, unknown> {
  if (error instanceof Error) {
    return {
      name: error.name,
      message: error.message,
      stack: error.stack,
    };
  }

  if (typeof error === 'object' && error !== null) {
    return {
      value: String(error),
    };
  }

  return {
    value: error ?? null,
  };
}

export function isTauriRuntimeAvailable(): boolean {
  if (typeof window === 'undefined') {
    return false;
  }

  return '__TAURI_INTERNALS__' in window || '__TAURI__' in window;
}
