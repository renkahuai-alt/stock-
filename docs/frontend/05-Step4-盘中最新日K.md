# 前端 Step 4：盘中最新日K

## 1. 目标
本步骤只负责前端对“盘中低频更新当天最后一根日K”的呈现，不做分钟线。

当前说明：
- Step 4 建立在 Step 3 已基本收口的前提上
- 若主路径仍保留静默 fallback / mock、主窗口仍有本地 fallback sync、或设置窗口仍会改写主窗口全局状态，则 Step 4 不能视为完成
- Step 4 的重点不是“把 live event 接上”本身，而是在不引入新一轮重渲染、错绑 watch、旧事件回写的前提下，稳定接入盘中最新日K

### 1.1 当前仓库落地口径
- watch 主链路当前由独立 `watchStore` 管理，前端同一时刻只允许维护一条活动 watch
- 相同 watch 请求必须复用当前活动 watch；`target / granularity / boardAlgorithm` 任一变化时必须先停旧 watch，再启新 watch
- `range` 不进入 watch key，也不允许因为切 `range` 重启 watch
- `started = false, marketState = closed` 是正常业务分支；Step 4 需要验证的是生命周期、局部 patch 和旧事件丢弃，而不是把它当异常处理
- 当前图表控制器仍是单实例占位实现，因此 Step 4 的前端验收重点是 watch 生命周期、状态栏反馈和 `updateOverlay()` 通道稳定，不把真实图表渲染性能写成已完成

## 2. 必做项
- 接入 `start_chart_watch`
- 接入 `stop_chart_watch`
- 接入 `chart-live-update` event
- 当前目标变化时切换 watch
- 窗口进入后台或隐藏时停止旧 watch

必须额外满足：
- watch 只允许在真实主路径上生效，不能建立在 fallback / mock 数据源之上
- `day` 粒度才允许开启 watch，`week` 粒度必须停 watch
- 切换 `target / granularity / boardAlgorithm` 时，旧 watch 必须先失效，新 watch 才能生效
- 仅切换 `range` 时不重启 watch，只刷新当前图表历史窗口
- `chart-live-update` 必须做归属校验和旧事件丢弃，不能“谁来了就打到当前图上”
- 盘中更新失败时必须可观测，但不能触发整页重载或主图重建

## 3. 实施前提与顺序
Step 4 不是“事件接上就算完成”的步骤。

前端开工前，至少需要后端先补齐最小真链路：
- `start_chart_watch`
- `stop_chart_watch`
- `chart-live-update`

若 Step 3 仍未完成以下任一项，则 Step 4 只能做预改造，不能宣称完成：
- 命令层已关闭静默 fallback
- 主窗口已移除本地 fallback sync
- 设置窗口已改成 `settings-only` 初始化
- 快路径跨窗口同步已收口

推荐固定顺序：
1. 先冻结 `start / stop / chart-live-update` 契约与字段归一规则
2. 再定义 watch key、token 与旧事件丢弃规则
3. 再实现 watch 生命周期管理
4. 再实现 `chart-live-update` 的局部 patch
5. 最后接后台 / 可见性切换 / 错误兜底

补充：
- 验收前必须确认后端 Step 4 已移除 scaffold / sample live event 路径
- 若后端仍返回伪 `started = true` 或 sample overlay，则前端只能做预改造，不能标记 Step 4 完成

## 3.1 Step 4 契约冻结
前端 Step 4 必须先冻结 watch 相关契约，避免联调阶段再次发明字段。

### `start_chart_watch(payload)` 输入
至少必须包含：
- `targetType`: `index | board | symbol`
- `targetId`
- `granularity`: 仅允许 `day`
- `boardAlgorithm?`: `equal_weight_v1 | market_cap_weight_v1`

固定规则：
- `range` 不进入 watch 输入
- `targetType = board` 且未传 `boardAlgorithm` 时，前端本地请求口径也必须归一为 `equal_weight_v1`
- `granularity != day` 时，不允许尝试启动 watch

### `start_chart_watch()` 返回
至少必须包含：
- `watchId`
- `started`
- `targetType`
- `targetId`
- `granularity`
- `boardAlgorithm?`
- `intervalSec`
- `marketState`
- `updatedAt`
- `message?`

固定规则：
- 仅当 `started = true` 时，前端才允许把该返回登记为当前活动 watch
- 若 `started = false` 且 `marketState = closed`：
  - 这是正常业务分支，不是异常
  - 前端不得弹错误 toast
  - 前端不得进入自动重试风暴

### `stop_chart_watch()` 返回
至少必须包含：
- `stopped`
- `watchId?`
- `updatedAt`

### `chart-live-update` event
每条 event 至少必须包含：
- `watchId`
- `targetType`
- `targetId`
- `granularity`
- `boardAlgorithm?`
- `updatedAt`
- `marketState`
- `sourceStatus`
- `overlay`
- `meta`

其中：
- `overlay` 至少包含：
  - `tradeDate`
  - `open`
  - `high`
  - `low`
  - `close`
  - `volume`

说明：
- `updatedAt` 是事件新旧判定字段，不允许省略
- `range` 不进入 event payload
- 前端不得自行扩展另一套 live payload 口径

## 4. watch 归属与去重规则
Step 4 必须把“当前正在看的那张日K”定义清楚，禁止 live update 越权更新。

### 4.1 单一事实源
watch 生命周期只能由主窗口的 chart/watch store 统一维护。

禁止：
- `MainWindow` 自己记一份活跃 watch
- `ChartPanel` 再自己记一份当前订阅目标
- 组件内部收到 event 后直接改图，不回写 store

### 4.2 watch key
前端必须至少基于以下维度构造本地 watch 身份：
- `targetType`
- `targetId`
- `granularity`
- `boardAlgorithm`

固定规则：
- `range` 不属于 watch key
- `targetType = board` 且未传 `boardAlgorithm` 时，前端本地 key 必须先归一成 `equal_weight_v1`
- 非 `board` 目标必须使用固定空值口径，例如 `null` 或固定 sentinel，不允许一处用 `undefined`、一处用空串
- 后端 `watchId` 返回前，前端必须先生成本地 lifecycle token / pending token，用于约束异步返回写回

后端返回的 `watchId` 是最终判定键，前端必须写入 chart/watch store，并优先用它判断事件归属。

补充：
- `start_chart_watch()` / `stop_chart_watch()` 返回写回前，必须先校验该 token 仍对应当前待生效 key
- 若用户已切目标、切粒度、切算法或窗口已进入后台，旧 command 的晚回结果必须直接丢弃，禁止覆盖当前活动 watch 状态

### 4.3 旧事件丢弃
`chart-live-update` 进入前端时，必须至少校验：
- 当前选中目标仍匹配
- 当前粒度仍为 `day`
- `boardAlgorithm` 仍匹配当前请求
- `watchId` 必须匹配当前活动 watch

同一 `watchId` 下还必须继续校验：
- `updatedAt` 必须晚于当前已生效 overlay 的 `updatedAt`
- 若 `updatedAt` 早于或等于当前已生效版本，必须视为旧事件直接丢弃

不匹配时必须直接丢弃，禁止回写图表或状态栏。

## 5. 局部更新要求
盘中更新只允许影响：
- 当前图表最后一根 bar
- 图表标题 / 来源提示
- 状态栏更新时间

盘中更新不能影响：
- 设置窗口
- 板块编辑输入框
- 笔记输入区
- 非当前目标页面区域
- 板块目录 / 成分股列表
- 图表挂载节点本身

补充：
- live update 只能 patch 当前图表 payload 的最后一根 bar 或 overlay 摘要
- 不能借 live update 顺手触发 `bootstrap()`、全量 `get_chart()`、全局 store 重放
- 若因 `range` 切换、页面恢复或受控补偿触发 `get_chart(day)`：
  - 响应写入前必须先校验该请求仍匹配当前选中态的 `targetType / targetId / granularity / boardAlgorithm / range`
  - 响应写入 store 前必须再次与当前活动 overlay 做只读合并
  - 不允许让晚到的历史响应覆盖已生效的 live patch
- `week` 视图不得临时合并 `day` overlay

## 6. 禁止项
- 因盘中更新重建图表
- 因盘中更新触发整页更新
- 因盘中更新导致输入焦点丢失
- 因收到 `chart-live-update` 再立刻触发 `bootstrap()` 补查
- 因收到 `chart-live-update` 对当前目标做全量 `get_chart()` 重拉
- 因 watch 启停失败而回退到本地 mock / fallback 数据
- 在 `week` 视图继续保留 `day` watch

## 7. 生命周期规则
### 7.1 启动条件
仅当以下条件同时满足时允许启动 watch：
- 当前窗口为主窗口
- 当前目标存在
- 当前粒度为 `day`
- 主窗口可见，且应用未进入后台
- 前端已拿到当前目标的稳定 chart payload

补充：
- `Settings` 窗口单独获得焦点，但主窗口仍可见时，不应被视为“进入后台”
- 不允许因为主窗口与设置窗口之间的焦点切换，产生多余的 `start/stop watch` 抖动
- “稳定 chart payload” 固定指当前选中目标最近一次 `get_chart()` 已成功写入 chart store，且该响应仍匹配当前选中态
- 只有 `ready` / `empty` 允许启动 watch；`loading / building / failed` 均不得启动 watch

### 7.2 停止条件
以下任一情况发生时，必须停止旧 watch：
- 切换目标
- 切换到 `week`
- 切换板块算法
- 主窗口隐藏 / 进入后台
- 主窗口销毁

补充：
- 切换 `range` 只允许触发一次受控 `get_chart()`，不允许因为 `range` 变化重复 `start/stop watch`
- 前端必须同时监听浏览器文档可见性与 Tauri 主窗口可见/活跃状态，并统一折叠成单一窗口活跃态
- 仅当“文档可见 + 主窗口活跃”同时成立时，才允许保留 watch；任一条件变为 false，都视为进入后台
- 同一轮 `hide / blur / minimize` 连续触发时必须做去重，避免重复 `start/stop watch`

### 7.3 幂等要求
- 重复对同一 key 启动 watch，不得生成重复订阅
- 停止一个已不存在的 watch，不得抛错或打断主流程
- 快速切换目标时，必须保证“旧 watch 失效”早于“新 watch 生效”

## 8. 错误与降级
Step 4 必须区分以下情况：
- `start_chart_watch` 失败
- `stop_chart_watch` 失败
- `chart-live-update` 长时间未到
- 当前图表仍可显示，但 live update 暂不可用
- `started = false, marketState = closed`

推荐处理：
- 保留当前最后一次成功图表，不清空主图
- 状态栏显示“盘中更新暂不可用”一类的轻量反馈
- 不自动切回 mock / fallback
- 不因 watch 失败把主图状态改成全局失败
- `marketState = closed` 时显示轻量“当前非盘中时段”提示，不按异常处理
- `started = false, marketState = closed` 时，前端必须视为“当前未启动 watch”，不得保留一个伪活动 watch
- 此时不进入前端自建轮询或自动重试；仅在用户切目标、切回 `day`、窗口重新回到前台活跃，或显式恢复时，再重新评估是否启动 watch
- 若收到 `marketState = closed` 或 `sourceStatus = market_closed` 的 live event，也必须按同样规则收口本地 watch 状态

## 9. 本步骤交付
- 当前目标盘中 overlay 呈现
- watch 生命周期管理
- 状态栏盘中更新时间显示
- `day` / `week` 切换下的 watch 正确启停
- 旧事件丢弃与当前 watch 归属判断
- 盘中失败的轻量降级 UI

## 10. 完成标准
- 用户在盘中能感知当天 K 线变化
- 前端无明显卡顿
- 盘中更新不打断输入
- 切换到 `week` 后不再收到 `day` watch 回写
- 切换目标后旧目标 live update 不会污染当前图表
- watch 启停失败时主图仍稳定，不会触发整页重载
- 未满足任一项，都不能标记 Step 4 完成

## 11. 常见时序坑与防护
### 11.1 旧 watch 回写新目标
风险：
- 用户已切到别的指数、板块或个股，旧 `chart-live-update` 仍把最后一根 bar 写进当前图

要求：
- 必须按 watch key 做归属判断
- 不匹配的 live update 必须直接丢弃

### 11.2 week 视图仍在吃 day 更新
风险：
- 用户切到 `week` 后，日K live update 继续推进当前图，导致粒度错乱

要求：
- `week` 视图必须停 watch
- `chart-live-update` 必须再次校验当前 granularity

### 11.3 live update 触发重查询风暴
风险：
- 每次收到 live update 都去 `bootstrap()` 或 `get_chart()`，导致 UI 抖动和卡顿

要求：
- live update 只能做局部 patch
- 查询仅用于重新进入页面、显式恢复、或丢事件后的受控补偿

### 11.4 watch 启停并发
风险：
- 快速切换目标 / 算法 / 窗口可见性时，多次 `start/stop` 交错，旧 watch 未停干净

要求：
- 必须有当前活跃 watch token 或等价机制
- 旧 token 的事件必须失效

### 11.5 设置与笔记被盘中更新打断
风险：
- 主窗口 live update 带动整页更新，导致输入区失焦或被重建

要求：
- 盘中更新必须严格局部 patch
- 设置窗口、笔记区、板块编辑输入框都不能参与这条更新链
