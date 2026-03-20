export function resolveMountTarget(windowName: string): HTMLElement {
  const target = document.getElementById('app');

  if (!target) {
    throw new Error(`${windowName} mount point not found`);
  }

  return target;
}

export function registerWindowCleanup(cleanup: () => void): void {
  window.addEventListener('beforeunload', cleanup, { once: true });
}
