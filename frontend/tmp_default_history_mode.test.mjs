import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const mainWindowSource = readFileSync(
  resolve('/Users/qr_luo/downloadtemp/new_stock/frontend/src/windows/main/MainWindow.svelte'),
  'utf8',
);
const selectionStoreSource = readFileSync(
  resolve('/Users/qr_luo/downloadtemp/new_stock/frontend/src/stores/selectionStore.ts'),
  'utf8',
);
const noteEditorSource = readFileSync(
  resolve('/Users/qr_luo/downloadtemp/new_stock/frontend/src/components/TargetNoteEditor.svelte'),
  'utf8',
);
const cssSource = readFileSync(
  resolve('/Users/qr_luo/downloadtemp/new_stock/frontend/src/styles/app.css'),
  'utf8',
);

assert(
  !mainWindowSource.includes('RangeSwitcher'),
  '主窗口不应再渲染时间区间按钮',
);
assert(
  selectionStoreSource.includes("range: 'all'"),
  'selectionStore 默认图表请求应固定为 all',
);
assert(
  selectionStoreSource.includes("range: 'all',"),
  'toGetChartRequest 应统一按 all 请求完整历史',
);
assert(
  noteEditorSource.includes('note-editor__header'),
  '观点笔记应提供头部容器，把标题和保存按钮放在同一行',
);
assert(
  noteEditorSource.includes('note-editor__title'),
  '观点笔记标题应继续保留',
);
assert(
  noteEditorSource.includes('note-editor__button'),
  '观点笔记保存按钮应继续保留',
);
assert(
  readRuleBlock(cssSource, '.note-editor__header').includes('justify-content: space-between'),
  '观点笔记头部应使用左右分布，让保存按钮位于同一行右侧',
);
assert(
  readRuleBlock(cssSource, '.main-panel').includes('grid-template-rows: auto minmax(0, 1fr) clamp(160px, 24vh, 208px)'),
  '主面板删掉区间按钮后应收成三行布局，让观点笔记占据底部固定区域',
);

function readRuleBlock(source, selector) {
  const escapedSelector = selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const match = source.match(new RegExp(`${escapedSelector}\\s*\\{([\\s\\S]*?)\\}`, 'm'));

  if (!match) {
    throw new Error(`Missing rule for ${selector}`);
  }

  return match[1].replace(/\s+/g, ' ').trim();
}

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}
