import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const controllerSource = readFileSync(
  resolve('/Users/qr_luo/downloadtemp/new_stock/frontend/src/charts/lightweightController.ts'),
  'utf8',
);

assert(
  controllerSource.includes('resolveVisiblePriceScaleRange('),
  '图表控制器应通过独立的 visible price scale 解析器统一计算 y 轴范围，避免不同跟随状态下混用两套缩放策略',
);

assert(
  !controllerSource.includes('this.series.priceScale().setAutoScale(true);'),
  '贴着最新时也不应再把价格轴交回库的 autoscale，否则最新价线和盘中高点会把图形上方撑空',
);

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}
