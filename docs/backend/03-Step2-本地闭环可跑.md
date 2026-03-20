# 后端 Step 2：本地闭环可跑

## 1. 完成定义
本步骤完成后，前端必须可以在不接入真实 `Longbridge` 的前提下，通过真实 `Tauri commands + SQLite + 本地 fixture` 跑通主流程联调。

Step 2 的目标不是继续靠硬编码样例“凑演示”，而是把后端本地数据闭环先搭起来，为 Step 3 的真实行情接入、后台任务和缓存系统提供稳定底座。

完成后必须达到：
- 主窗口通过真实 command 从 SQLite 读取本地数据
- 板块创建可落库，并在应用重启后恢复
- 笔记可落库，并在应用重启后恢复
- 图表数据先从 SQLite 取出，再经 `chart_engine` 组装
- 前端不需要额外 mock bridge
- `commands` 层不再依赖硬编码业务常量

## 2. 本步骤范围
### 2.1 必须完成
- SQLite schema 初始化与迁移
- `repository` 真实读写实现
- dev fixture 导入机制
- 本地图表生成链路
- 以下 commands 的本地可用实现：
  - `bootstrap`
  - `get_chart`
  - `save_board`
  - `get_board_build_status`
  - `get_target_note`
  - `save_target_note`
  - `open_settings_window`
  - `close_settings_window`

### 2.2 本步骤不做
- 真实 `Longbridge` 接入
- 真正执行的后台 `board_build` 任务
- 真实 `sync_jobs` 调度流水线
- 盘中 `watch`
- 后端性能优化专项

说明：
- Step 2 允许保留后台路径的“契约占位”和“状态落库”，但不要求真正启动异步构建任务。
- Step 2 不允许为了省事走错误的数据路径设计，例如：command 直接返回硬编码样例、业务主数据只存在内存。

## 3. 固定执行路径与模块边界
Step 2 的业务数据固定来自 SQLite。

固定执行路径为：

`fixture -> SQLite -> repository -> service / chart_engine -> commands -> frontend`

硬性要求：
- 不允许 command 直接返回硬编码 fixture 常量
- 不允许 command 层直接拼业务对象绕过 `repository`
- 不允许 `AppState` 充当正式业务数据库
- fixture 只作为 SQLite 初始数据来源，不得成为 command 的直接返回源

### 3.1 模块职责
- `repositories/`
  - 负责 schema 初始化、迁移、fixture 导入、所有 SQLite 读写
- `services/`
  - 负责 `bootstrap` 编排、板块保存、笔记保存、目标查询
- `chart_engine/`
  - 负责日线裁剪、周 K 聚合、板块图表生成
- `commands/`
  - 只负责参数接收、service 调用、结果返回、错误映射

### 3.2 `AppState` 边界
`AppState` 只允许保存：
- 瞬时窗口态
- 临时任务句柄
- 进程内短生命周期缓存

`AppState` 不允许保存：
- 板块主数据
- 笔记正式数据
- 图表正式数据
- `board_build` 持久状态

原则：
- 只要数据需要跨重启恢复，就必须落 SQLite，不得只存在内存。

## 4. fixture 数据策略
### 4.1 fixture 存放位置
Step 2 的 dev fixture 固定放在：

`new_stock/src-tauri/dev-fixtures/step2/`

推荐至少包含：
- `seed.json`
- 或按表拆分的 `symbols.json / daily_bars.json / boards.json / board_members.json / target_notes.json`

### 4.2 fixture 导入触发规则
- debug / dev 环境允许空库自动导入 fixture
- release 环境不自动导入 fixture
- 自动导入只允许触发一次初始化逻辑
- 是否已导入过 fixture，必须记录在 `app_settings`

推荐做法：
- 使用 `app_settings` 中的 `dev_fixture_version`
- 若当前值为空，且环境为 debug / dev，则导入 `step2_v1`
- 若值已是 `step2_v1`，重复启动不得再次产生重复数据

### 4.3 fixture 幂等要求
- 同一份 fixture 重复导入不能产生重复 row
- `symbols` 不得重复
- `daily_bars` 不得重复
- `boards` 不得重复
- `board_members` 不得重复
- `target_notes` 不得重复

要求：
- 所有 fixture 导入写操作必须基于主键或唯一键做 `upsert` 或 `insert or ignore`
- 空库自动导入只能触发一次初始化逻辑，不能每次启动都重复装载

### 4.4 fixture 最小覆盖范围
Step 2 的 fixture 至少要覆盖：
- 四大指数：`DJI / IXIC / GSPC / RUT`
- 至少 `1` 个默认板块
- 至少 `3` 只默认个股
- 至少 `1` 条默认笔记
- 至少能支持 `day / week` 切图验证的连续日线

建议：
- 每个默认目标至少准备 `260` 个交易日的日线
- 这样可以稳定验证 `1m / 3m / 6m / 1y / all` 的裁剪行为

## 5. 数据库要求
Step 2 必须完成以下表的真实建表与最小字段集。

### 5.1 `app_settings`
- `key TEXT PRIMARY KEY`
- `value TEXT NOT NULL`
- `updated_at TEXT NOT NULL`

### 5.2 `symbols`
- `target_id TEXT PRIMARY KEY`
- `target_type TEXT NOT NULL`
- `display_code TEXT NOT NULL`
- `name TEXT NOT NULL`
- `market TEXT`
- `security_type TEXT NOT NULL`
- `currency TEXT`
- `total_shares REAL`
- `circulating_shares REAL`
- `updated_at TEXT NOT NULL`

说明：
- Step 2 的四大指数也要作为正式目标写入该表
- 推荐用 `target_type = index | symbol` 区分

### 5.3 `daily_bars`
- `target_id TEXT NOT NULL`
- `trade_date TEXT NOT NULL`
- `open REAL NOT NULL`
- `high REAL NOT NULL`
- `low REAL NOT NULL`
- `close REAL NOT NULL`
- `volume REAL`
- `source TEXT NOT NULL`
- `updated_at TEXT NOT NULL`
- 主键：`(target_id, trade_date)`

说明：
- Step 2 的指数日线与个股日线统一走该表
- `get_chart(index)` 不允许走单独硬编码分支

### 5.4 `boards`
- `board_id TEXT PRIMARY KEY`
- `name TEXT NOT NULL UNIQUE`
- `sort_order INTEGER NOT NULL`
- `composition_algorithm TEXT NOT NULL`
- `build_status TEXT NOT NULL`
- `build_phase TEXT NOT NULL`
- `build_total INTEGER NOT NULL`
- `build_completed INTEGER NOT NULL`
- `build_failed INTEGER NOT NULL`
- `build_job_id TEXT`
- `build_message TEXT`
- `build_started_at TEXT`
- `build_finished_at TEXT`
- `created_at TEXT NOT NULL`
- `updated_at TEXT NOT NULL`

说明：
- `boards.build_*` 字段必须在 Step 2 一次性建齐，不能等到 Step 3 再补
- 后台路径占位也必须依赖这些字段持久化

### 5.5 `board_members`
- `board_id TEXT NOT NULL`
- `target_id TEXT NOT NULL`
- `sort_order INTEGER NOT NULL`
- 主键：`(board_id, target_id)`

### 5.6 `board_daily_bars`
- `board_id TEXT NOT NULL`
- `composition_algorithm TEXT NOT NULL`
- `trade_date TEXT NOT NULL`
- `open REAL NOT NULL`
- `high REAL NOT NULL`
- `low REAL NOT NULL`
- `close REAL NOT NULL`
- `volume REAL`
- `updated_at TEXT NOT NULL`
- 主键：`(board_id, composition_algorithm, trade_date)`

说明：
- Step 2 若已支持板块出图，就必须能区分不同算法的板块历史

### 5.7 `target_notes`
- `target_type TEXT NOT NULL`
- `target_id TEXT NOT NULL`
- `content TEXT NOT NULL`
- `created_at TEXT NOT NULL`
- `updated_at TEXT NOT NULL`
- 主键：`(target_type, target_id)`

说明：
- `save_target_note` 必须基于该唯一键做 `upsert`

### 5.8 `sync_state`
- `target_type TEXT NOT NULL`
- `target_id TEXT NOT NULL`
- `latest_trade_date TEXT`
- `last_sync_at TEXT`
- `last_sync_status TEXT`
- `last_error_code TEXT`
- `last_error_message TEXT`
- 主键：`(target_type, target_id)`

### 5.9 `sync_jobs`
- `job_id TEXT PRIMARY KEY`
- `mode TEXT NOT NULL`
- `status TEXT NOT NULL`
- `started_at TEXT NOT NULL`
- `finished_at TEXT`
- `summary_json TEXT`
- `error_json TEXT`

说明：
- `sync_jobs` 在 Step 2 可暂不承载真实任务写入
- 但 schema 必须先到位，避免 Step 3 再做破坏式迁移

### 5.10 SQLite 原则
- 开启 `WAL`
- 所有写操作必须包事务
- `repository` 成为唯一数据访问入口
- 禁止每条业务数据单独 `commit`
- 禁止前端直接访问 SQLite

## 6. Step 2 服务层实现要求
Step 2 必须最少形成以下后端服务职责：

### 6.1 `BootstrapService`
- 读取四大指数、板块列表、成员映射、当前 note、同步状态
- 统一组装 `bootstrap()` 所需结果

### 6.2 `ChartService`
- 根据 `targetType / targetId / granularity / range / boardAlgorithm` 读取与生成图表
- 个股/指数：从 `daily_bars` 裁剪
- 周 K：从日线聚合
- 板块：从 `board_daily_bars` 读取；缺失时允许走本地即时重算

### 6.3 `BoardService`
- 校验 `save_board()` 入参
- 写入 `boards` 与 `board_members`
- 按双路径规则落库存状态
- 快路径时同步生成 `board_daily_bars`

### 6.4 `NoteService`
- 基于 `target_notes` 做读取与 `upsert`

## 7. Commands 实现要求
### 7.1 `bootstrap`
- 必须从 SQLite 读取主窗口所需基础数据
- 至少覆盖：`boards`、`board_members`、`sync_state`
- `activeTargetNote` 必须来自 `target_notes`
- 不允许返回硬编码 board / index 样例
- 若 dev fixture 已启用，必须先导入 fixture 再读取

### 7.2 `get_chart`
- 必须支持：
  - `targetType`
  - `targetId`
  - `granularity`
  - `range`
  - `boardAlgorithm`
- Step 2 可以完全基于 fixture 数据计算
- 但必须真实走 `repository + chart_engine + service` 路径
- 前端不得自己聚合周 K 或板块 K 线

#### `get_chart` 默认与错误语义
- 参数缺省：
  - `granularity` 缺省为 `day`
  - `range` 缺省为 `all`
  - `boardAlgorithm` 缺省为 `equal_weight_v1`
- 目标合法但当前范围无数据：
  - 返回成功结果
  - `bars = []`
  - `latestTradeDate = null`
  - `sourceStatus = "empty"`
- `targetId` 不存在或 `targetType` 非法：
  - 返回明确的业务错误
  - 不允许伪造默认图表
- `granularity / range / boardAlgorithm` 不支持：
  - 返回明确的参数错误

### 7.3 `save_board`
- 必须真实写入 `boards` 和 `board_members`
- 保存后重新 `bootstrap()` 必须能读到新板块
- 返回结构必须完整保留正式契约字段：
  - `boardId`
  - `rebuildStarted`
  - `backgroundSyncStarted`
  - `buildStatus`
  - `buildPhase`
  - `buildJobId`
  - `compositionAlgorithm`

#### Step 2 的双路径规则
Step 2 不执行真实后台任务，但业务语义必须先固定。

快路径触发条件：
- 所有成员股在本地已有日线
- 成员数 `<= 20`
- 预估重算 bars 不超过 `10_000`

快路径必须做到：
- 立即计算 `board_daily_bars`
- 将 `boards.build_status = succeeded`
- 将 `boards.build_phase = completed`
- 保存后可直接出图

后台路径占位触发条件：
- 任一成员缺本地日线
- 或成员数 `> 20`
- 或预估重算 bars `> 10_000`

后台路径占位必须做到：
- 不真正启动异步任务
- 但必须把 `boards.build_*` 字段真实落库
- 返回 `backgroundSyncStarted = true`
- `buildStatus = queued`
- `buildPhase = queued`
- `buildTotal = 成员数`
- `buildCompleted = 0`
- `buildFailed = 0`
- `buildJobId` 使用稳定占位格式，例如 `step2-placeholder-<uuid>`
- `buildMessage` 写明“等待 Step 3 后台构建”
- `updatedAt` 必须有值，供 Step 3 前端做状态去重

### 7.4 `get_board_build_status`
- 必须从 SQLite 读取 `boards.build_*` 状态
- 不允许退回 `AppState` 内存默认值
- 若 `boardId` 不存在，返回明确的业务错误
- 返回 payload 至少必须包含：
  - `boardId`
  - `name`
  - `buildStatus`
  - `buildPhase`
  - `buildTotal`
  - `buildCompleted`
  - `buildFailed`
  - `buildJobId?`
  - `buildMessage?`
  - `updatedAt`

### 7.5 `get_target_note` / `save_target_note`
- 必须真实读写 `target_notes`
- `save_target_note` 必须是 `upsert`
- 重启应用后内容可恢复
- 不允许把 note 临时存在 `AppState`

### 7.6 `open_settings_window` / `close_settings_window`
- 保留真实 command 封装路径
- command 层只负责窗口调度，不夹带业务假数据

## 8. 图表与板块口径
### 8.1 四大指数本地口径
Step 2 的四大指数必须走正式 SQLite 路径。

要求：
- `bootstrap()` 返回的四大指数必须来自 `symbols`
- `get_chart(index)` 必须从 `daily_bars` 读取
- 不允许在 `commands` 中写死指数 bars

### 8.2 日 K / 周 K
- `day` 数据来自 `daily_bars`
- `week` 数据必须由后端统一聚合
- 周 K 聚合结果不得由前端生成

### 8.3 板块算法
Step 2 的板块算法口径必须先固定：
- `equal_weight_v1` 必须可用
- `market_cap_weight_v1` 由本地 fixture 的股份数据驱动
- 若 Step 2 暂不实现市值加权真实计算，则必须返回明确业务错误，不能静默回退成等权

### 8.4 板块权重显示口径
若 `bootstrap()` 需要回传板块成员权重，则：
- `equal_weight_v1` 按等权返回
- `market_cap_weight_v1` 按“本地最新可用收盘价 × 本地股份数据”返回
- 不能因为 Step 2 是本地闭环就返回随意常量

若前端需要在同一板块内切换算法并刷新成员权重，则：
- 不要求重新 `bootstrap()`
- 必须通过独立接口 `get_board_member_summaries(payload)` 获取
- `bootstrap().membersByBoard` 只表示板块默认算法快照

## 9. 建议实施顺序
后端工程师按以下顺序推进，不要跳步：

1. 先完成 SQLite schema 与迁移
2. 再完成 fixture 导入与幂等保护
3. 再实现 repository 读写
4. 再实现 `BootstrapService / NoteService / BoardService / ChartService`
5. 再把 commands 接到 service，而不是继续直接拼样例对象
6. 最后补测试与最小联调验证

## 10. 测试要求
至少覆盖：
- schema 初始化
- `WAL` 打开
- fixture 导入
- fixture 幂等导入
- `bootstrap` 返回结构
- 四大指数本地读取
- `save_board` 持久化闭环
- `save_board` 后台路径状态落库
- `get_board_build_status` 读库
- `get_chart` `day / week`
- `get_chart` 空态与非法参数处理
- note 持久化闭环

开发完成后至少执行：
- `cargo fmt --check`
- `cargo test`
- 一轮以结构和风险为主的后端审查

如已接入 `clippy`，再执行：
- `cargo clippy --all-targets -- -D warnings`

## 11. 验收标准
- 无需 `Longbridge` 即可完整联调主流程
- 新建板块后，`bootstrap` 和 `get_chart` 都能读到结果
- 笔记保存后重启仍存在
- 四大指数也走 SQLite 正式路径
- 前端不需要额外 mock bridge
- command 层不再依赖硬编码业务常量
- `get_board_build_status` 能读到后台路径占位状态

## 12. 与 Step 3 的衔接要求
Step 2 完成后，后端应已经具备以下可直接延续到 Step 3 的基础：
- 可迁移的 SQLite schema
- 清晰的 `repository / service / chart_engine / commands` 分层
- 完整的 `save_board()` 双路径契约
- 可持久化的板块构建状态字段
- 不依赖内存假数据的本地联调能力

如果 Step 2 完成后仍然出现以下情况，视为未达标：
- command 直接返回硬编码图表数据
- 新建板块只能在当前进程内可见，重启后丢失
- note 只能存在内存
- 指数走特殊硬编码分支
- `get_chart()` 遇到空数据时返回伪造 bars
