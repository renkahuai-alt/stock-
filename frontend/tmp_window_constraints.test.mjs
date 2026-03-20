import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const cssPath = resolve('/Users/qr_luo/downloadtemp/new_stock/frontend/src/styles/app.css');
const tauriConfigPath = resolve('/Users/qr_luo/downloadtemp/new_stock/src-tauri/tauri.conf.json');

const css = readFileSync(cssPath, 'utf8');
const tauriConfig = JSON.parse(readFileSync(tauriConfigPath, 'utf8'));
const mainWindow = tauriConfig.app.windows.find((window) => window.label === 'main');

if (!mainWindow) {
  throw new Error('tauri.conf.json 缺少 main window 配置');
}

const windowShellBlock = readRuleBlock(css, '.window-shell');
const workspaceShellBlock = readRuleBlock(css, '.workspace-shell');
const rootBlock = readRuleBlock(css, 'html,\nbody,\n#app');

assert(
  !windowShellBlock.includes('min-width: 1280px'),
  'window-shell 不能继续写死 min-width: 1280px，否则小屏幕会直接被撑爆',
);
assert(
  windowShellBlock.includes('position: fixed'),
  'window-shell 应固定在视口内，避免 range 点击后根页面滚动把左侧区域挤出视口',
);
assert(
  windowShellBlock.includes('inset: 0'),
  'window-shell 应使用 inset: 0 锁定桌面窗口内容区域',
);
assert(
  !windowShellBlock.includes('min-height: 800px'),
  'window-shell 不能继续写死 min-height: 800px，否则较矮窗口会直接把内容顶出视口',
);
assert(
  workspaceShellBlock.includes('minmax(0, 1fr)'),
  'workspace-shell 主图区列必须允许压缩，否则会把左侧内容挤出可视区',
);
assert(
  rootBlock.includes('width: 100%'),
  'html/body/#app 应锁定 width: 100%，避免 range 按钮触发根容器横向滚动',
);
assert(
  rootBlock.includes('overflow: hidden'),
  'html/body/#app 应禁掉根容器滚动，否则点击 ALL 后可能把左侧区域滚出视口',
);
assert(
  typeof mainWindow.width === 'number' && mainWindow.width <= 1320,
  `main window 初始宽度应收敛到 1320 以内，当前为 ${mainWindow.width}`,
);
assert(
  typeof mainWindow.minWidth === 'number' && mainWindow.minWidth <= 960,
  `main window 最小宽度应降到 960 以内，当前为 ${mainWindow.minWidth}`,
);
assert(
  typeof mainWindow.minHeight === 'number' && mainWindow.minHeight <= 720,
  `main window 最小高度应降到 720 以内，当前为 ${mainWindow.minHeight}`,
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
