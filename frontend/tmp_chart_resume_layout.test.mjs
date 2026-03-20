import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const controllerPath = resolve('/Users/qr_luo/downloadtemp/new_stock/frontend/src/charts/lightweightController.ts');
const canvasPath = resolve('/Users/qr_luo/downloadtemp/new_stock/frontend/src/components/ChartCanvas.svelte');

const controllerSource = readFileSync(controllerPath, 'utf8');
const canvasSource = readFileSync(canvasPath, 'utf8');

assert(
  controllerSource.includes('refreshLayout(): void'),
  'LightweightChartController 需要提供 refreshLayout()，用于窗口恢复后的显式重绘',
);
assert(
  controllerSource.includes('this.chart.resize('),
  'refreshLayout() 应显式调用 chart.resize(...)，避免切后台后尺寸未恢复',
);
assert(
  controllerSource.includes('this.refreshLayout();'),
  'setData() 在图表数据切换后应主动刷新布局，避免 ALL 等大范围切换后显示异常',
);
assert(
  controllerSource.includes('pendingInitialViewport'),
  '控制器应记录首屏视口待落位状态，避免首帧尺寸未稳定时把整段历史铺出来',
);
assert(
  controllerSource.includes('if (this.pendingInitialViewport && this.bars.length > 0)'),
  'refreshLayout() 应在首屏布局稳定后补一次最近半年视口，确保默认视口真正生效',
);
assert(
  canvasSource.includes("document.addEventListener('visibilitychange'"),
  'ChartCanvas 需要监听 visibilitychange，在窗口恢复时刷新布局',
);
assert(
  canvasSource.includes("window.addEventListener('focus'"),
  'ChartCanvas 需要监听 focus，在切回应用时刷新布局',
);
assert(
  canvasSource.includes("window.addEventListener('resize'"),
  'ChartCanvas 需要监听 resize，避免窗口尺寸变化后只渲染一部分',
);
assert(
  canvasSource.includes('controller.refreshLayout()'),
  'ChartCanvas 恢复逻辑应调用 controller.refreshLayout()',
);

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}
