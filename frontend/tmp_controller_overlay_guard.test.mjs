import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const controllerSource = readFileSync(
  resolve('/Users/qr_luo/downloadtemp/new_stock/frontend/src/charts/lightweightController.ts'),
  'utf8',
);

assert(
  controllerSource.includes("private deferredOverlayBar: ActiveOverlayPayload['bar'] | null = null;"),
  '图表控制器应缓存非跟随最新阶段收到的 live overlay，作为最后一道保险',
);
assert(
  controllerSource.includes('if (!this.followLatestState.autoFollowLatest) {'),
  '图表控制器在历史浏览模式下应拒绝直接把 live overlay 画进主图',
);
assert(
  controllerSource.includes('this.deferredOverlayBar = bar;'),
  '历史浏览时收到的 overlay 应先缓存，避免把当前视口压扁',
);
assert(
  controllerSource.includes('flushDeferredOverlayIfNeeded(): void'),
  '回到最新位置时，控制器应补上之前冻结的 overlay',
);
assert(
  controllerSource.includes('this.flushDeferredOverlayIfNeeded();'),
  '控制器在恢复跟随最新时应尝试冲刷冻结的 overlay',
);

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}
