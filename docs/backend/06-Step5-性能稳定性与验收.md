# 后端 Step 5：性能稳定性与验收

## 1. 目标
本步骤不是再发明新功能，而是对 Step 1 到 Step 4 已冻结的后端契约、性能红线和异常恢复规则做最终收口：
- 把旧版本最容易回归的性能坑逐项压实
- 把异常恢复、日志、测试、打包前检查补齐
- 让后端达到“可联调、可回归、可打包、可交付”的状态

Step 5 的核心原则只有两条：
- 不允许为了“优化”而破坏 Step 1 到 Step 4 已冻结的接口和语义
- 不允许把问题留到人工体验阶段才发现，必须通过量化指标、回归测试和可观测性提前拦截

## 2. 本步骤范围

### 2.1 必须完成
- Step 1 到 Step 4 契约漂移复查
- 性能基线测量与收敛
- Step 3 / Step 4 关键回归矩阵补齐
- 异常恢复验证
- 日志与可观测性补齐
- release 打包前专项预检
- 面向前端 / QA 的验收证据整理

### 2.2 本步骤不做
- 新增 command / event
- 变更 Step 1 已冻结的模块边界
- 改写 Step 3 / Step 4 的业务语义
- 引入新的实时行情方案
- 临时删减字段绕过前后端契约问题

## 3. Step 5 前置冻结项
Step 5 默认 Step 1 到 Step 4 已冻结以下红线，收口阶段只能验证、补测试、修实现，不能擅自改契约。

### 3.1 Step 1 契约冻结
不得改名：
- `bootstrap`
- `save_credentials`
- `get_sync_status`
- `run_sync`
- `get_chart`
- `save_board`
- `get_board_build_status`
- `get_target_note`
- `save_target_note`
- `open_settings_window`
- `close_settings_window`
- `start_chart_watch`
- `stop_chart_watch`

不得改名：
- `sync-status`
- `board-build-status`
- `chart-live-update`
- `settings-saved`

### 3.2 Step 2 本地正式路径冻结
必须继续满足：
- `fixture -> SQLite -> repository -> service / chart_engine -> commands -> frontend`
- command 层不直接拼样例业务对象
- `AppState` 不承担正式业务持久化
- `boards.build_*`、`target_notes`、`sync_jobs` schema 已经稳定存在

### 3.3 Step 3 性能与一致性红线
必须继续满足：
- `save_board()` 后台路径返回前禁止任何远端请求
- 后台路径必须保持“轻事务 + 入队”
- `save_board()` 后台路径返回目标 `< 300ms`
- `startup / manual` 默认按 `latest_trade_date` 做增量补缺
- 板块重算默认 `tail append`
- job 级 `symbol / latest_trade_date / static_info` 去重
- 图表缓存 key 必须包含 `targetType + targetId + range + granularity + boardAlgorithm`
- 禁止全量 `cache.clear()`
- `board-build-status` / `get_board_build_status()` 必须继续带 `buildJobId`、`updatedAt`

### 3.4 Step 4 watch 红线
必须继续满足：
- 同一时刻只允许 `1` 个活动 watch
- `start_chart_watch()` / `stop_chart_watch()` 快返回
- `start_chart_watch()` 不等待远端 `market_status()` 或首轮 quote
- `range` 不进入 `start_chart_watch(payload)`，也不进入 `chart-live-update`
- `chart-live-update` 必须带 `watchId`、`updatedAt`
- 同一 `watchId` 下 `updatedAt` 严格递增
- `updatedAt` 必须以后端本地发送时刻或版本戳生成，不直接复用不稳定的远端原始时间字段
- `started = false, marketState = closed` 是正常业务分支
- `week` 视图不启用盘中 watch
- overlay 只作为内存层，不污染正式历史缓存
- 已移除 sample / scaffold live event 路径
- `save_credentials()` 后 provider cache 必须失效并重新预热

### 3.5 当前实现补充红线
Step 5 还必须继续满足：
- 本地市场时段判断优先走 `America/New_York` 本地规则日历
- 本地规则日历必须覆盖周末、主要休市日和半日市
- provider 预热不允许反向污染 `start / stop` SLA
- `QuoteContext` 冷启动只能在后台 warm path 体现，不能回流到 command 快返回路径

## 4. 收口实施顺序
后端工程师在 Step 5 固定按以下顺序推进，不要跳步：

1. 先确认 Step 1 到 Step 4 契约没有漂移
2. 再测性能基线，明确冷路径 / 热路径
3. 再补缺失的回归测试和异常测试
4. 再补日志、指标和排障信息
5. 最后做 release 打包前专项预检

如果第 1 步就发现契约漂移，应先修实现，不允许把问题带入性能测量。

## 5. 性能测量方法必须冻结

### 5.1 测量环境
性能数据必须至少区分以下环境：
- debug / dev：只用于日常开发定位
- release：作为正式验收口径

必须记录：
- 测试日期
- macOS 版本
- 机器型号与芯片
- 是否真实 Longbridge 凭证
- 数据库状态：空库 / 已有历史 / 已预热
- 是否冷启动

### 5.2 冷路径与热路径必须分开
至少区分：

1. 冷路径
- 首次启动
- provider 未预热
- 本地缓存未命中

2. 热路径
- provider 已预热
- 正式缓存已建立
- 数据库已有历史数据

禁止：
- 把冷启动耗时和热路径耗时混成一个数字
- 用一次偶然快结果代替稳定基线

### 5.3 统计口径
建议至少记录：
- `median`
- `p95`
- 最小 / 最大值
- 样本数

要求：
- 每项核心指标至少连续测 `10` 次
- 明显异常值要单独记录原因，不允许直接静默丢弃

## 6. 必测性能指标

### 6.1 `save_board()` 后台路径
必须验证：
- 返回前无远端请求
- 返回路径只做本地查询、落库、写状态、入队
- release 环境下返回目标 `< 300ms`

必须补证据：
- 耗时数据
- 对应日志
- 若超标，明确卡点在参数校验 / SQLite / 入队 / 其它哪一层

### 6.2 `get_chart()` 缓存命中路径
必须验证：
- 缓存命中后快速返回
- `day / week / range / boardAlgorithm` 互不污染
- live overlay 不污染正式历史缓存

建议目标：
- release 环境缓存命中路径 `< 100ms`

### 6.3 `startup / manual` 同步
必须验证：
- 默认按 `latest_trade_date` 增量补缺
- 本地连续历史不被重复全量拉取
- 首次接入或历史覆盖不足时，symbol 至少补齐到 `3Y` 窗口
- 只有无历史 / 新成员 / 历史覆盖不足 / 补洞 / 明确全量回补才走 `full backfill`

必须补证据：
- 请求区间日志
- 补数前后本地 `latest_trade_date`
- 是否触发 `tail append` 或 `full rebuild`

### 6.4 `board_build`
必须验证：
- 批大小固定 `5`
- 批内最多 `3` 并发
- 同一时刻只允许 `1` 个 `board_build` 处于 `running`
- 首建至少回填 `3Y` 历史，不能只落约 `1Y`
- 默认走 `tail append`
- 只有成员变更 / 算法切换 / 补洞 / 历史修复才允许 `full rebuild`
- job 级 symbol 去重有效

必须补证据：
- 批次日志
- 单 job 内 symbol 去重前后数量
- `buildJobId`、`updatedAt` 状态推进日志

### 6.5 `start_chart_watch()` / `stop_chart_watch()`
必须验证：
- command 层快返回
- 不等待远端 `market_status()` 或首轮 quote
- 单活动 watch 不漂移
- provider 预热生效
- 本地市场时段判断不会阻塞 command 返回

目标：
- `start_chart_watch()` 返回 `< 100ms`
- `stop_chart_watch()` 返回 `< 100ms`

### 6.6 首条 `chart-live-update`
必须区分：
1. 冷路径首条 live event
2. provider 预热后的 warm path 首条 live event

要求：
- warm path 明显优于冷路径
- 冷启动慢只允许体现在后台 warm path，不允许反向污染 command 快返回
- 同一 `watchId` 下 `updatedAt` 保持严格递增

### 6.7 30 分钟持续运行稳定性
必须验证：
- `sync + board_build + watch` 并发存在时无明显资源泄漏
- 盘中更新持续 `30` 分钟后无明显内存上涨
- 无事件风暴
- 无多余 SQLite 写入

必须补证据：
- 稳定性场景说明：目标组合、是否真实凭证、是否冷启动、是否 release
- 至少记录 `0 / 10 / 20 / 30` 分钟的进程内存或 RSS 采样
- 记录 `chart-live-update` 推送总数、异常突刺时刻与原因
- 记录 SQLite 写入摘要或 `WAL` / 数据文件增长摘要
- 若判定“无明显上涨 / 无事件风暴 / 无多余写入”，必须给出对应采样表或日志摘录

## 7. 回归验证矩阵

### 7.1 Step 2 回归
至少覆盖：
- SQLite schema 与迁移正常
- fixture 幂等导入
- `target_notes` 重启后恢复
- `boards.build_*` 字段完整存在

### 7.2 Step 3 回归
至少覆盖：
- `save_board()` 快路径保持可用
- `save_board()` 后台路径返回前无远端请求
- 后台路径返回 `< 300ms`
- `startup / manual` 只补缺失区间
- job 级 symbol 去重
- `tail append / full rebuild` 触发条件正确
- 冷启动后旧 `queued / running` 状态处理符合文档
- `board-build-status` 与 `get_board_build_status()` 字段同构
- 局部缓存失效正确
- 不出现全量 `cache.clear()`

### 7.3 Step 4 回归
至少覆盖：
- 同时只能存在 `1` 个活动 watch
- 重复 `start` 复用同一 `watchId`
- `stop_chart_watch()` 幂等
- `granularity = week` 返回明确 `WATCH_UNSUPPORTED_GRANULARITY`
- `started = false, marketState = closed` 走正常业务分支
- `chart-live-update` 带 `watchId`、`updatedAt`
- 同一 `watchId` 下 `updatedAt` 严格递增
- `range` 不进入 `start_chart_watch(payload)` 和 `chart-live-update`
- `day` 合并 overlay、`week` 不合并 overlay
- 切目标 / 后台化 / 隐藏窗口后旧 watch 停止
- 晚到事件不会串到新目标
- `save_credentials()` 后 provider cache 失效并重新预热
- 不再存在 sample / scaffold live event 路径

### 7.4 本地市场时段与 provider 回归
至少覆盖：
- 周末返回 `market closed`
- 主要休市日返回 `market closed`
- 半日市规则正确
- 本地市场时段判断不依赖远端 `market_status()`
- provider 预热失败不阻塞 `start / stop` 快返回

### 7.5 跨模块并发回归
至少覆盖：
- `run_sync()` 运行时创建板块
- `board_build` 运行时切图
- `watch` 运行时切换目标
- `watch + board_build + get_chart` 同时发生
- 连续快速 `start / stop watch`
- 快速重复创建多个大板块

## 8. 异常恢复矩阵

### 8.1 凭证错误
必须验证：
- `save_credentials()` 错误可识别
- provider cache 不继续持有旧凭证实例
- 预热失败不阻塞后续重试
- 日志不打印真实凭证

### 8.2 远端限频
必须验证：
- 后端能明确区分限频
- 不会引发无限重试
- 不会导致事件风暴或 UI 卡死

### 8.3 网络中断
必须验证：
- 单轮失败不会清空现有 overlay
- `board_build / sync / watch` 的失败状态可查询
- 恢复联网后可重新发起任务

### 8.4 单个 symbol 拉数失败
必须验证：
- 单 symbol 失败不会导致整个 job 永远卡在 `running`
- 部分成功时 `buildFailed` 与 `buildMessage` 可用
- 有效成分不足时任务转 `failed`

### 8.5 应用休眠 / 崩溃恢复
必须验证：
- 休眠后旧 watch 不继续推脏事件
- 冷启动后旧 `running` 任务不假装仍在运行
- 重启后的状态与文档约定一致

## 9. 可观测性与排障要求

### 9.1 必须有的日志
至少包括：
- `save_board()` 路径选择与耗时
- `board_build` 任务创建 / 开始 / 完成 / 失败
- 每批 symbol 数量、去重结果与耗时
- SQLite 写事务耗时
- 缓存命中 / 失效摘要
- `start_chart_watch()` / `stop_chart_watch()` 参数摘要与结果
- 本地 market status 判定结果
- provider `prewarm` 触发来源、是否命中 in-flight 去重、开始 / 成功 / 失败 / 耗时
- `chart-live-update` 推送次数
- 连续失败次数与降级状态

### 9.2 禁止项
- 失败只在前端看到，后端无日志
- 性能问题没有耗时分段
- 凭证、token、真实密钥被打到日志

## 10. 打包前专项预检

### 10.1 数据与迁移
必须验证：
- 从已有用户数据库升级不会破坏旧数据
- 迁移后 `boards.build_*`、`sync_jobs`、`target_notes`、`board_daily_bars` 结构完整
- `WAL` 在打包后的运行目录仍然开启

### 10.2 Keychain 与权限
必须验证：
- 打包后 Keychain 读写可用
- 凭证更新后 runtime provider cache 正常失效
- 日志中仍不输出敏感信息

### 10.3 macOS 打包链路
必须验证：
- `.app / .dmg` 可产出
- 签名可通过
- notarization 可通过
- Gatekeeper 首开行为正常

### 10.4 release 环境冒烟
至少执行：
- 首次启动
- 保存凭证
- `bootstrap()`
- `get_chart(day / week)`
- 新建单成员板块
- 新建大板块后台构建
- `run_sync(startup / manual)`
- `start_chart_watch()` / `stop_chart_watch()`

## 11. 后端测试要求

### 11.1 自动化测试
至少补齐：
- 单元测试
- `repository / chart_engine / service` 集成测试
- 关键命令回归测试
- 基于 fake provider / stub / 半真数据夹具的性能回归测试

要求：
- 自动化测试默认必须可由 `cargo test` 稳定执行
- 自动化测试不依赖真实 Longbridge 凭证、真实网络状态或盘中时段

### 11.2 真实凭证烟测与性能样本
真实 Longbridge 凭证验证属于手工验收样本，不并入默认自动化测试。

至少应单独保留：
- 真实凭证 release 烟测记录
- 冷路径 / warm path 性能样本
- 首条 `chart-live-update` 冷 / 热差异样本
- 如受市场时段限制，必须记录实际测试日期、时区和市场状态

### 11.3 建议执行命令
开发完成后至少执行：
- `cargo fmt --check`
- `cargo test`

如已有 release 性能样本或打包前验证需要，再执行：
- `cargo test --release`

如已接入 `clippy`，再执行：
- `cargo clippy --all-targets -- -D warnings`

### 11.4 需要保留的验收证据
至少整理：
- 性能测试结果表
- 关键回归测试清单
- 异常恢复验证记录
- 打包验证记录
- 一份面向前端 / QA 的已知限制说明

## 12. 后端交付物
Step 5 完成后，后端交付必须至少包含：
- `new_stock/src-tauri/`
- 可运行的 Rust 核心服务
- 稳定的 SQLite 数据层
- 同步 / `board_build` / watch 任务层
- 缓存层
- 后端测试
- 性能与回归验证记录

## 13. 验收标准

### 13.1 性能
- `save_board()` 后台路径返回 `< 300ms`
- `get_chart()` 缓存命中路径稳定快速返回
- `start_chart_watch()` 返回 `< 100ms`
- `stop_chart_watch()` 返回 `< 100ms`
- warm path 首条 live event 显著优于冷路径

### 13.2 稳定性
- 大板块构建期间 UI 可操作
- `sync` 和 `watch` 不阻塞主流程
- 连续 `30` 分钟 watch 无明显资源泄漏
- 应用休眠 / 恢复后状态不串线

### 13.3 一致性
- Step 1 到 Step 4 契约无漂移
- `buildJobId`、`watchId`、`updatedAt` 语义保持稳定
- `range` 未重新混入 watch 生命周期
- 图表缓存、overlay、板块状态字段口径一致

### 13.4 可交付性
- 后端达到可联调、可测试、可打包状态
- QA 可以依据文档直接执行回归
- 前端不需要额外兼容后端字段漂移

## 14. 本步骤禁止项
- 为了赶验收临时改 command / event 名称
- 为了“优化”把远端调用放回 `save_board()` 返回路径
- 为了“省事”重新引入全量 `cache.clear()`
- 为了“补救”在前端新增临时业务聚合逻辑
- 为了“打包成功”跳过 Keychain / 迁移验证
- 为了“看起来稳定”屏蔽错误日志或吞掉失败状态
- 没有量化数据就宣称性能达标
- 保留 sample / scaffold live event 路径却宣称 Step 4 / Step 5 已完成
