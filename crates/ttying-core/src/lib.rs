mod pty;
mod server;
mod session;

pub use server::{random_token, start_background, ServerConfig, ServerHandle};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const DEFAULT_PORT: u16 = 7755;

#[cfg(target_os = "windows")]
pub fn default_shell() -> Vec<String> {
    vec!["cmd.exe".to_string()]
}

#[cfg(target_os = "android")]
pub fn default_shell() -> Vec<String> {
    vec!["/system/bin/sh".to_string()]
}

#[cfg(all(unix, not(target_os = "android")))]
pub fn default_shell() -> Vec<String> {
    vec![std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())]
}
