# 后端 Step 3：真实数据与板块构建

## 1. 目标
本步骤是后端核心交付：接入真实历史数据、同步、板块异步构建和缓存，解决旧项目在真实数据量上来后“创建大板块卡死、重复计算、状态失真”的问题。

Step 3 重点不是单独完成某个模块，而是把以下链路一次打通：
- 真实历史数据接入
- 本地增量同步
- `board_build` 后台流水线
- 图表生成与缓存
- 板块构建状态落库、查询、事件通知

## 2. 本步骤范围
### 2.1 必须完成
- 接入 `Longbridge` 历史日 K
- 接入最新交易日查询
- 接入静态信息刷新
- 实现 `startup / manual` 同步
- 实现周 K 聚合
- 实现 `board_build` 后台流水线
- 实现板块构建状态落库与恢复
- 实现缓存层与细粒度失效

### 2.2 本步骤不做
- 分钟 K
- 高频实时订阅
- 盘口 / 逐笔
- 盘中 `watch` 正式接入
- 跨多板块并行构建

## 3. 实施前提与顺序
Step 3 不是“先把前端接一下真实数据”就能完成。

后端必须先把最小真链路补齐，前端 Step 3 才有稳定接入点：
- `bootstrap`
- `save_board`
- `get_board_build_status`
- `get_chart`

若这 4 条链路仍然返回 scaffold / 样例数据，则 Step 3 只能算后端骨架未完成，前端不应被要求做最终联调验收。

推荐后端固定顺序：
1. 先统一 `buildStatus / buildPhase / updatedAt / buildJobId` 契约
2. 再补 `models + commands` 真字段
3. 再补 `repository + chart_engine + services`
4. 再接 `Longbridge` 历史数据与同步
5. 最后接 `board_build` 后台流水线、事件与缓存

其中最小真链路必须至少达到：
- `bootstrap`
  - 真实读取 SQLite 中的 indexes / boards / members / note / sync 状态
  - 不允许返回单板块硬编码样例
- `save_board`
  - 快路径必须真实落库并可恢复
  - 后台路径必须真实落库 `boards.build_*`
  - 不允许只把状态塞进内存 `Mutex`
- `get_board_build_status`
  - 必须从持久层读取当前板块状态
  - 不允许对未知 `boardId` 返回伪默认状态
- `get_chart`
  - 必须走 `repository + chart_engine`
  - 不允许继续返回 sample chart

## 4. `save_board()` 与 `board_build` 总原则
### 4.1 `save_board()` 双路径必须保留
1. 快路径
- 本地数据已齐
- 允许立即重算并直接出图

2. 后台路径
- 缺历史数据或超过保护阈值
- 立即返回
- 启动 `board_build`

### 4.2 快路径保护阈值
即使本地数据已齐，也必须加保护：
- `成员数 > 20` 强制转后台
- 或 `预估重算 bars > 10_000` 强制转后台

要求：
- 阈值命中时，不允许因为“理论上本地有数据”而继续同步重算
- 前端必须能从返回结构明确识别后台路径

### 4.3 后台路径返回边界
后台路径返回前只允许执行：
- 参数校验
- 本地 SQLite 查询
- 生成 `buildJobId`
- 写入 `boards / board_members / boards.build_*`
- 将任务入队

后台路径返回前禁止执行：
- 任何 `Longbridge` 远端调用
- 最新交易日查询
- 静态信息刷新
- 历史日 K 拉取
- 板块历史重算
- 任何可能阻塞返回的重 CPU 计算

要求：
- 后台路径返回必须是“轻事务 + 入队”模型
- 旧版“先拉远端、再写库、再返回 UI”的串行路径不得复现

当前实现补充：
- `save_board()` 先统一成员去重、算法校验和本地 symbol 可用性检查，再基于历史完整度与保护阈值决定快路径或后台路径
- 命中后台路径时先把 `build_status / build_phase` 写成 `queued`，后续远端拉数、重算与落库全部放到异步 `board_build`

### 4.4 后台路径性能预算
`save_board()` 在后台路径下必须满足：
- 返回时间目标 `< 300ms`

说明：
- 这是 Step 3 的开发红线，不是只在 Step 5 验收时才检查。
- 若实现方案无法稳定满足该预算，则视为 Step 3 设计不合格。

## 5. `board_build` 状态机必须冻结
后端 Step 3 必须把板块构建状态定义清楚，不能边写边发明。

### 5.1 `build_status`
只允许：
- `queued`
- `running`
- `succeeded`
- `failed`

### 5.2 `build_phase`
只允许：
- `queued`
- `fetching_symbols`
- `fetching_history`
- `recomputing_board`
- `persisting`
- `completed`
- `failed`

### 5.3 状态推进规则
- 新任务创建后先写 `queued`
- 真正开始执行后写 `running`
- 全部完成后写 `succeeded + completed`
- 任一不可恢复错误写 `failed`

### 5.4 部分成功规则
若部分成分股失败：
- 允许板块构建最终成功，但必须有明确条件
- 若仍可生成可接受的板块结果，则：
  - `build_status = succeeded`
  - `build_failed > 0`
  - `build_message` 写清缺失情况
- 若有效成分股不足以生成板块结果，则：
  - `build_status = failed`

禁止：
- 部分失败时既不算成功也不算失败，长期停在 `running`

## 6. 状态持久化与恢复
### 6.1 状态必须落库
以下字段必须在 `boards` 中持续更新：
- `build_status`
- `build_phase`
- `build_total`
- `build_completed`
- `build_failed`
- `build_job_id`
- `build_message`
- `build_started_at`
- `build_finished_at`
- `updated_at`

### 6.2 重启恢复规则
应用重启后若存在：
- `queued`
- `running`

后端必须做以下二选一，且文档与实现保持一致：

推荐方案：
- 启动时统一标记为 `failed`
- `build_message` 写明“应用中断，任务未完成”

可选方案：
- 启动时扫描可恢复任务并重排队

首版更推荐前者，简单、稳定、容易验证。

禁止：
- 冷启动后把旧 `running` 任务静默丢失
- 冷启动后继续显示 `running`，但实际没有任务在跑

## 7. Longbridge 接入的高风险点
### 7.1 交易日与时区口径
这是 Step 3 最容易出错的地方之一。

要求：
- `latest_trade_date` 必须按美股市场日历口径判断
- 不能直接用本机本地日期代替交易日
- 必须明确处理：
  - 美东盘前
  - 美东盘中
  - 美东收盘后
  - 中国时区下跨日

禁止：
- 因时区误判导致多拉一天或少拉一天
- 把“今天”直接当成“最新交易日”

### 7.2 标的口径统一
历史日线、静态信息、指数和个股标识必须统一标准化。

要求：
- `symbol` 主键口径唯一
- 指数与个股在存储层有一致的目标标识方案
- `board_members` 存的必须是标准化后的 symbol

禁止：
- 一处用展示名，一处用交易代码
- 静态信息与 K 线数据对不上主键

### 7.3 远端异常分类
至少区分：
- 鉴权失败
- 限频
- 网络错误
- 空数据
- 单个 symbol 拉取失败

禁止：
- 所有远端错误都压成一个“unknown error”

### 7.4 增量同步锚点规则
`startup / manual` 默认必须按本地 `latest_trade_date` 做增量补缺。

固定规则：
- 以本地持久层中各目标的 `latest_trade_date` 为锚点
- 只请求缺失交易日区间
- 本地已存在且连续的历史区间不得重复全量拉取

仅以下情况允许 `full backfill`：
- 目标无本地历史
- 新增成分股首次接入
- 本地最早历史仍晚于目标回填窗口，导致 `3Y / all` 无法展示
- 检测到历史补洞
- 用户显式触发全量回补

禁止：
- 每次 `startup / manual` 都退回全量历史回补
- 因实现偷懒忽略本地 `latest_trade_date`

### 7.5 `latest_trade_date` 与静态信息复用
同一轮同步 / 同一 `board_build job` 内，必须复用已获取的市场侧元数据。

要求：
- 同一 job 内 `latest_trade_date` 只获取一次并复用
- 同一 job 内同一 symbol 的静态信息不得重复请求
- `static_info` 必须有明确刷新频次，禁止每次构建都强制刷新

推荐：
- `static_info` 采用按日或按版本刷新
- 若本地缓存仍有效，则直接复用

当前实现补充：
- `run_sync()` 与 `board_build` 都在单 job 内只获取一次 `latest_trade_date` 并复用到后续历史请求
- `static_info` 仅对本地已过期的 symbol 刷新；仍在有效期内的记录直接复用，不在每次构建里强制刷新
## 8. `board_build` 执行模型
### 8.1 固定规则
- 每批 `5` 个成分股
- 批内最多 `3` 并发
- 同一时刻只允许 `1` 个板块构建处于 `running`
- 首建默认至少回填最近 `3Y/day`
- 若本地已有历史但最早交易日仍晚于 `3Y` 窗口，必须判定为“历史覆盖不足”并补齐
- `all` 口径允许长于 `3Y`，但不得短于 `3Y` 基线
- `startup/manual` 与 `board_build` 必须共用同一套“历史覆盖不足”判断，避免 symbol / board 口径漂移

### 8.2 板块重算策略
板块重算默认必须走尾部增量，不得动辄整段全量重建。

默认策略：
- 若成分股集合未变、算法未变、历史无补洞，则优先 `tail append`
- 日常增量同步后，优先只追加新增交易日对应的板块 bar

仅以下情况允许 `full rebuild`：
- 板块成员变更
- 板块算法切换
- 检测到历史补洞
- 明确执行历史修复

禁止：
- 每批次 symbol 完成后都对整个板块历史做一次全量重算
- 无差别把所有板块更新都实现成整段重建

### 8.3 批处理原则
- 每批统一拉取
- 每批统一入库
- 每批统一更新进度

禁止：
- 每只股票拉一次就立刻单独写库
- 每只股票处理完就推一次事件
- 每只股票都触发一次板块重算

当前实现补充：
- 当前实现按 `members.chunks(5)` 固定分批、批内通过 `Semaphore(3)` 控制并发，并通过运行时 `board_build_gate` 保证同一时刻仅 `1` 个正式 `board_build`

### 8.4 job 级去重规则
Step 3 必须冻结 job 级去重规则，不能只写“尽量避免重复请求”。

至少要求：
- 同一 `board_build job` 内同一 symbol 只允许进入一次拉取计划
- 同一 `board_build job` 内 `latest_trade_date` 只计算一次
- 同一 `board_build job` 内同一 symbol 的 `static_info` 最多刷新一次
- 同一批次内重复出现的 symbol 必须先去重再发起远端请求

### 8.5 进度更新节流
后端应按批次或阶段更新进度，不按单 symbol 高频推送。

建议：
- 进入新 phase 推一次
- 每批结束推一次
- 最终完成或失败再推一次

## 9. SQLite 与事务风险控制
### 9.1 事务原则
- 批量写必须统一事务
- 板块结果写入必须成组提交
- 禁止每个 symbol 独立事务

### 9.2 读写冲突控制
Step 3 会同时出现：
- `sync`
- `board_build`
- `get_chart`

要求：
- 长事务要尽量缩短
- 网络请求不能包在数据库写事务里
- 重 CPU 计算不能长时间占住写事务

推荐执行顺序：
1. 拉远端数据
2. 内存标准化
3. 开事务批量写库
4. 事务提交后再做必要缓存更新与事件发送

### 9.3 WAL 与 busy 处理
- 必须开启 `WAL`
- 必须有明确的 busy / retry 策略
- 出现锁等待时要记录日志，便于定位长事务

## 10. 图表生成与缓存坑位
### 10.1 图表数据生成边界
后端统一负责：
- 历史范围裁剪
- 周 K 聚合
- 板块 K 线生成
- payload 标准化

前端不允许重复做这些计算。

### 10.2 周 K 边界统一
Step 3 必须冻结周 K 规则：
- 周起止边界统一
- 同一 symbol 在不同请求中不能出现不同聚合结果
- 板块周 K 必须由板块日 K 再聚合，不能临时改为“成员股周 K 后再聚合”而口径漂移

### 10.3 缓存分层
必须至少实现：
- 原始日线序列缓存
- 周线聚合缓存
- 图表 payload 缓存

### 10.4 缓存 key 组成
缓存 key 维度必须冻结，避免不同图表口径相互污染。

图表 payload 缓存 key 至少必须包含：
- `targetType`
- `targetId`
- `range`
- `granularity`
- `boardAlgorithm`

若存在盘中 overlay、不同来源状态或其它会影响结果的维度，也必须进入 key 或版本戳。

禁止：
- 只用 `targetId` 做图表缓存 key
- `day / week`
- 不同 `range`
- 不同 `boardAlgorithm`
  共享同一条缓存记录

### 10.5 缓存失效规则
只允许失效：
- 当前板块缓存
- 本次受影响 symbol 缓存
- 与当前目标直接相关的聚合缓存

禁止：
- 全量 `cache.clear()`
- 任意板块更新后把所有图表缓存作废

当前实现补充：
- 运行时通过 `invalidate_targets()` 只清受影响 symbol / board 的原始日线、周线、图表 payload 和 live overlay 缓存，不做全局清空

### 10.6 重复计算防护
必须避免：
- 同一 symbol 在同一任务里重复请求
- 同一板块历史在同一流程中重复重算
- `get_chart()` 命中缓存失败后又在多层重复聚合

## 11. 对外接口一致性要求
### 11.1 `get_board_build_status()` 与事件同构
`get_board_build_status()` 返回字段必须与 `board-build-status` event 尽量同构，至少包含：
- `boardId`
- `name`
- `buildJobId`
- `buildStatus`
- `buildPhase`
- `buildTotal`
- `buildCompleted`
- `buildFailed`
- `buildMessage`
- `updatedAt`

禁止：
- 查询接口和事件接口两套完全不同字段命名

### 11.2 事件顺序要求
后端不能假设事件到达顺序绝对可靠。

要求：
- 每条状态事件都带 `buildJobId`
- 每条状态事件都带 `updatedAt`
- 最终完成和失败事件必须明确可判定

### 11.3 完成后的图表刷新契约
后端不负责强推整页刷新，但要为前端提供足够信息判断是否刷新当前图。

推荐：
- `succeeded` 事件发完后，前端自行按当前激活目标决定是否补拉 `get_chart()`

## 12. 错误处理与可观测性
至少记录以下日志：
- `save_board()` 路径选择
- `board_build` 任务创建 / 开始 / 完成 / 失败
- 每批次 symbol 数量与耗时
- SQLite 写事务耗时
- 缓存命中 / 失效摘要
- Longbridge 请求失败分类

禁止：
- 构建失败只返回给前端，不落后端日志

## 13. 测试与验收重点
至少覆盖：
- 真实历史日 K 拉取
- 最新交易日判定
- `startup` 增量同步
- `startup / manual` 只补缺失区间
- 周 K 聚合口径一致
- `save_board()` 快路径
- `save_board()` 后台路径
- 后台路径返回前无远端请求
- 后台路径返回 `< 300ms`
- 大板块后台构建不阻塞
- 板块默认尾部增量重算
- 成员变更或补洞触发全量重建
- job 级 symbol 去重
- 板块部分成功
- 板块失败恢复
- 冷启动后旧 `running` 状态处理
- 缓存 key 隔离正确
- 缓存命中与细粒度失效
- SQLite 锁冲突下的稳定性

## 14. 本步骤禁止项
- `save_board()` 同步拉完整历史再返回
- 后台路径返回前发起任何远端请求
- 每只股票单独提交事务
- 因共用 symbol 重算所有关联板块
- 把网络拉取、重算、写库全部包成一个长阻塞调用
- 没有 `build_job_id` 就推状态事件
- 没有 `updatedAt` 就让前端判断新旧状态
- 构建完成后直接触发全量缓存清空
