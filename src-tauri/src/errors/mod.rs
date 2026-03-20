use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error, Serialize)]
pub enum AppError {
    #[error("{0}")]
    Message(String),
}

impl From<tauri::Error> for AppError {
    fn from(value: tauri::Error) -> Self {
        Self::Message(value.to_string())
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Message(value.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(value: std::io::Error) -> Self {
        Self::Message(value.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(value: serde_json::Error) -> Self {
        Self::Message(value.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Serialize)]
pub struct ErrorPayload {
    pub message: String,
}

pub fn classify_error_code(error: &AppError) -> &'static str {
    let message = error.to_string().to_lowercase();
    if message.contains("credential")
        || message.contains("auth")
        || message.contains("token")
        || message.contains("secret")
    {
        "auth_failed"
    } else if message.contains("limit") || message.contains("rate") {
        "rate_limited"
    } else if message.contains("network") || message.contains("timeout") {
        "network_error"
    } else if message.contains("empty") {
        "empty_data"
    } else {
        "symbol_failed"
    }
}
