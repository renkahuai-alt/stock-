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
const mainWindowSource = readFileSync(
  resolve('/Users/qr_luo/downloadtemp/new_stock/frontend/src/windows/main/MainWindow.svelte'),
  'utf8',
);

assert(
  controllerSource.includes('DEFAULT_INITIAL_VISIBLE_BARS'),
  '图表控制器应定义默认首屏可见 K 线数量，用于最近 6 个月视口',
);
assert(
  controllerSource.includes('focusRecentBarsAndCaptureAnchor'),
  '图表控制器应提供最近 6 个月视口初始化逻辑',
);
assert(
  controllerSource.includes('setVisibleRange({'),
  '最近 6 个月默认视口应按真实时间范围设置，而不是继续依赖 logical range',
);
assert(
  controllerSource.includes('lockVisibleTimeRangeOnResize: true'),
  '时间轴在窗口放大后也应锁定当前时间范围，避免全屏后默认半年被拉成两年多',
);
assert(
  controllerSource.includes('setInitialVisibleBars'),
  '图表控制器应允许主窗口按日K/周K切换默认首屏可见 bars 数量',
);
assert(
  chartCanvasSource.includes('export let initialVisibleBars'),
  'ChartCanvas 应接收首屏可见 bars 配置，避免把周K也按 132 根展示',
);
assert(
  mainWindowSource.includes("initialVisibleBars={$selectionStore.granularity === 'week' ? 26 : 132}"),
  '主窗口应在日K默认 132 根、周K默认 26 根之间切换，保证两种粒度都接近最近半年',
);
assert(
  controllerSource.includes('handleScroll: {\n        mouseWheel: false,'),
  '滚轮应停止做滚动，避免与新的自由浏览模式冲突',
);
assert(
  controllerSource.includes('handleScale: {\n        axisPressedMouseMove: false,\n        mouseWheel: false,\n        pinch: false,'),
  '应关闭图表库原生缩放，改由前端接管等比例缩放，避免触控板手势把时间窗拉乱',
);

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}
