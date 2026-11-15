pub mod cli;
pub mod jobs;
pub mod media;
mod process;
pub mod settings;
mod tools;

pub const APP_NAME: &str = "Easy Media Compress";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(windows)]
pub fn release_version(value: &str) -> Option<[u32; 3]> {
    let mut parts = value.trim().split('.');
    let version = [
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    ];
    parts.next().is_none().then_some(version)
}
