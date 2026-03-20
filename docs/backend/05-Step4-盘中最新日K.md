# 后端 Step 4：盘中最新日K

## 1. 目标
本步骤负责后端侧“盘中低频更新当前活动目标的当天最后一根日K”，并且必须显式规避旧版本已经出现过的坑：
- 盘中更新触发整页状态流，误伤设置窗口和输入框
- 切目标后旧 watch 未释放，晚到事件串到新目标
- 同一轮重复请求相同 symbol
- 板块盘中更新时重算整段历史
- 盘中 overlay 污染正式图表缓存

Step 4 的完成标准不是“能动起来”，而是：
- 当前图表可以看到当天 bar 的变化
- 输入、设置窗口和非当前目标区域不受影响
- watch 生命周期可控、可停止、可恢复
- 不把盘中低频更新重新做成旧版那种轮询风暴

## 2. 本步骤范围
### 2.1 必须完成
- 实现 `start_chart_watch(payload)`
- 实现 `stop_chart_watch()`
- 实现 `chart-live-update` event
- 只监听当前活动目标
- 默认 `15s` 一次
- 仅在美股正常交易时段运行
- 个股 / 指数当天 overlay
- 板块当天 overlay
- overlay 内存层与历史缓存隔离
- watch 资源释放与晚到事件防串线

### 2.2 本步骤不做
- 分钟 K
- 高频实时订阅
- 盘口 / 逐笔
- 多目标并行 watch
- 盘中高频写 SQLite
- 盘中触发正式同步或正式板块重建
- `week` 视图盘中更新

说明：
- Step 4 只服务 `day` 粒度的盘中 overlay。
- 当前若前端处于 `week`，后端不负责临时产出“周线盘中版”；要么不启动 watch，要么返回明确业务错误。

## 3. 与 Step 3 的衔接前提
Step 4 默认建立在 Step 3 已完成以下能力之上：
- `get_chart()` 已支持 `targetType / targetId / range / granularity / boardAlgorithm`
- 图表历史数据、周 K、板块 K 线都已走正式 `repository + chart_engine`
- 图表缓存 key 已包含 `targetType + targetId + range + granularity + boardAlgorithm`
- 板块构建、同步、图表缓存失效已经有正式边界

Step 4 只补“盘中 overlay”这一层，不重写 Step 3 的历史数据链路。

## 4. 总原则
### 4.1 只允许 1 个活动 watch
Step 4 首版必须固定为应用级单活动 watch：
- 同一时刻只允许 `1` 个 chart watch 处于运行态
- 新 watch 启动前，必须先取消旧 watch
- 重复调用 `start_chart_watch(payload)` 且 payload 相同：
  - 不允许再起第二条轮询任务
  - 应复用现有 watch，返回现有 `watchId`
- `range` 不属于 watch 生命周期输入，也不属于 watch 身份
- `stop_chart_watch()` 必须幂等
- 切目标、窗口进入后台、窗口隐藏、应用休眠、应用退出时必须立即停止旧 watch

禁止：
- 主窗口和设置窗口各自偷偷起一条 watch
- 切目标后旧任务继续跑，并向新目标页面发事件

### 4.2 `start / stop` 必须快速返回
`start_chart_watch()` 和 `stop_chart_watch()` 都不能等待远端 quote 请求完成后再返回。

要求：
- command 层只做参数校验、状态切换、任务注册 / 取消
- 首次远端拉取在后台异步执行
- `stop_chart_watch()` 立即取消 in-flight 请求

建议目标：
- `start_chart_watch()` 返回 `< 100ms`
- `stop_chart_watch()` 返回 `< 100ms`

当前实现补充：
- `start_chart_watch()` 先做参数校验、单活动 watch 切换、provider 可用性判断，再走本地 market status 快速判定
- command 返回前不等待远端 `market_status()`，也不等待首轮 quote 请求完成
- Longbridge `QuoteContext` 冷启动通过后台 `prewarm` 提前建立；预热不计入 `start / stop` SLA
- provider 预热固定在应用 `setup`、`bootstrap()`、`save_credentials()`、`start_chart_watch()` 触发
- `save_credentials()` 必须先失效 runtime provider cache，再启动后台预热，避免继续复用旧凭证对应的 provider 实例

### 4.3 盘中更新只允许更新 overlay
盘中 watch 只能维护“当天最后一根 bar 的内存覆盖层”。

禁止：
- 每次轮询都重算整段历史
- 每次轮询都重新调用完整 `get_chart()`
- 每次轮询都向前端发送整段 bars
- 每次轮询都写 SQLite

## 5. Command / Event 契约
### 5.1 `start_chart_watch(payload)` 输入
至少必须包含：
- `targetType`: `index | board | symbol`
- `targetId`
- `granularity`: 仅允许 `day`
- `boardAlgorithm?`: `equal_weight_v1 | market_cap_weight_v1`

规则：
- `range` 属于 `get_chart()` 的历史裁剪维度，不进入 `start_chart_watch(payload)`
- 仅切换 `range` 时，不允许因为历史窗口变化而重启 watch
- `targetType = board` 且未传 `boardAlgorithm` 时，默认 `equal_weight_v1`
- `granularity != day`：
  - 返回明确业务错误，例如 `WATCH_UNSUPPORTED_GRANULARITY`
  - 不允许静默启动一个与请求不一致的 watch
- `targetId` 不存在：
  - 返回明确业务错误
  - 不允许返回伪成功

### 5.2 `start_chart_watch()` 返回
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

建议口径：
- `marketState`: `open | closed`

规则：
- 市场关闭时：
  - `started = false`
  - `marketState = closed`
  - `message` 写明“当前不在盘中时段”
  - 不启动后台轮询任务
- 重复启动相同 payload 时：
  - 返回当前活动 `watchId`
  - 不允许新建第二条轮询

### 5.3 `stop_chart_watch()` 返回
至少必须包含：
- `stopped`
- `watchId?`
- `updatedAt`

规则：
- 无活动 watch 时也必须返回可判定结果
- 不允许因为“当前没在运行”就抛未处理异常

### 5.4 `chart-live-update` event
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
- `sourceStatus` 建议至少区分：
  - `live`
  - `delayed`
  - `degraded`
  - `market_closed`
- `overlay` 至少包含：
  - `tradeDate`
  - `open`
  - `high`
  - `low`
  - `close`
  - `volume`
- `meta` 对 `board` 目标建议额外补充：
  - `valueMode`
  - `weightSnapshot?`
  - `weightSnapshotTradeDate?`

规则：
- 每条 event 必须带 `watchId`
- 每条 event 必须带 `updatedAt`
- 同一 `watchId` 下，`updatedAt` 必须严格单调递增
- `updatedAt` 必须以后端事件发送时刻或本地版本戳生成，不直接复用不稳定的远端原始时间字段
- `range` 不进入 event payload
- 前端必须能够用 `watchId` 丢弃旧任务的晚到事件
- 单轮轮询最多推 `1` 条 live event
- 若本轮数据完全无变化，可不推事件

禁止：
- event 不带 `watchId`
- event 只带“最后价格”，不带可合并的完整 overlay bar

## 6. 市场时段与交易日口径
### 6.1 时段口径必须冻结
Step 4 首版仅在美股正常交易时段运行：
- `America/New_York`
- `09:30 - 16:00`

首版不包含：
- pre-market
- after-hours

### 6.2 交易日判断
必须复用正式市场日历 / 交易日判断逻辑：
- 不允许直接用本机当天日期代替交易日
- 不允许用中国时区自然日直接判断“今天是交易日”

当前实现补充：
- Step 4 当前为“本地快速 market status + 后台真实 quote”双路径
- 开闭市快速判断使用 `America/New_York` 时区、本地美股规则日历和最近已完成交易日回退逻辑
- 本地规则日历必须覆盖：周末、主要休市日、半日市；不能把开闭市判断重新退化成“只看纽约时间几点几分”
- 若未来市场节假日规则有调整，必须优先更新本地规则日历，避免重新把开闭市判断放回远端阻塞路径

### 6.3 盘后处理
若 watch 运行中遇到收盘：
- 必须停止后续轮询
- 可发送 `market_closed` 状态事件
- 不允许在盘后继续轮询 quote

## 7. 个股 / 指数 overlay 生成规则
### 7.1 个股
个股当天 overlay 使用 quote 快照生成：
- `tradeDate`：当前盘中的交易日
- `open`：当日开盘价，生成后在该交易日内固定
- `high`：当日最高价
- `low`：当日最低价
- `close`：最新价
- `volume`：当日累计成交量

合并规则：
- 若历史序列最后一根 `tradeDate` 与 overlay 相同：
  - 只替换最后一根
- 若历史序列最后一根早于 overlay 的 `tradeDate`：
  - 只在内存中 append 一根当天 bar

禁止：
- 为了生成个股 overlay 重新拉整段历史

### 7.2 指数
指数盘中 overlay 必须沿用 Step 2 / Step 3 已冻结的正式目标口径。

要求：
- 指数与个股共用统一的 `targetType / targetId` 标识方案
- 不允许在 watch 分支临时引入另一套 symbol 命名
- 若盘中 quote 来源与历史 bars 来源存在转换、映射或缩放：
  - 必须在后端完成转换后再发给前端
  - 不允许把“原始代理数据”直接暴露给前端自行修正

禁止：
- 历史图和盘中图使用两套不同的指数语义

## 8. 板块 overlay 生成规则
### 8.1 板块只允许重算今天这一根 bar
要求：
- 只计算当前交易日的板块 overlay
- 不允许重算整段 `board_daily_bars`
- 不允许因为盘中 watch 触发正式板块重建

### 8.2 成员 quote 请求必须先去重
每轮轮询都必须先对板块成员 symbol 做标准化与去重。

要求：
- 同一轮内同一 symbol 只能请求一次
- 不允许因为成员重复、权重计算分支或容错逻辑，再次请求同一 symbol
- 若 quote provider 支持批量请求，应优先走批量请求

### 8.3 板块当日 OHLC 计算口径
板块盘中 overlay 必须继续沿用“成员股收益驱动板块指数”的正式语义。

推荐计算方式：
1. 取板块上一个正式交易日收盘值 `B_prev`
2. 对每个有效成员取上一个正式交易日收盘价 `P_prev_i`
3. 基于当日 quote 计算成员相对变化
4. 用权重聚合当日 `open / high / low / close`

可执行口径：
- `board_open = B_prev * Σ(w_i * open_i / P_prev_i)`
- `board_high = B_prev * Σ(w_i * high_i / P_prev_i)`
- `board_low = B_prev * Σ(w_i * low_i / P_prev_i)`
- `board_close = B_prev * Σ(w_i * last_i / P_prev_i)`

要求：
- `w_i` 必须先归一化
- 仅允许按“今天这一轮 quote”重算这 4 个值
- 不允许把成员周 K 或历史板块 bars 重新参与临时回算

### 8.4 `equal_weight_v1`
`equal_weight_v1` 规则固定为：
- 有效成员等权
- 每轮按有效成员重新归一化

### 8.5 `market_cap_weight_v1`
`market_cap_weight_v1` 规则固定为：
- 优先使用本地最新股份数据快照
- 不允许结合当前 `quote.close` 动态重算当轮市值权重
- 必须复用“上一完整交易日收盘价 * shares”的固定权重快照
- 不允许在 watch 轮询中刷新远端静态信息

同一份权重快照必须同时用于：
- 板块历史合成
- 当日盘中 overlay
- 成员 `weight_percent`

元信息口径：
- `valueMode = synthetic_board_points`
- `weightSnapshot = previous_close_x_shares`
- `weightSnapshotTradeDate = 历史最后一根已完成交易日`

若股份数据不完整：
- 先降级为最近可用权重快照
- 若最近可用权重快照也不存在，或有效权重覆盖不足：
  - 本轮不产出新的板块 overlay
  - `sourceStatus = degraded`
  - 在 `meta` 中明确写明原因

### 8.6 板块 volume 口径
板块 `volume` 字段必须沿用 Step 3 已冻结的板块日 K 口径。

禁止：
- 在 Step 4 临时把板块 volume 改成“成员股 volume 简单求和”
- 日线、周线、盘中三条路径使用三套不同 volume 语义

## 9. overlay 存储、缓存与合并
### 9.1 overlay 必须独立于正式历史缓存
盘中 overlay 必须作为独立内存层存在，不能直接覆盖正式历史缓存。

要求：
- overlay 至少按以下维度区分：
  - `targetType`
  - `targetId`
  - `granularity`
  - `boardAlgorithm`
- 正式历史 bars 缓存与盘中 overlay 分开存储

当前实现补充：
- 运行时至少分离以下内存层：
  - provider cache
  - raw daily cache
  - weekly cache
  - chart payload cache
  - live overlay cache
  - active watch handle
- `live overlay cache` 只存当前 watch 产生的当天 bar 覆盖层，不得替代正式 `daily_bars` / `board_daily_bars`

### 9.2 `get_chart(day)` 与 overlay 合并
若当前目标存在活动 overlay：
- `get_chart(day)` 可在返回前做“历史 bars + overlay”只读合并
- 合并仅影响本次返回，不修改正式历史缓存

对 `board` 目标额外要求：
- 合并后的 `sourceStatus` 必须与当前活动 overlay 一致
- 合并后的 `meta.valueMode / meta.weightSnapshot / meta.weightSnapshotTradeDate` 必须与当前活动 overlay 一致

### 9.3 `get_chart(week)` 行为
Step 4 首版不支持 `week` 盘中 watch。

要求：
- `week` 不得临时由前端自行聚合盘中变化
- 后端不得在 watch 路径偷偷重算整周 payload

### 9.4 缓存污染防护
必须明确做到：
- 盘中每一轮更新不能触发全量 `cache.clear()`
- 原始日线缓存不因 overlay 每轮变化而失效
- 周线聚合缓存不因 `day` overlay 每轮变化而失效
- 图表 payload 若包含 live overlay 版本戳，必须只影响当前目标当前口径

禁止：
- 因一条 live event 清空所有图表缓存
- `day` live overlay 污染 `week` 缓存结果

## 10. 性能与节流要求
### 10.1 单轮工作边界
单轮轮询只允许做：
1. 判断 watch 是否仍有效
2. 拉取当前目标所需 quote
3. 生成 1 根 overlay
4. 若数据有变化，则发送 1 条 `chart-live-update`

禁止：
- 单轮同时做同步任务
- 单轮同时做正式板块落库
- 单轮同时做静态信息刷新

### 10.2 事件节流
要求：
- 最多每个轮询周期推 `1` 条 event
- 数据无变化时可不推
- 连续失败时不能疯狂推报错事件

当前实现补充：
- 板块成员 quote 会先按标准化 target 去重，再统一请求远端数据
- 只有 `watchId / marketState / sourceStatus / overlay / meta.message` 发生变化时才会真正发送新的 `chart-live-update`

### 10.3 旧版性能坑防护
Step 4 必须明确规避：
- 切目标后两个 watch 同时跑
- 同一板块同一轮重复请求相同 symbol
- 每次盘中更新都触发整段 bars 重建
- 每次盘中更新都让前端重新 `bootstrap()`
- 每次盘中更新都导致输入组件收到无关状态流

### 10.4 预热与冷启动分摊
Step 4 的性能口径必须拆成两段：

1. command 快返回
- `start_chart_watch()` / `stop_chart_watch()` 的 SLA 只考察 command 自身
- 不允许把 provider 冷启动、远端交易日查询或首轮 quote 拉取时间算进 command 返回耗时

2. 后台 warm path
- provider 冷启动可以在后台预热
- 首条 `chart-live-update` 的 warm path 应尽量接近单轮真实 quote 请求耗时
- 若未预热，允许出现冷启动首轮延迟，但不能反向污染 command 快返回契约

当前实现补充：
- `AppRuntime` 固定维护 provider cache 与 `prewarm in-flight` 防抖状态
- 同一时间只允许 `1` 条 provider 预热任务在后台运行

### 10.5 当前实测（非契约，仅作为实施记录）
以下为 `2026-03-19` 使用真实 Longbridge 凭证的本地实测记录，仅用于说明当前实现已经把性能瓶颈从 command 返回迁移到后台预热：

- 未预热、强制 open 观测：
  - symbol `start_chart_watch()` 约 `36.51ms`
  - board `start_chart_watch()` 约 `53.16ms`
  - symbol 首条 live event 约 `6232.38ms`（冷启动）
  - board 首条 live event 约 `3094.78ms`（冷启动）

- 预热后观测：
  - provider `prewarm` 约 `2494.66ms`（后台执行）
  - warm symbol `start_chart_watch()` 约 `4.86ms`
  - warm board `start_chart_watch()` 约 `3.01ms`
  - warm symbol 首条 live event 约 `72.80ms`
  - warm board 首条 live event 约 `83.51ms`
  - `stop_chart_watch()` 约 `0.02ms ~ 0.05ms`

## 11. 错误处理与降级
### 11.1 参数错误
至少区分：
- `targetType` 非法
- `targetId` 不存在
- `granularity` 非 `day`
- `boardAlgorithm` 非法

要求：
- 返回明确业务错误
- 不允许静默改参

### 11.2 远端 quote 异常
至少区分：
- 网络错误
- 鉴权失败
- 限频
- 空数据
- 单个成员 quote 缺失

要求：
- 单轮失败不能直接清空现有 overlay
- 单轮失败不能导致 watch 失控重启
- 连续失败应进入受控降级，而不是事件风暴

建议口径：
- 连续 `1~2` 轮失败：仅记录日志，下一轮继续
- 连续 `>= 3` 轮失败：
  - 允许发送 1 条 `sourceStatus = degraded` 的状态事件
  - 恢复前不要重复刷屏

### 11.3 晚到事件防串线
要求：
- 目标切换或 stop 后，旧 watch 的 in-flight 请求必须取消
- 即便仍有晚到结果，也必须因 `watchId` 不匹配而被丢弃

禁止：
- A 目标的晚到 event 更新到 B 目标图表

## 12. 日志与可观测性
至少记录：
- `start_chart_watch()` 参数摘要与结果
- `stop_chart_watch()` 触发原因
- watch 创建 / 复用 / 停止
- provider `prewarm` 触发来源、是否命中 in-flight 去重、开始 / 成功 / 失败 / 耗时
- 每轮 quote 请求的 symbol 数与耗时
- 板块成员去重前后数量
- `chart-live-update` 推送次数
- 连续失败次数与降级状态

禁止：
- watch 异常只在前端控制台可见，后端无日志

## 13. 测试要求
至少覆盖：
- `start_chart_watch()` 正常启动
- `stop_chart_watch()` 幂等
- 重复 `start` 不产生第二条 watch
- 切目标后旧 watch 被取消
- 窗口进入后台后 watch 停止
- `granularity = week` 返回明确错误
- 同一 `watchId` 下 `updatedAt` 严格递增
- 个股当天 overlay 替换最后一根
- 指数盘中口径与正式历史口径一致
- 板块只重算今天一根，不重算整段历史
- 板块单轮成员 quote 去重
- `market_cap_weight_v1` 缺 shares 时降级为最近权重快照
- 连续失败时不会产生事件风暴
- live overlay 不污染 `get_chart(day)` 正式历史缓存
- live overlay 不污染 `get_chart(week)` 结果
- 连续运行 `30` 分钟无明显资源泄漏
- `start_chart_watch()` 不等待远端 `market_status()`
- provider 预热后首条 live event 明显优于未预热冷启动路径

测试分层：
- 自动化回归默认使用 fake provider / stub / 半真数据夹具验证 watch 生命周期、去重、overlay 合并与冷启动分摊
- 真实 Longbridge 凭证 + 可验证盘中时段下，再补充冷路径 / warm path 烟测样本；该部分不并入默认 `cargo test`

当前已落地的 Step 4 回归测试至少包含：
- `granularity = week` 返回 `WATCH_UNSUPPORTED_GRANULARITY`
- 市场关闭时返回 `started = false` 且不启动后台任务
- 相同 watch 重复启动复用同一 `watchId`
- `day` 合并 overlay、`week` 不合并 overlay
- 板块单轮成员 quote 去重
- `market_cap_weight_v1` 下盘中 overlay 与历史板块使用同一套静态权重快照
- `market_cap_weight_v1` 下无明显跳空时，历史最后一根日 K 与盘中首笔 overlay 连续
- `market_cap_weight_v1` 下 `chart-live-update` 与 `get_chart(day)` 合并结果的 `valueMode / weightSnapshot / weightSnapshotTradeDate` 一致
- `start_chart_watch()` 不等待慢 `market_status()`
- `prewarm_provider()` 可显著降低首条 live event 冷启动延迟

## 14. 验收标准
- 当前活动目标在盘中可看到当天 bar 变化
- `start_chart_watch()` 和 `stop_chart_watch()` 快速返回，不阻塞主流程
- 切目标、后台化、隐藏窗口后旧 watch 不再继续推事件
- 设置窗口、板块编辑输入、笔记输入不被盘中更新打断
- 板块盘中更新不重算整段历史
- 无全量缓存清空
- 无多余 SQLite 写入
- `market_cap_weight_v1` 下，板块历史曲线、盘中 overlay、成员权重使用同一套权重快照
- `market_cap_weight_v1` 下，无明显跳空时，历史最后一根日 K 与盘中首笔 overlay 连续
- `board_daily_bars`、`chart-live-update`、`get_chart(day)` 合并结果三者的 `boardAlgorithm / valueMode / weightSnapshot / weightSnapshotTradeDate` 一致

## 15. 本步骤禁止项
- `week` 视图偷偷启用盘中聚合
- 同时存在多条活动 watch
- 每轮轮询对同一 symbol 重复请求
- 每轮轮询写 SQLite
- 每轮轮询发送整段 chart payload
- 每轮轮询重算整段板块历史
- 没有 `watchId` 就发送 `chart-live-update`
- 没有 `updatedAt` 就让前端判断事件新旧
