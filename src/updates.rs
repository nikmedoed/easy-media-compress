use serde::Deserialize;
use std::{
    os::windows::process::CommandExt,
    path::PathBuf,
    process::Command,
    sync::mpsc::{self, Receiver},
};

pub const RELEASES: &str = "https://github.com/nikmedoed/easy-media-compress/releases/latest";
const API: &str = "https://api.github.com/repos/nikmedoed/easy-media-compress/releases/latest";
const SETUP: &str = "EasyMediaCompress-Setup-x64.exe";

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    size: u64,
}

fn newer_release(json: &[u8]) -> Option<String> {
    let release: Release = serde_json::from_slice(json).ok()?;
    let version = release.tag_name.strip_prefix('v')?;
    (easy_media_compress::release_version(version)?
        > easy_media_compress::release_version(easy_media_compress::VERSION)?
        && release
            .assets
            .iter()
            .any(|asset| asset.name == SETUP && asset.size > 0))
    .then_some(release.tag_name)
}

pub fn check(wake: impl FnOnce() + Send + 'static) -> Option<Receiver<String>> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    // The Inno Setup uninstaller is a sibling of the installed GUI.
    let installed = dir.read_dir().ok()?.flatten().any(|entry| {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        name.starts_with("unins") && name.ends_with(".exe")
    });
    if !installed {
        return None;
    }
    let curl = PathBuf::from(std::env::var_os("SystemRoot")?).join(r"System32\curl.exe");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let output = Command::new(curl)
            .creation_flags(0x0800_0000)
            .args([
                "-q",
                "-fsS",
                "--proto",
                "=https",
                "--max-time",
                "8",
                "--max-filesize",
                "1048576",
                "--user-agent",
                "EasyMediaCompress",
                API,
            ])
            .output();
        if let Ok(output) = output
            && output.status.success()
            && let Some(tag) = newer_release(&output.stdout)
        {
            let _ = sender.send(tag);
            wake();
        }
    });
    Some(receiver)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_offers_a_newer_release_with_a_windows_installer() {
        assert_eq!(
            newer_release(br#"{"tag_name":"v99.0.0","assets":[{"name":"EasyMediaCompress-Setup-x64.exe","size":123}]}"#).as_deref(),
            Some("v99.0.0"),
        );
        assert!(newer_release(br#"{"tag_name":"v0.2.6","assets":[{"name":"EasyMediaCompress-Setup-x64.exe","size":123}]}"#).is_none());
        assert!(
            newer_release(
                br#"{"tag_name":"v99.0.0","assets":[{"name":"EasyMediaCompress.exe","size":123}]}"#
            )
            .is_none()
        );
    }
}
