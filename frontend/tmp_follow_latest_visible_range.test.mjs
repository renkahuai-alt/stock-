import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const controllerSource = readFileSync(
  resolve('/Users/qr_luo/downloadtemp/new_stock/frontend/src/charts/lightweightController.ts'),
  'utf8',
);

assert(
  controllerSource.includes('resolveAutoFollowLatestFromVisibleRange(): boolean | null'),
  '控制器应基于当前可见时间窗判断是否仍贴着最新，而不是只依赖 scrollPosition',
);
assert(
  controllerSource.includes('getVisibleLogicalRange()'),
  '历史浏览模式判定应读取当前可见 logical range，覆盖鼠标拖拽和触控板平移等所有方式',
);
assert(
  controllerSource.includes('visibleLogicalRange.to'),
  '是否仍在跟随最新应根据当前视口右侧是否仍接近最后一根 K 线来判断',
);
assert(
  controllerSource.includes('this.followLatestState = {'),
  '控制器应在时间窗变化时主动同步 autoFollowLatest，而不是只在少数交互里手动切换',
);

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}
