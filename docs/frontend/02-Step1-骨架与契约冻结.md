# 前端 Step 1：骨架与契约冻结

## 1. 目标
本步骤只做前端骨架和 UI 基线冻结，不做真实业务实现。

完成后前端应达到：
- 主窗口和设置窗口骨架可见
- UI 与旧版设计基线一致
- Svelte 工程结构、stores、图表控制器和 Tauri 通信壳固定

### 1.1 当前仓库落地说明
- Step 1 冻结项已经落地到 `new_stock/frontend/`，后续 Step 2 到 Step 5 只能在这套骨架内补业务，不允许再回改布局、入口和契约命名
- 主窗口与设置窗口都必须继续保持独立入口，不能退化成“主窗口兼容一切”的单入口模式
- 白屏修复后的启动可观测性已经成为 Step 1 骨架的一部分，后续步骤不得把它删回静默失败

## 2. 必做项
- 建立 `Svelte 5 + TypeScript + Vite` 工程结构
- 建立主窗口入口和设置窗口入口
- `src/main.ts` 只负责调用 `mountMainWindow()`
- `src/settings.ts` 只负责调用 `mountSettingsWindow()`
- 建立以下目录：
  - `app/`
  - `components/`
  - `stores/`
  - `services/`
  - `charts/`
  - `windows/`
  - `styles/`
  - `types/`
- 建立以下 store：
  - `appStore`
  - `selectionStore`
  - `chartStore`
  - `syncStore`
  - `boardBuildStore`
  - `settingsStore`
  - `noteStore`

### 2.1 启动编排边界
- `src/app/` 只负责窗口初始化、listener 注册和 `bootstrap` 启动顺序
- 不允许把业务状态计算、表单保存逻辑或图表切换逻辑塞进 `src/app/`
- 业务状态流固定放在 `stores/` 与 `services/`

### 2.2 通信契约冻结要求
- `contracts + commands + events` 的名字必须与 `/Users/qr_luo/downloadtemp/new_stock/docs/01-项目开发需求文档.md` 中的固定契约保持一字不差
- 不允许在 Step 1 擅自改名
- 不允许先发明“前端专用 payload”再在后续步骤回填
- 必须先按 `/Users/qr_luo/downloadtemp/new_stock/docs/01-项目开发需求文档.md` 第 8 章固定契约建立前端类型和封装占位

### 2.3 启动可观测性基线
- 在最早入口阶段打启动标记，避免桌面壳层出现“窗口已起但内容区静默白屏”
- 主窗口和设置窗口都必须至少记录：
  - `mount-start`
  - `mount-complete`
  - `bootstrap-start`
  - `bootstrap-failed`
- 必须绑定 `window.onerror` 与 `unhandledrejection`
- 上述诊断能力属于 Step 1 骨架的一部分，后续步骤只能补充，不能删除

## 3. UI 冻结要求
- 完整继承 `/Users/qr_luo/downloadtemp/docs/04-UI规格附录.md`
- 完整继承 `/Users/qr_luo/downloadtemp/docs/03-前端开发文档.md`
- 不允许修改：
  - 顶部四大指数切换
  - 左侧板块列 + 中间成分股列 + 右侧主图区
  - 主图下方笔记区
  - 独立设置窗口
  - `等权 / 市值` 与 `日K / 周K` 控制关系

### 3.1 尺寸与比例冻结
- 静态骨架必须按旧版 UI 关键尺寸实现，不能只做“风格相似”
- 必须固定保留：
  - 默认窗口：`1440 x 920`
  - 最小窗口：`1280 x 800`
  - Toolbar：`52px`
  - IndexSwitcher：`44px`
  - StatusBar：`36px`
  - 左侧板块列：`132px`
  - 中间成分股列：`188px`
  - 左侧双列总宽：`320px`
  - 主图区最小宽度：`860px`
- 以上尺寸以 `/Users/qr_luo/downloadtemp/docs/04-UI规格附录.md` 为最终基线

## 4. 本步骤交付
- 静态主窗口
- 静态设置窗口
- 领域 stores 骨架
- `Lightweight Charts` 单实例控制器占位
- Tauri commands/events 前端封装占位

### 4.1 图表控制器接口冻结
- Step 1 不接真实图表业务
- 但图表控制器必须先把未来接口定出来，后续步骤不得推翻
- 至少保留以下接口：
  - `mount()`
  - `setData()`
  - `updateOverlay()`
- 控制器语义固定为“当前主图单实例占位实现”

## 5. 完成标准
- 主窗口和设置窗口都能独立挂载
- UI 骨架与旧版结构、尺寸和控件关系一致
- `stores` 已完成拆分，目录与命名不再变化
- `commands/events` 前端封装完成，命名与固定契约一致
- 图表控制器已是单实例占位实现
- 启动阶段保留 `mount / bootstrap / failed` 诊断日志，不再静默白屏
- `npm run check` 通过
- 桌面联调时至少用 `cargo run` 验证主窗口与设置窗口非白屏
- 完成后需要再做一轮基于 skill 的审查，至少覆盖：
  - 是否偷偷改了旧版布局
  - 是否仍存在整块重建风险
  - 是否存在会影响输入焦点稳定性的结构
- 通过 Step 1 后，后续步骤不再讨论 UI 布局和控件结构，只在既定骨架内填业务能力
