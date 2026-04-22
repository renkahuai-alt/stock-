import unittest
from pathlib import Path


ROOT = Path("/Users/qr_luo/downloadtemp/new_stock")


class StartupAutoSyncTests(unittest.TestCase):
    def test_main_window_recovers_startup_sync_after_listeners_are_ready(self) -> None:
        main_window = (ROOT / "frontend/src/app/mainWindow.ts").read_text()

        listener_index = main_window.find("const unlisten = await registerCoreListeners")
        recovery_index = main_window.find("await recoverStartupSyncResult()")

        self.assertGreaterEqual(listener_index, 0, "main window must register core listeners")
        self.assertGreaterEqual(
            recovery_index,
            0,
            "main window must recover startup sync result after boot",
        )
        self.assertLess(
            listener_index,
            recovery_index,
            "startup sync recovery must run after listeners are registered to avoid missing sync-status events",
        )

    def test_startup_sync_recovery_refreshes_status_and_chart(self) -> None:
        main_flow = (ROOT / "frontend/src/services/mainFlow.ts").read_text()

        self.assertIn("export async function recoverStartupSyncResult", main_flow)
        self.assertIn("await refreshSyncStatus()", main_flow)
        self.assertIn("await syncSelectionChartState()", main_flow)


if __name__ == "__main__":
    unittest.main()
