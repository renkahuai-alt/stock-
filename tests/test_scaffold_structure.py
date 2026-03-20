import json
import unittest
from pathlib import Path


ROOT = Path("/Users/qr_luo/downloadtemp/new_stock")


class NewStockScaffoldTests(unittest.TestCase):
    def test_frontend_files_exist(self) -> None:
        expected = [
            ROOT / "frontend/package.json",
            ROOT / "frontend/tsconfig.json",
            ROOT / "frontend/vite.config.ts",
            ROOT / "frontend/index.html",
            ROOT / "frontend/src/main.ts",
            ROOT / "frontend/src/windows/main/MainWindow.svelte",
            ROOT / "frontend/src/windows/settings/SettingsWindow.svelte",
            ROOT / "frontend/src/styles/tokens.css",
            ROOT / "frontend/src/styles/app.css",
        ]
        for path in expected:
            with self.subTest(path=path):
                self.assertTrue(path.exists(), f"missing {path}")

    def test_backend_files_exist(self) -> None:
        expected = [
            ROOT / "src-tauri/Cargo.toml",
            ROOT / "src-tauri/build.rs",
            ROOT / "src-tauri/tauri.conf.json",
            ROOT / "src-tauri/icons/icon.png",
            ROOT / "src-tauri/src/main.rs",
            ROOT / "src-tauri/src/lib.rs",
            ROOT / "src-tauri/src/commands/mod.rs",
            ROOT / "src-tauri/src/services/mod.rs",
            ROOT / "src-tauri/src/repositories/mod.rs",
            ROOT / "src-tauri/src/jobs/mod.rs",
            ROOT / "src-tauri/src/live_quote/mod.rs",
            ROOT / "src-tauri/src/chart_engine/mod.rs",
            ROOT / "src-tauri/src/secret_store/mod.rs",
        ]
        for path in expected:
            with self.subTest(path=path):
                self.assertTrue(path.exists(), f"missing {path}")

    def test_frontend_package_declares_required_stack(self) -> None:
        package_path = ROOT / "frontend/package.json"
        self.assertTrue(package_path.exists(), "frontend/package.json missing")
        package = json.loads(package_path.read_text())
        dependencies = {
            **package.get("dependencies", {}),
            **package.get("devDependencies", {}),
        }
        for dep in [
            "svelte",
            "typescript",
            "vite",
            "@sveltejs/vite-plugin-svelte",
            "@tauri-apps/api",
            "lightweight-charts",
        ]:
            with self.subTest(dep=dep):
                self.assertIn(dep, dependencies)

    def test_rust_commands_cover_required_contract(self) -> None:
        commands_mod = ROOT / "src-tauri/src/commands/mod.rs"
        self.assertTrue(commands_mod.exists(), "commands/mod.rs missing")
        content = commands_mod.read_text()
        for command_name in [
            "bootstrap",
            "save_credentials",
            "get_sync_status",
            "run_sync",
            "get_chart",
            "save_board",
            "get_board_build_status",
            "get_target_note",
            "save_target_note",
            "open_settings_window",
            "close_settings_window",
            "start_chart_watch",
            "stop_chart_watch",
        ]:
            with self.subTest(command_name=command_name):
                self.assertIn(command_name, content)

    def test_step1_commands_use_direct_payload_contracts(self) -> None:
        commands_mod = ROOT / "src-tauri/src/commands/mod.rs"
        content = commands_mod.read_text()
        self.assertNotIn("CommandResponse", content)
        self.assertNotIn("fn ok(", content)
        self.assertNotIn("serde_json::json", content)

    def test_step1_models_define_output_payloads(self) -> None:
        models_mod = ROOT / "src-tauri/src/models/mod.rs"
        self.assertTrue(models_mod.exists(), "models/mod.rs missing")
        content = models_mod.read_text()
        for model_name in [
            "BootstrapPayload",
            "SyncStatusPayload",
            "ChartPayload",
            "SaveBoardResponse",
            "BoardBuildStatusPayload",
            "LiveQuoteOverlayPayload",
            "SimpleStatusPayload",
        ]:
            with self.subTest(model_name=model_name):
                self.assertIn(model_name, content)

    def test_step1_placeholder_commands_emit_all_core_events(self) -> None:
        commands_mod = ROOT / "src-tauri/src/commands/mod.rs"
        content = commands_mod.read_text()
        for event_name in [
            "events::SYNC_STATUS",
            "events::BOARD_BUILD_STATUS",
            "events::CHART_LIVE_UPDATE",
            "events::SETTINGS_SAVED",
        ]:
            with self.subTest(event_name=event_name):
                self.assertIn(event_name, content)

    def test_tauri_capability_permissions_use_core_prefix(self) -> None:
        capability_path = ROOT / "src-tauri/capabilities/default.json"
        self.assertTrue(capability_path.exists(), "capabilities/default.json missing")
        config = json.loads(capability_path.read_text())
        permissions = config.get("permissions", [])
        for permission in permissions:
            with self.subTest(permission=permission):
                self.assertTrue(
                    permission.startswith("core:"),
                    f"permission must use Tauri 2 core prefix: {permission}",
                )


if __name__ == "__main__":
    unittest.main()
