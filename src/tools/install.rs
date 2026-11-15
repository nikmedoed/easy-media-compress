use super::download::download_tool;
use crate::process::command;
use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};

pub(super) fn install_tool(family: &str, name: &str, file: &str) -> Result<PathBuf> {
    static SETUP: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = SETUP.lock().map_err(|e| anyhow!("{e}"))?;
    let dirs = || {
        let mut paths: Vec<_> = std::env::var_os("PATH")
            .iter()
            .flat_map(std::env::split_paths)
            .filter(|p| p.is_absolute())
            .collect();
        if cfg!(windows) {
            for scope in ["User", "Machine"] {
                if let Ok(output) = command(Path::new("powershell.exe"))
                    .args([
                        "-NoProfile",
                        "-Command",
                        &format!("[Environment]::GetEnvironmentVariable('Path','{scope}')"),
                    ])
                    .output()
                {
                    paths.extend(
                        std::env::split_paths(String::from_utf8_lossy(&output.stdout).trim())
                            .filter(|p| p.is_absolute()),
                    );
                }
            }
        }
        paths.extend(
            ["/opt/homebrew/bin", "/usr/local/bin"]
                .into_iter()
                .map(PathBuf::from),
        );
        paths
            .into_iter()
            .map(|p| p.join(file))
            .find(|p| p.is_file())
    };
    let base = std::env::var_os(if cfg!(windows) {
        "LOCALAPPDATA"
    } else {
        "HOME"
    })
    .ok_or_else(|| anyhow!("Cannot locate user data directory"))?;
    let root = PathBuf::from(base)
        .join(".local/share/easy-media-compress/tools")
        .join(family);
    let cached = root.join(file);
    if let Some(path) = dirs() {
        return Ok(path);
    }
    if cached.is_file() {
        return Ok(cached);
    }
    let package = if family == "ffmpeg" {
        "ffmpeg"
    } else {
        "imagemagick"
    };
    let mut manager = if cfg!(windows) {
        let mut cmd = command(Path::new("winget.exe"));
        let id = if family == "ffmpeg" {
            "Gyan.FFmpeg"
        } else {
            "ImageMagick.Q16-HDRI"
        };
        cmd.args([
            "install",
            "--id",
            id,
            "--exact",
            "--source",
            "winget",
            "--silent",
            "--disable-interactivity",
            "--accept-package-agreements",
            "--accept-source-agreements",
        ]);
        cmd
    } else if cfg!(target_os = "macos") {
        let brew = ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"]
            .into_iter()
            .find(|p| Path::new(p).is_file())
            .unwrap_or("brew");
        let mut cmd = command(Path::new(brew));
        cmd.args(["install", package]);
        cmd
    } else {
        let mut cmd = command(Path::new("sudo"));
        cmd.args(["-n", "apt-get", "install", "-y", package]);
        cmd
    };
    let _ = manager.status();
    if let Some(path) = dirs() {
        return Ok(path);
    }
    download_tool(&root, name, file)
}
