# new_stock

`new_stock/` 是当前项目的重构版工作区。

目标：
- 保留旧版产品行为、视觉风格和数据口径
- 重构为 `Tauri 2 + Svelte 5 + TypeScript + Rust + SQLite`
- 优先解决旧版在真实 Longbridge 数据量下的卡顿、阻塞和重渲染问题
- 完整保留旧项目已经定稿的 UI 设计，不做重新设计

实施文档：
- `docs/01-项目开发需求文档.md`
- `docs/frontend/01-前端实施需求文档.md`
- `docs/backend/01-后端实施需求文档.md`
- `docs/frontend/02-Step1-骨架与契约冻结.md`
- `docs/frontend/03-Step2-本地闭环可跑.md`
- `docs/frontend/04-Step3-真实数据与板块构建.md`
- `docs/frontend/05-Step4-盘中最新日K.md`
- `docs/frontend/06-Step5-性能稳定性与验收.md`
- `docs/backend/02-Step1-骨架与契约冻结.md`
- `docs/backend/03-Step2-本地闭环可跑.md`
- `docs/backend/04-Step3-真实数据与板块构建.md`
- `docs/backend/05-Step4-盘中最新日K.md`
- `docs/backend/06-Step5-性能稳定性与验收.md`

说明：
- 旧项目代码不复用运行时实现，只作为行为、UI 和测试口径对照组
- 新项目首版平台固定为 `macOS`
- 前端与后端职责已经在需求文档中拆开，实施时不得跨边界扩展
