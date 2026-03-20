let settingsWindowRef: Window | null = null;

export function openSettingsWindowFallback(): void {
  if (typeof window === 'undefined') {
    return;
  }

  if (settingsWindowRef && !settingsWindowRef.closed) {
    settingsWindowRef.focus();
    return;
  }

  const url = new URL('settings.html', window.location.href);
  settingsWindowRef = window.open(
    url.toString(),
    'new_stock_settings',
    'popup=yes,width=720,height=560,resizable=yes',
  );
  settingsWindowRef?.focus();
}

export function closeSettingsWindowFallback(): void {
  if (settingsWindowRef && !settingsWindowRef.closed) {
    settingsWindowRef.close();
    settingsWindowRef = null;
    return;
  }

  if (typeof window !== 'undefined' && window.location.pathname.endsWith('/settings.html')) {
    window.close();
  }
}
