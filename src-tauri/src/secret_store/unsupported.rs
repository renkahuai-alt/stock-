use crate::errors::{AppError, AppResult};
use crate::models::SaveCredentialsPayload;

pub fn save_credentials(_payload: &SaveCredentialsPayload) -> AppResult<()> {
    Err(AppError::Message(
        "secure credential storage is currently only supported on macOS and Windows".into(),
    ))
}

pub fn load_credentials() -> AppResult<Option<SaveCredentialsPayload>> {
    Ok(None)
}
