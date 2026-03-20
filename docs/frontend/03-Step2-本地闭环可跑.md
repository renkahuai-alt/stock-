# 前端 Step 2：本地闭环可跑

## 1. 目标
本步骤要让前端在不依赖真实 Longbridge 的情况下，完整跑通本地主流程。

完成后前端应达到：
- 主窗口切图可用
- 设置窗口可输入和保存
- 笔记区可保存和恢复
- 板块创建链路可走真实前端状态流

### 1.1 当前仓库落地说明
- Step 2 的“可写本地样例数据源”能力保留为显式本地回归 / 演示路径，不再允许混入 Step 3 之后的真实主路径
- QA / release 口径下不得依赖本地 fallback 伪装真实成功；正式联调时必须显式记录 `VITE_ENABLE_LOCAL_FALLBACK` 的值
- Step 2 产物的核心价值是验证前端状态流和输入稳定性，而不是为后续步骤保留一条长期并行的数据口径

## 2. 必做项
- 接入 `bootstrap`
- 接入 `get_chart`
- 接入 `save_board`
- 接入 `save_credentials`
- 接入 `get_target_note`
- 接入 `save_target_note`
- 接入 `open_settings_window`
- 接入 `close_settings_window`

实现方式固定为：
- `app/` 继续只做启动、窗口初始化和 listener 注册
- 业务闭环放进 `stores/ + services/`
- `commands.ts` 保持真实命令名不变，并继续作为唯一入口
- fallback 不再是纯静态值，升级为“可写的本地样例数据源”
- 本地样例数据源只允许保留为显式 fallback / regression path，不允许再混入 Step 3+ 的默认主路径

禁止改成：
- 组件内直接持有业务主状态
- 组件直接拼装本地业务数据
- 为了 Step 2 再额外发明一层 mock bridge

## 3. 页面联通要求
- 顶部指数点击可切图
- 板块点击可切图
- 成分股点击可切图
- 日K / 周K 切换可驱动请求参数
- 等权 / 市值切换可驱动请求参数
- 笔记保存后当前输入不丢焦点

Store 职责固定为：
- `appStore`
  - 作为主窗口基础数据的单一事实源
  - 负责板块列表、成分股映射、当前目标基础摘要
- `selectionStore + chartStore`
  - 负责切图闭环
  - 指数 / 板块 / 个股、日K / 周K、等权 / 市值都只改变请求参数
  - 只更新图表数据，不重建图表实例
- `settingsStore`
  - 负责设置页输入、保存态和反馈态
  - 保存统一走 `save_credentials`
- `noteStore`
  - 负责笔记读取、保存和恢复
  - 保存后只更新当前 note，不替换输入节点

`save_board` 在 Step 2 的本地闭环要求：
- 先通过真实 `save_board()` 封装路径返回结果
- fallback 数据源必须可写
- 能新增/编辑板块
- 能同步推动：
  - `appStore`
  - `selectionStore`
  - `chartStore`
- 返回结构必须完整复刻正式契约：
  - `boardId`
  - `rebuildStarted`
  - `backgroundSyncStarted`
  - `buildStatus`
  - `buildPhase`
  - `buildJobId`
  - `updatedAt`
  - `compositionAlgorithm`

## 4. 禁止项
- 不允许整页重渲染
- 不允许图表实例重建
- 不允许设置窗口输入被主窗口状态更新打断
- 不允许把更多业务逻辑塞进组件，回到“组件管业务”的老路
- 不允许让 `commands.ts` 自己成为可变状态中心
- 不允许为了省事简化 `save_board()` 的返回结构

图表实现硬性要求：
- `ChartCanvas` 只允许 `mount()` 一次
- 切图只走 `setData()`
- 切范围、粒度、板块算法也只走数据更新
- Step 2 就必须坚持单实例，不能等 Step 3 再修

## 5. 本步骤交付
- 主窗口本地闭环
- 设置窗口本地闭环
- 笔记区本地闭环
- 前端通过真实 command 路径获取本地样例数据
- 可写本地样例数据源
- `save_board` 的新增/编辑板块闭环

## 6. 完成标准
- 无需真实 Longbridge 即可完整演示主流程
- 输入稳定
- 图表切换稳定
- 新建板块后，列表、选择态和主图同步更新
- 设置保存后输入不丢焦点
- 笔记保存后切目标再切回可恢复
- 必须能明确区分“本地闭环”与“真实主路径”，不能 silently fallback

## 7. 检查与验证
开发完成后至少执行：
- 最小可执行检查
- `npm run check`
- `npm run build`
- 一轮以结构和风险为主的前端审查
- 记录验收时 `VITE_ENABLE_LOCAL_FALLBACK` 的值

审查重点固定为：
- 是否偷偷改了旧版布局
- 是否还存在整块重建风险
- 是否有会影响输入焦点的结构
- 图表是否仍然保持单实例
