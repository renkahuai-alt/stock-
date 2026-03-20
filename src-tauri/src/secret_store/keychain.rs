use std::process::Command;

use crate::errors::{AppError, AppResult};
use crate::models::SaveCredentialsPayload;

const KEYCHAIN_SERVICE: &str = "new_stock.longbridge.credentials";
const KEYCHAIN_ACCOUNT: &str = "default";

pub fn save_credentials(payload: &SaveCredentialsPayload) -> AppResult<()> {
    let serialized = serde_json::to_string(payload)?;
    let output = Command::new("security")
        .args([
            "add-generic-password",
            "-U",
            "-a",
            KEYCHAIN_ACCOUNT,
            "-s",
            KEYCHAIN_SERVICE,
            "-w",
            &serialized,
        ])
        .output()?;

    if !output.status.success() {
        return Err(AppError::Message(format!(
            "failed to save keychain credentials: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    Ok(())
}

pub fn load_credentials() -> AppResult<Option<SaveCredentialsPayload>> {
    let output = Command::new("security")
        .args([
            "find-generic-password",
            "-w",
            "-a",
            KEYCHAIN_ACCOUNT,
            "-s",
            KEYCHAIN_SERVICE,
        ])
        .output()?;

    if output.status.success() {
        let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if raw.is_empty() {
            return Ok(None);
        }

        return serde_json::from_str(&raw)
            .map(Some)
            .map_err(|error| AppError::Message(format!("invalid keychain payload: {error}")));
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("could not be found") {
        return Ok(None);
    }

    Err(AppError::Message(format!(
        "failed to read keychain credentials: {}",
        stderr.trim()
    )))
}
