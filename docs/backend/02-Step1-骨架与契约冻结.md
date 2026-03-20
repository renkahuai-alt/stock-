# 后端 Step 1：骨架与契约冻结

## 1. 目标
本步骤只做 Rust/Tauri 宿主骨架、模块分层和接口契约冻结。

完成后后端应达到：
- `src-tauri/` 骨架完整
- commands/events 名称固定
- 数据模型和模块边界固定

## 2. 必做项
- 建立 `src-tauri/` 工程
- 建立以下模块：
  - `app_shell`
  - `commands`
  - `services`
  - `jobs`
  - `repositories`
  - `live_quote`
  - `chart_engine`
  - `secret_store`
  - `models`
  - `errors`
  - `events`

## 3. 契约冻结
- 固定 commands：
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
- 固定 events：
  - `sync-status`
  - `board-build-status`
  - `chart-live-update`
  - `settings-saved`

## 4. 本步骤交付
- Tauri 宿主骨架
- 主窗口和设置窗口管理壳
- Rust 模块骨架
- command/event 占位实现

## 5. 完成标准
- 后续步骤不再改命令名、事件名和主要模块边界
