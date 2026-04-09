use std::sync::OnceLock;

use keyring::{Entry, Error as KeyringError};

use crate::errors::{AppError, AppResult};
use crate::models::SaveCredentialsPayload;

const CREDENTIAL_SERVICE: &str = "new_stock.longbridge.credentials";
const CREDENTIAL_ACCOUNT: &str = "default";

fn ensure_native_store() -> AppResult<()> {
    static INIT: OnceLock<Result<(), String>> = OnceLock::new();

    INIT.get_or_init(|| keyring::use_native_store(false).map_err(|error| error.to_string()))
        .as_ref()
        .map_err(|error| {
            AppError::Message(format!(
                "failed to initialize Windows credential store: {error}"
            ))
        })?;

    Ok(())
}

fn entry() -> AppResult<Entry> {
    ensure_native_store()?;
    Entry::new(CREDENTIAL_SERVICE, CREDENTIAL_ACCOUNT)
        .map_err(|error| AppError::Message(format!("failed to initialize credential entry: {error}")))
}

pub fn save_credentials(payload: &SaveCredentialsPayload) -> AppResult<()> {
    let serialized = serde_json::to_string(payload)?;
    entry()?
        .set_password(&serialized)
        .map_err(|error| AppError::Message(format!("failed to save Windows credentials: {error}")))
}

pub fn load_credentials() -> AppResult<Option<SaveCredentialsPayload>> {
    let raw = match entry()?.get_password() {
        Ok(value) => value,
        Err(KeyringError::NoEntry) => return Ok(None),
        Err(error) => {
            return Err(AppError::Message(format!(
                "failed to read Windows credentials: {error}"
            )))
        }
    };

    if raw.trim().is_empty() {
        return Ok(None);
    }

    serde_json::from_str(&raw)
        .map(Some)
        .map_err(|error| AppError::Message(format!("invalid credential payload: {error}")))
}
