import unittest
from pathlib import Path


ROOT = Path("/Users/qr_luo/downloadtemp/new_stock")


class MacOSCredentialStoreTests(unittest.TestCase):
    def test_macos_credentials_use_local_file_store_not_keychain(self) -> None:
        secret_mod = (ROOT / "src-tauri/src/secret_store/mod.rs").read_text()

        self.assertIn('#[cfg(target_os = "macos")]\nmod local_file;', secret_mod)
        self.assertIn(
            '#[cfg(target_os = "macos")]\npub use local_file::{load_credentials, save_credentials};',
            secret_mod,
        )
        self.assertNotIn('#[cfg(any(target_os = "macos", target_os = "windows"))]\nmod native_credential;', secret_mod)

    def test_macos_build_does_not_depend_on_keyring(self) -> None:
        cargo_toml = (ROOT / "src-tauri/Cargo.toml").read_text()

        self.assertIn('[target.\'cfg(target_os = "windows")\'.dependencies]', cargo_toml)
        self.assertNotIn('[target.\'cfg(any(target_os = "macos", target_os = "windows"))\'.dependencies]', cargo_toml)


if __name__ == "__main__":
    unittest.main()
