type BootWindowName = 'main' | 'settings';

type BootDetail = Record<string, unknown> | string | number | boolean | null | undefined;

declare global {
  interface Window {
    __NEW_STOCK_BOOT__?: Record<string, unknown>;
    __NEW_STOCK_MARK_BOOT__?: (
      windowName: BootWindowName,
      stage: string,
      detail?: BootDetail,
    ) => void;
  }
}

function fallbackLog(windowName: BootWindowName, stage: string, detail?: BootDetail): void {
  const payload = detail === undefined ? '' : detail;
  console.info(`[boot:${windowName}] ${stage}`, payload);
}

export function markBootStage(windowName: BootWindowName, stage: string, detail?: BootDetail): void {
  if (typeof window === 'undefined') {
    return;
  }

  if (typeof window.__NEW_STOCK_MARK_BOOT__ === 'function') {
    window.__NEW_STOCK_MARK_BOOT__(windowName, stage, detail);
    return;
  }

  fallbackLog(windowName, stage, detail);
}

export function installEntryDiagnostics(windowName: BootWindowName): void {
  markBootStage(windowName, 'entry-executed', {
    href: window.location.href,
    readyState: document.readyState,
  });
}
