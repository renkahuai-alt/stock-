import subprocess
import unittest
from pathlib import Path


ROOT = Path("/Users/qr_luo/downloadtemp/new_stock")
FRONTEND = ROOT / "frontend"


class FrontendDistPathTests(unittest.TestCase):
    def test_built_html_uses_relative_asset_paths_for_tauri(self) -> None:
        subprocess.run(
            ["npm", "run", "build"],
            cwd=FRONTEND,
            check=True,
            capture_output=True,
            text=True,
        )

        index_html = (FRONTEND / "dist/index.html").read_text()
        settings_html = (FRONTEND / "dist/settings.html").read_text()

        self.assertIn('./assets/', index_html)
        self.assertIn('./assets/', settings_html)
        self.assertNotIn('src="/assets/', index_html)
        self.assertNotIn('href="/assets/', index_html)
        self.assertNotIn('src="/assets/', settings_html)
        self.assertNotIn('href="/assets/', settings_html)


if __name__ == "__main__":
    unittest.main()
