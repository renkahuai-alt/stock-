import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const cssPath = resolve('/Users/qr_luo/downloadtemp/new_stock/frontend/src/styles/app.css');
const css = readFileSync(cssPath, 'utf8');

expectRule('.window-shell', ['height: 100%', 'overflow: hidden']);
expectRule('.workspace-shell', ['height: 100%', 'overflow: hidden']);
expectRule('.rail', ['min-height: 0', 'overflow: auto']);
expectRule('.main-panel', [
  'display: grid',
  'grid-template-rows: auto minmax(0, 1fr) auto clamp(160px, 24vh, 208px)',
  'overflow: hidden',
]);
expectRule('.chart-canvas', ['min-height: 0', 'height: 100%']);
expectRule('.note-editor', [
  'display: grid',
  'grid-template-rows: auto minmax(0, 1fr) auto',
  'min-height: 0',
  'overflow: hidden',
]);
expectRule('.note-editor__input', ['min-height: 0', 'height: 100%', 'resize: none', 'overflow: auto']);

function expectRule(selector, declarations) {
  const block = readRuleBlock(selector);

  declarations.forEach((declaration) => {
    if (!block.includes(declaration)) {
      throw new Error(`Expected ${selector} to include "${declaration}"`);
    }
  });
}

function readRuleBlock(selector) {
  const escapedSelector = selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const match = css.match(new RegExp(`${escapedSelector}\\s*\\{([\\s\\S]*?)\\}`, 'm'));

  if (!match) {
    throw new Error(`Missing rule for ${selector}`);
  }

  return normalize(match[1]);
}

function normalize(value) {
  return value.replace(/\s+/g, ' ').trim();
}
