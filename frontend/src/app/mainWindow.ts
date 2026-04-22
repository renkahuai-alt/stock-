import '../styles/tokens.css';
import '../styles/app.css';
import { mount } from 'svelte';
import { installEntryDiagnostics, markBootStage } from './bootstrapDiagnostics';
import MainWindow from '../windows/main/MainWindow.svelte';
import {
  handleChartLiveUpdateEvent,
  initializeMainWindowWatchRuntime,
  shutdownChartWatchRuntime,
} from '../services/chartWatchFlow';
import { registerCoreListeners } from '../services/events';
import { formatErrorMessage } from '../services/errors';
import { installRuntimeMetadata, recordRuntimeSignal, serializeError } from '../services/runtimeDiagnostics';
import { applyBoardBuildStatus } from '../stores/appStore';
import { upsertBoardBuildStatus } from '../stores/boardBuildStore';
import {
  handleBoardBuildStatusEvent,
  handleSyncStatusEvent,
  refreshBootstrapCatalogState,
  recoverStartupSyncResult,
  syncBootstrapState,
} from '../services/mainFlow';
import { setSyncStatusFailure } from '../stores/syncStore';
import { registerWindowCleanup, resolveMountTarget } from './shared';

let initialized = false;

export function mountMainWindow(): void {
  installEntryDiagnostics('main');
  installRuntimeMetadata({
    windowName: 'main',
    localFallbackEnabled: import.meta.env.VITE_ENABLE_LOCAL_FALLBACK === 'true',
  });
  recordRuntimeSignal('window.main.mount-requested');
  markBootStage('main', 'mount-start');
  mount(MainWindow, {
    target: resolveMountTarget('Main window'),
  });
  markBootStage('main', 'mount-complete');
  recordRuntimeSignal('window.main.mount-complete');

  if (!initialized) {
    initialized = true;
    void initializeMainWindow();
  }
}

async function initializeMainWindow(): Promise<void> {
  markBootStage('main', 'bootstrap-start');
  recordRuntimeSignal('window.main.bootstrap-start');

  try {
    await syncBootstrapState();
    markBootStage('main', 'bootstrap-sync-complete');
    recordRuntimeSignal('window.main.bootstrap-sync-complete');
  } catch (error) {
    const message = formatErrorMessage(error, '未知错误');
    markBootStage('main', 'bootstrap-sync-failed', { message });
    recordRuntimeSignal('window.main.bootstrap-sync-failed', {
      message,
      error: serializeError(error),
    });
    setSyncStatusFailure(`主窗口初始化失败：${message}`);
  }

  try {
    const unlisten = await registerCoreListeners({
      onSyncStatus: (payload) => {
        void handleSyncStatusEvent(payload).catch((error) => {
          recordRuntimeSignal('window.main.sync-status-handle-failed', {
            error: serializeError(error),
          });
          setSyncStatusFailure(`同步结果刷新失败：${formatErrorMessage(error, '未知错误')}`);
        });
      },
      onBoardBuildStatus: (payload) => {
        applyBoardBuildStatus(payload);
        upsertBoardBuildStatus(payload);
        void handleBoardBuildStatusEvent(payload);
      },
      onChartLiveUpdate: (payload) => handleChartLiveUpdateEvent(payload),
      onSettingsSaved: () => {
        recordRuntimeSignal('window.main.settings-saved-received');
        void refreshBootstrapCatalogState().catch((error) => {
          recordRuntimeSignal('window.main.catalog-refresh-failed', {
            error: serializeError(error),
          });
          setSyncStatusFailure(`目录刷新失败：${formatErrorMessage(error, '未知错误')}`);
        });
      },
    });
    markBootStage('main', 'listeners-registered');
    recordRuntimeSignal('window.main.listeners-registered');

    const disposeWatchRuntime = await initializeMainWindowWatchRuntime();
    markBootStage('main', 'watch-runtime-ready');
    recordRuntimeSignal('window.main.watch-runtime-ready');
    await recoverStartupSyncResult();
    markBootStage('main', 'startup-sync-recovered');

    registerWindowCleanup(() => {
      unlisten.forEach((dispose) => dispose());
      disposeWatchRuntime();
      void shutdownChartWatchRuntime();
    });

    markBootStage('main', 'bootstrap-complete');
    recordRuntimeSignal('window.main.bootstrap-complete');
  } catch (error) {
    const message = formatErrorMessage(error, '未知错误');
    markBootStage('main', 'bootstrap-failed', { message });
    recordRuntimeSignal('window.main.bootstrap-failed', {
      message,
      error: serializeError(error),
    });
    setSyncStatusFailure(`主窗口初始化失败：${message}`);
  }
}
