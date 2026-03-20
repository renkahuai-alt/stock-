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
  controllerSource.includes('applyProportionalZoom(deltaY: number, clientX?: number): void'),
  '图表控制器应暴露受控的等比例缩放入口，统一接住鼠标滚轮和触控板双指缩放',
);
assert(
  controllerSource.includes('getVisibleLogicalRange()'),
  '等比例缩放应基于当前可见逻辑范围计算，而不是让时间窗随手势失控漂移',
);
assert(
  controllerSource.includes('Math.exp('),
  '等比例缩放应使用连续比例因子，而不是离散跳变',
);
assert(
  chartCanvasSource.includes("chartHostEl.addEventListener('wheel'"),
  'ChartCanvas 应接管 wheel 事件，把缩放交给受控逻辑',
);
assert(
  chartCanvasSource.includes('controller.applyProportionalZoom(event.deltaY, event.clientX)'),
  'wheel 事件应调用控制器的等比例缩放，而不是继续依赖图表库默认行为',
);
assert(
  chartCanvasSource.includes('event.preventDefault()'),
  '接管 wheel 后应阻止默认事件，避免浏览器/图表库再做第二次缩放',
);

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}
