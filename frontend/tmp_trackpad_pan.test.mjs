import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const controllerSource = readFileSync(
  resolve('/Users/qr_luo/downloadtemp/new_stock/frontend/src/charts/lightweightController.ts'),
  'utf8',
);
const chartCanvasSource = readFileSync(
  resolve('/Users/qr_luo/downloadtemp/new_stock/frontend/src/components/ChartCanvas.svelte'),
  'utf8',
);

assert(
  controllerSource.includes('applyHorizontalPan(deltaX: number): void'),
  '图表控制器应暴露受控的横向平移入口，恢复触控板左右滑动查看更早 K 线',
);
assert(
  controllerSource.includes('setVisibleLogicalRange({ from, to })'),
  '横向平移应基于逻辑范围整体平移，而不是退回图表库默认 wheel 行为',
);
assert(
  chartCanvasSource.includes('Math.abs(event.deltaX) > Math.abs(event.deltaY)'),
  'ChartCanvas 应区分触控板左右平移和上下缩放，避免把所有 wheel 都当成缩放',
);
assert(
  chartCanvasSource.includes('controller.applyHorizontalPan(event.deltaX)'),
  '当横向手势更强时，应调用受控的横向平移逻辑浏览历史',
);

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}
