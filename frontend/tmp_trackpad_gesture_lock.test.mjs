import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const chartCanvasSource = readFileSync(
  resolve('/Users/qr_luo/downloadtemp/new_stock/frontend/src/components/ChartCanvas.svelte'),
  'utf8',
);

assert(
  chartCanvasSource.includes('WHEEL_AXIS_DOMINANCE_RATIO'),
  'ChartCanvas 应通过轴向主导阈值判断触控板手势意图，避免轻微斜向噪声把平移误判成缩放',
);
assert(
  chartCanvasSource.includes('activeWheelIntent'),
  'ChartCanvas 应在一次连续手势期间锁定当前意图，避免中途在缩放和平移之间来回跳',
);
assert(
  chartCanvasSource.includes('WHEEL_GESTURE_LOCK_MS'),
  'ChartCanvas 应为触控板手势设置短暂锁定窗口，等一段手势结束后再重新判定下一次意图',
);
assert(
  chartCanvasSource.includes('event.ctrlKey'),
  'ChartCanvas 应保留对系统级缩放手势的明确识别，避免真正的 pinch 被误判成普通滑动',
);

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}
