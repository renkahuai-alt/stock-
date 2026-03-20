async function main(): Promise<void> {
  const mod = await import('./src/charts/viewportPolicy');

  expect(
    typeof mod.isViewportFollowingLatest,
    (value) => value === 'function',
    'isViewportFollowingLatest 应存在',
  );
  expect(
    typeof mod.resolveViewportUpdateStrategy,
    (value) => value === 'function',
    'resolveViewportUpdateStrategy 应存在',
  );

  const sameTargetLatest = mod.resolveViewportUpdateStrategy({
    previousViewportKey: 'symbol:NVDA:day:1y',
    nextViewportKey: 'symbol:NVDA:day:1y',
    isFollowingLatest: true,
    lockedVisibleRange: { from: 180, to: 240 },
    nextBarCount: 241,
  });

  expectDeepEqual(
    sameTargetLatest,
    {
      intent: 'follow-latest',
      visibleRange: null,
      resetFollowLatest: false,
    },
    '同一目标且仍在最新位置时，应继续跟随最新',
  );

  const sameTargetManualPan = mod.resolveViewportUpdateStrategy({
    previousViewportKey: 'symbol:NVDA:day:1y',
    nextViewportKey: 'symbol:NVDA:day:1y',
    isFollowingLatest: false,
    lockedVisibleRange: { from: 96, to: 156 },
    nextBarCount: 241,
  });

  expectDeepEqual(
    sameTargetManualPan,
    {
      intent: 'preserve-visible-range',
      visibleRange: { from: 96, to: 156 },
      resetFollowLatest: false,
    },
    '用户手动滑动后，同一目标更新应保持用户停下时的位置',
  );

  const changedTarget = mod.resolveViewportUpdateStrategy({
    previousViewportKey: 'symbol:NVDA:day:1y',
    nextViewportKey: 'index:DJI:day:1y',
    isFollowingLatest: false,
    lockedVisibleRange: { from: 96, to: 156 },
    nextBarCount: 241,
  });

  expectDeepEqual(
    changedTarget,
    {
      intent: 'follow-latest',
      visibleRange: null,
      resetFollowLatest: true,
    },
    '切换目标后应重置为跟随最新，而不是沿用旧视口',
  );

  expect(
    mod.isViewportFollowingLatest({ from: 180, to: 244.2 }, 241),
    (value) => value === true,
    '右侧仍覆盖最后一根 K 线时，应判定为跟随最新',
  );

  expect(
    mod.isViewportFollowingLatest({ from: 120, to: 238.2 }, 241),
    (value) => value === false,
    '用户拖离最后一根 K 线后，应停止自动跟随',
  );
}

interface ResolvedViewportStrategy {
  intent: 'follow-latest' | 'preserve-visible-range';
  visibleRange: { from: number; to: number } | null;
  resetFollowLatest: boolean;
}

function expect<T>(value: T, predicate: (value: T) => boolean, message: string): void {
  if (!predicate(value)) {
    throw new Error(message);
  }
}

function expectDeepEqual(actual: unknown, expected: ResolvedViewportStrategy, message: string): void {
  const actualText = JSON.stringify(actual);
  const expectedText = JSON.stringify(expected);

  if (actualText !== expectedText) {
    throw new Error(`${message}\nexpected: ${expectedText}\nactual:   ${actualText}`);
  }
}

void main();
