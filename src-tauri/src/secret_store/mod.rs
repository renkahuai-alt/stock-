#[cfg(target_os = "macos")]
mod keychain;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod unsupported;
#[cfg(target_os = "windows")]
mod windows_credential;

#[cfg(target_os = "macos")]
pub use keychain::{load_credentials, save_credentials};
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub use unsupported::{load_credentials, save_credentials};
#[cfg(target_os = "windows")]
pub use windows_credential::{load_credentials, save_credentials};
