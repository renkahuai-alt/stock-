use std::fs;
use std::path::PathBuf;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::errors::{AppError, AppResult};
use crate::models::SaveCredentialsPayload;

const APP_DATA_DIR_NAME: &str = "new_stock";
const CREDENTIAL_FILE_NAME: &str = "longbridge_credentials.json";

pub fn save_credentials(payload: &SaveCredentialsPayload) -> AppResult<()> {
    let path = credential_file_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let serialized = serde_json::to_vec(payload)?;
    let temp_path = path.with_extension("json.tmp");
    fs::write(&temp_path, serialized)?;
    restrict_file_permissions(&temp_path)?;
    fs::rename(&temp_path, &path)?;
    restrict_file_permissions(&path)?;

    Ok(())
}

pub fn load_credentials() -> AppResult<Option<SaveCredentialsPayload>> {
    let path = credential_file_path();
    if !path.exists() {
        return Ok(None);
    }

    let raw = fs::read_to_string(&path)?;
    if raw.trim().is_empty() {
        return Ok(None);
    }

    serde_json::from_str(&raw)
        .map(Some)
        .map_err(|error| AppError::Message(format!("invalid local credential payload: {error}")))
}

fn credential_file_path() -> PathBuf {
    if let Ok(path) = std::env::var("NEW_STOCK_CREDENTIALS_PATH") {
        return PathBuf::from(path);
    }

    let base = dirs::data_local_dir()
        .or_else(dirs::data_dir)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));

    base.join(APP_DATA_DIR_NAME).join(CREDENTIAL_FILE_NAME)
}

#[cfg(unix)]
fn restrict_file_permissions(path: &PathBuf) -> AppResult<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_file_permissions(_path: &PathBuf) -> AppResult<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_path_can_be_overridden_for_tests() {
        let path = std::env::temp_dir().join("new-stock-test-credentials.json");
        std::env::set_var("NEW_STOCK_CREDENTIALS_PATH", &path);

        assert_eq!(credential_file_path(), path);

        std::env::remove_var("NEW_STOCK_CREDENTIALS_PATH");
    }
}
