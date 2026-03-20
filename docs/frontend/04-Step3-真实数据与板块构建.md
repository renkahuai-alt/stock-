# 前端 Step 3：真实数据与板块构建

## 1. 目标
本步骤把前端从“本地样例闭环”切到“真实后端数据 + 板块异步构建”，并保证在真实数据量上来后仍然保持局部更新、输入稳定和图表单实例。

Step 3 的重点不是“把接口接通”本身，而是把以下三件事同时处理对：
- 真实图表数据接入
- 板块后台构建状态接入
- 事件与查询并存时的状态一致性

当前说明：
- 本文档描述的是 Step 3 的完成态要求，不等同于当前代码已全部达到
- 若代码仍保留 fallback / mock 双通路、设置窗口复用主窗口 bootstrap、或快路径跨窗口不同步，则 Step 3 只能视为进行中，不能宣称完成

### 1.1 当前仓库落地口径
- 主窗口冷启动允许走一次完整 `syncBootstrapState()`；设置窗口保存成功后的主窗口补偿只允许走目录级 `refreshBootstrapCatalogState()`，不得无条件重放整套 `bootstrap`
- `boardBuildStore` 是板块构建状态的唯一事实源，`BoardList / MainWindow / ChartHeader` 都必须基于同一份构建态驱动
- 构建状态固定按 `buildJobId + updatedAt` 做新旧判断；旧 job 或更早时间戳不得覆盖当前状态
- 当前激活目标是板块且收到当前 `buildJobId = succeeded` 时，主图区才允许自动刷新；其他情况只能推进目录、行状态和状态栏
- 设置窗口初始化必须保持 `settings-only`，不能再借“拿目录数据”顺手改写主窗口的 `selection / chart / appStore`

## 2. 必做项
- 去掉主路径上的 mock 依赖
- 接入真实 `get_sync_status`
- 接入真实 `get_board_build_status`
- 接入 `board-build-status` event
- 接入真实 `save_board()` 双路径返回
- 接入真实 `get_chart()` 空态、错误态和构建中态

必须额外满足：
- `commands` 主路径不得静默 fallback 到本地样例数据
- 主窗口不得继续注册本地 fallback 同步通路
- 设置窗口不得再通过主窗口式 `bootstrap -> applyBootstrapPayload()` 改写全局应用状态
- 快路径新建 / 编辑板块后，主窗口必须能在无后台构建事件时也收到目录更新

## 3. 实施前提与顺序
Step 3 不是“只改前端就能联通”的步骤。

前端开工前，至少需要后端先补齐最小真链路：
- `bootstrap`
- `save_board`
- `get_board_build_status`
- `get_chart`

若这 4 条链路仍然是 scaffold / 样例返回，前端只能先做结构整改，不能宣称 Step 3 完成。

推荐前端固定顺序：
1. 先统一 `buildStatus / buildPhase / updatedAt / buildJobId` 契约
2. 再改 `boardBuildStore`
3. 再改 `chartStore`
4. 再改 `mainFlow / settingsFlow`
5. 最后接 `board-build-status` 事件优先与冷启动补查

## 4. 本步骤完成标准
完成后前端必须达到：
- 创建小板块时可走快路径并立即出图
- 创建大板块时立即进入“构建中”态，不阻塞界面
- 构建完成后，当前选中板块可自动刷新图表
- 构建失败后，当前板块行和主图区都能看到失败反馈
- 在构建期间切到别的指数、板块或个股，不会被后台完成事件误切回
- 设置窗口、笔记区、板块输入框在后台状态变化时不丢焦点

同时必须满足以下收口条件：
- 主路径所有真实命令失败时显式报错，不允许自动切回 fallback 样例结果
- 主窗口数据初始化、状态推进、跨窗口同步都基于真实命令与真实事件
- 设置窗口初始化只同步 settings 所需数据，不触发主窗口状态树重放
- `get_sync_status()` 已进入主流程，而不是只有封装存在、主流程仍主要依赖 `bootstrap.syncStatus + event`

未满足任一项，都不能标记 Step 3 完成

## 5. 状态来源与优先级
Step 3 前端会同时拿到两类板块构建状态：

1. 查询结果
- `save_board()` 返回
- `get_board_build_status()` 返回

2. 事件结果
- `board-build-status`

前端必须明确优先级与去重策略，禁止“谁先到就覆盖谁”。

### 5.1 单一事实源
板块构建状态只能由 `boardBuildStore` 维护。

禁止：
- `App.svelte` 自己维护一份
- `BoardList` 内部再维护一份
- `ChartPanel` 再派生一份“临时状态”且不回写 store

### 5.2 去重字段
后端状态进入前端时，前端必须至少基于以下字段做去重与新旧判断：
- `boardId`
- `buildJobId`
- `updatedAt`
- `buildStatus`
- `buildPhase`

若收到旧 `jobId` 或更早 `updatedAt` 的事件，必须丢弃。

### 5.3 查询与事件的职责
- 查询用于冷启动恢复、页面首次进入、事件丢失后的补偿
- 事件用于实时推进当前构建进度
- 事件是主通路，查询是兜底通路

禁止：
- 每次收到事件后立刻再次全量查询
- 把轮询当主方案，把事件当装饰
- 用 fallback 本地事件去弥补真实桥接链路未接通的问题

### 5.4 板块成员权重显示规则
板块左侧成员列表的权重显示必须与当前算法一致。

固定规则：
- `bootstrap().membersByBoard` 只表示板块默认算法快照
- 当前选中目标是 `board` 且用户切换 `boardAlgorithm` 时：
  - 不允许继续复用旧的 `membersByBoard[boardId]`
  - 必须调用 `get_board_member_summaries({ boardId, compositionAlgorithm })`
  - 必须按 `boardId + compositionAlgorithm` 维度缓存成员权重结果
- 切换算法时，图表和成员权重必须一起推进，不允许只切图不切左侧比例

禁止：
- 通过重新 `bootstrap()` 刷新成员权重
- 在前端自己做等权 / 市值权重计算
- 只按 `boardId` 维度缓存成员权重，导致算法切换后继续显示旧结果

## 6. `save_board()` 双路径前端规则
### 6.1 快路径
当返回：
- `backgroundSyncStarted = false`

前端必须：
- 立即更新板块列表
- 立即把该板块写入 `appStore`
- 若当前用户动作是“创建后进入该板块”，则切换到该板块
- 立即请求 `get_chart()` 并刷新主图
- 明确把板块目录更新同步到主窗口，不能只更新当前设置窗口本地 store

说明：
- 快路径不能假设后端一定会补发 `board-build-status`
- 若主窗口只监听后台构建事件，则快路径会出现“设置窗口已看到新板块、主窗口目录没更新”的裂缝，这不符合 Step 3 完成标准

### 6.2 后台路径
当返回：
- `backgroundSyncStarted = true`

前端必须：
- 立即更新板块列表
- 把该板块写为“构建中”
- 主图区进入“构建中空态”或“保留旧图 + 构建提示”
- 不等待完整图表返回
- 不阻塞其它交互

实现要求：
- 后台路径禁止直接调用“全量 `bootstrap + loadChart`”式重同步
- 后台路径必须先写入 `boardBuildStore`
- 后台路径是否拉图，必须由当前板块状态决定，而不是固定 `loadChart()`

禁止：
- 因为没拿到完整图表而让创建动作一直 pending
- 在后台路径里反复重试 `save_board()`
- 把后台路径误处理成快路径

## 7. 板块构建状态机
前端只允许识别以下 `buildStatus`：
- `queued`
- `running`
- `succeeded`
- `failed`

前端只允许识别以下 `buildPhase`：
- `queued`
- `fetching_symbols`
- `fetching_history`
- `recomputing_board`
- `persisting`
- `completed`
- `failed`

说明：
- 若后端后续扩展 phase，前端必须有兜底显示，不得崩溃。
- 未识别 phase 统一显示为“处理中”，但不得覆盖已知 `buildStatus`。

## 8. 局部更新规则
后台构建状态变化时，前端只允许更新以下区域：
- 当前板块行状态
- 主图区构建态
- 状态栏摘要

禁止更新以下区域：
- 设置窗口表单
- 笔记输入框 DOM 节点
- 非当前板块的列表行
- 图表挂载节点
- 整页容器

补充：
- 设置窗口与主窗口之间的同步，也必须遵守局部更新原则
- 不允许因为设置页保存成功、构建状态补查、或同步状态刷新而触发主窗口整棵状态树重放

## 9. 图表刷新规则
### 9.1 单实例原则
`ChartCanvas` 在 Step 3 仍然必须保持：
- 只 `mount()` 一次
- 只保留一套图表实例
- 数据更新只走 `setData()` / `update()`

### 9.2 自动刷新条件
仅当以下条件同时满足时，构建完成后允许自动刷新主图：
- 当前激活目标是该 `boardId`
- 收到的状态是当前 `buildJobId`
- `buildStatus = succeeded`

如果用户已经切走：
- 只更新板块行状态
- 不自动打断当前主图

### 9.3 构建中显示策略
前端必须选定一种并固定，不允许实现过程中摇摆：

推荐策略：
- 若当前板块已有旧图，则保留旧图
- 在图表标题区或空态区显示“正在更新板块数据”
- 若当前板块从未有图，则显示构建中空态

## 10. 常见时序坑与防护
### 10.1 保存后立即切图
风险：
- `save_board()` 返回后立刻切图，但后端真实状态还没落稳，导致先看到旧图或空图

要求：
- 快路径可立即拉图
- 后台路径先进入构建态，不强拉完整图
- 禁止把“保存板块”实现成无条件 `syncBootstrapState()`

### 10.2 旧事件覆盖新状态
风险：
- 板块重新编辑后，新旧两次构建事件交错

要求：
- 必须按 `buildJobId + updatedAt` 去重
- 旧任务事件不得回写当前任务状态

### 10.3 用户中途切换目标
风险：
- 用户已切到指数或个股，后台完成事件回来后误刷新主图区

要求：
- 自动刷新必须绑定“当前选中目标仍是该板块”

### 10.4 冷启动恢复
风险：
- 应用重启后，板块仍在构建，但前端不知道

要求：
- `bootstrap()` 后若发现有 `queued / running` 板块，前端必须补查 `get_board_build_status()`
- 主窗口必须通过真实查询补查，不允许继续依赖本地 fallback sync

### 10.5 设置窗口误走主窗口链路
风险：
- 设置窗口挂载时复用主窗口 `bootstrap -> selection sync -> loadChart`，会额外触发图表请求和主窗口刷新

要求：
- 设置窗口必须使用 `settings-only` 初始化
- 不触发主图 `loadChart`
- 不触发主窗口整套 bootstrap 重跑

补充说明：
- 即使设置窗口已经不主动拉主图，只要仍执行 `bootstrap -> applyBootstrapPayload()` 并写全局 store，也仍未达到本条要求

### 10.6 主路径静默 fallback
风险：
- 真实命令失败后前端无感切回本地样例数据，导致联调、回归、构建时序判断全部失真

要求：
- Step 3 主路径必须关闭静默 fallback
- 仅允许在显式开发模式或独立演示模式下启用 mock / fallback
- 真实主路径失败时必须可观测，可显示错误，可进入兜底 UI，但不能伪装成真实成功

### 10.7 快路径跨窗口不同步
风险：
- 设置窗口快路径保存成功后，仅当前窗口本地 upsert，主窗口目录不刷新

要求：
- 必须提供快路径的跨窗口同步机制
- Step 3 在此处只允许一种实现方式：
  - 设置窗口 `save_board()` 成功后，由后端统一发出 `settings-saved`
  - 主窗口收到 `settings-saved` 后，统一执行一次“目录级补查”
  - 目录级补查只允许刷新 `boards / membersByBoard / syncStatus`
  - 目录级补查不得触发主图 `loadChart`
  - 目录级补查不得重放主窗口整套 `bootstrap` 状态树
- 不允许新增专用快路径事件
- 不允许依赖共享可变 store 做跨窗口同步
- 不允许仅依赖后台构建事件来覆盖快路径目录联动
## 11. 错误与空态处理
Step 3 必须区分以下情况，不能统一显示“加载失败”：
- `get_chart()` 空数据
- `save_board()` 后台构建中
- 板块构建失败
- 后端限频或远端不可用
- 查询构建状态失败但本地已有旧图

推荐 UI 语义：
- 有旧图时优先保留旧图，再给轻量错误提示
- 无旧图时再显示完整空态 / 错误态

## 12. Store 边界
Step 3 组件职责不能回退。

建议固定：
- `appStore`
  - 板块基础信息
  - 成分股映射
  - 当前目标摘要
- `selectionStore`
  - 当前 target / granularity / range / algorithm
- `chartStore`
  - 图表请求、加载态、构建态、空态、错误态
  - `lastGoodPayload`
  - 当前有效 payload
- `boardBuildStore`
  - 板块构建状态、进度、事件去重
  - 冷启动补查结果
  - 自动刷新判定辅助信息
- `syncStore`
  - 同步状态摘要
  - 主动 `get_sync_status()` 查询结果
  - 事件与查询合并后的当前有效状态

UI 读取规则：
- `BoardList` 的行状态必须来自 `appStore + boardBuildStore` 合成后的 view model
- `MainWindow` 的状态栏板块摘要必须基于 `boardBuildStore`
- `ChartHeader` 的构建中 / 失败提示必须基于当前 target 的构建态
- 禁止只读 `appStore.boards` 的静态状态字段就决定整页显示

禁止：
- 组件内部自行拼装构建状态
- `chartStore` 顺手托管设置页或笔记页状态
- `boardBuildStore` 直接持有整套图表数据
- `boardBuildStore` 只是裸 `Record<string, Payload>` 且没有去重规则
- `chartStore` 只有单一 payload，没有 `loading / building / empty / failed / lastGoodPayload`
- `syncStore` 只有一次性 bootstrap 注入值，没有主动刷新能力

## 13. 当前收尾清单
若当前代码仍存在以下任一项，则 Step 3 应标记为“进行中”：
- `commands` 层仍是 `invokeOrFallback(...)`，真实失败会自动切本地样例
- 主窗口仍注册 `registerLocalFallbackSync()` 或等价本地模拟同步
- 设置窗口仍通过 `bootstrap()` 后 `applyBootstrapPayload()` 回写全局 `appStore`
- 快路径保存板块后，没有保证主窗口一定收到目录更新
- `get_sync_status()` 虽已封装，但主流程仍主要依赖 `bootstrap.syncStatus + event`

建议按以下顺序收尾：
1. 去掉命令层静默 fallback
2. 移除主窗口本地 fallback sync
3. 补快路径跨窗口同步
4. 改成真正的 settings-only 初始化
5. 把 `get_sync_status()` 纳入主流程主动刷新

## 13. 测试与验收重点
至少覆盖：
- `save_board()` 快路径后立即出图
- `save_board()` 后台路径后立即进入构建中态
- 构建完成后当前板块自动刷新
- 构建完成后非当前板块不打断主图
- 构建失败态可见
- 重启恢复后能继续展示构建状态
- 设置窗口输入期间后台事件不会打断输入
- 笔记输入期间后台事件不会打断输入
- 图表实例始终只创建一次
- 设置窗口打开和保存过程中不触发主图请求

## 14. 本步骤禁止项
- 后台构建状态变化导致整页重绘
- 后台构建状态变化导致设置窗口或笔记区被刷新
- 板块构建期间图表节点重建
- 事件一来就全量重新 `bootstrap()`
- 为了图省事把构建态塞进组件本地状态
- 把“构建完成自动刷新”实现成“无条件重拉当前页面所有数据”
