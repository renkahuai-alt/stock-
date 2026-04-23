#[cfg(target_os = "macos")]
mod local_file;
#[cfg(target_os = "windows")]
mod native_credential;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod unsupported;

#[cfg(target_os = "macos")]
pub use local_file::{load_credentials, save_credentials};
#[cfg(target_os = "windows")]
pub use native_credential::{load_credentials, save_credentials};
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub use unsupported::{load_credentials, save_credentials};
