import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const viewportStorePath = resolve('/Users/qr_luo/downloadtemp/new_stock/frontend/src/stores/chartViewportStore.ts');
const viewportStoreSource = existsSync(viewportStorePath)
  ? readFileSync(viewportStorePath, 'utf8')
  : '';
const controllerSource = readFileSync(
  resolve('/Users/qr_luo/downloadtemp/new_stock/frontend/src/charts/lightweightController.ts'),
  'utf8',
);
const chartStoreSource = readFileSync(
  resolve('/Users/qr_luo/downloadtemp/new_stock/frontend/src/stores/chartStore.ts'),
  'utf8',
);

assert(
  viewportStoreSource.includes('autoFollowLatest'),
  '应新增图表视口运行时 store，显式记录当前是否仍在跟随最新',
);
assert(
  controllerSource.includes('setChartViewportAutoFollowLatest'),
  '图表控制器应把 autoFollowLatest 状态同步出去，供业务层判断是否冻结盘中 overlay',
);
assert(
  chartStoreSource.includes('deferredLiveUpdate'),
  '图表 store 应缓存历史浏览期间收到的最新盘中 overlay，而不是直接改当前图',
);
assert(
  chartStoreSource.includes('get(chartViewportStore).autoFollowLatest'),
  '盘中 live update 入库前应先判断当前是否仍在跟随最新',
);
assert(
  chartStoreSource.includes('chartViewportStore.subscribe'),
  '回到最新位置时，图表 store 应恢复并补上缓存的 live overlay',
);

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}
