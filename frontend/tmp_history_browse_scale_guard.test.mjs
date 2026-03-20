import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const controllerSource = readFileSync(
  resolve('/Users/qr_luo/downloadtemp/new_stock/frontend/src/charts/lightweightController.ts'),
  'utf8',
);

assert(
  controllerSource.includes('syncFollowLatestPresentation(): void'),
  '图表控制器应根据是否跟随最新切换主图呈现，避免历史浏览时仍显示当前最新价的视觉锚点',
);
assert(
  controllerSource.includes('priceLineVisible: this.followLatestState.autoFollowLatest'),
  '历史浏览模式下应关闭主图 price line，避免最新价把旧历史的价格轴顶高',
);
assert(
  controllerSource.includes('lastValueVisible: this.followLatestState.autoFollowLatest'),
  '历史浏览模式下应关闭 last value，避免离屏最新值继续影响旧历史阅读',
);
assert(
  controllerSource.includes('this.syncFollowLatestPresentation();'),
  '控制器在跟随最新状态变化后应立即同步 series 展示策略',
);
assert(
  controllerSource.includes('syncVisiblePriceScale(): void'),
  '控制器应在时间窗变化后显式同步当前可见价格轴，而不是完全依赖库的自动行为',
);
assert(
  controllerSource.includes('this.series.priceScale().setVisibleRange(visiblePriceScaleRange);'),
  '历史浏览模式下应按当前可见 K 线手动设定价格轴范围，避免旧历史被压扁',
);
assert(
  controllerSource.includes('resolveVisiblePriceScaleRange('),
  '贴着最新与历史浏览都应复用同一套 visible price scale 解析逻辑，避免两套 y 轴策略互相打架',
);

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}
