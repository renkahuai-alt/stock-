import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const helperSource = readFileSync(
  resolve('/Users/qr_luo/downloadtemp/new_stock/frontend/src/charts/visiblePriceScale.ts'),
  'utf8',
);

assert(
  helperSource.includes('export function resolveVisiblePriceScaleRange('),
  '应提供独立的 visible price scale 解析器，统一控制当前可见 K 线的 y 轴范围',
);
assert(
  helperSource.includes('const fromIndex = clamp(Math.floor(visibleLogicalRange.from), 0, bars.length - 1);'),
  '价格轴范围应按当前可见逻辑区间裁切到实际 bars，避免离屏数据参与缩放',
);
assert(
  helperSource.includes('const priceSpan = Math.max(maxPrice - minPrice, Math.max(Math.abs(maxPrice), 1) * 0.001);'),
  '价格轴解析器应为窄波动区间保留最小 span，避免几乎横线时出现退化缩放',
);
assert(
  helperSource.includes('const padding = priceSpan * paddingRatio;'),
  '价格轴解析器应保留可调 padding，而不是把库默认 autoscale 当成黑盒',
);

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}
