use crate::process::command;
use anyhow::{Context, Result, anyhow};
use std::path::{Path, PathBuf};

fn latest_windows_archive(name: &str, file: &str) -> Result<(String, String, String)> {
    let (repo, suffix) = if name == "magick" {
        ("ImageMagick/ImageMagick", "-portable-Q16-x64.7z")
    } else {
        ("GyanD/codexffmpeg", "-essentials_build.zip")
    };
    let output = command(Path::new("curl.exe"))
        .args(["--fail", "--silent", "--show-error", "--proto", "=https"])
        .arg(format!(
            "https://api.github.com/repos/{repo}/releases/latest"
        ))
        .output()?;
    if !output.status.success() {
        return Err(anyhow!("Could not look up the latest release for {name}"));
    }
    let release: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let assets = release["assets"]
        .as_array()
        .context("Missing release assets")?;
    for asset in assets {
        let Some(stem) = asset["name"].as_str().and_then(|n| n.strip_suffix(suffix)) else {
            continue;
        };
        let Some(url) = asset["browser_download_url"].as_str() else {
            continue;
        };
        if !url.starts_with(&format!("https://github.com/{repo}/releases/download/")) {
            continue;
        }
        let Some(hash) = asset["digest"]
            .as_str()
            .and_then(|s| s.strip_prefix("sha256:"))
        else {
            continue;
        };
        if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            continue;
        }
        let member = if name == "magick" {
            file.to_owned()
        } else {
            format!("{stem}-essentials_build/bin/{file}")
        };
        return Ok((url.to_owned(), hash.to_owned(), member));
    }
    Err(anyhow!("No Windows archive with SHA-256 found for {name}"))
}

pub(super) fn download_tool(root: &Path, name: &str, file: &str) -> Result<PathBuf> {
    let (url, hash, member) = match (std::env::consts::OS, std::env::consts::ARCH, name) {
        ("windows", "x86_64", "ffmpeg" | "ffprobe" | "magick") => latest_windows_archive(name, file)?,
        ("linux" | "macos", arch, "ffmpeg" | "ffprobe") => {
            let arch = match arch { "x86_64" => "amd64", "aarch64" => "arm64", _ => return Err(anyhow!("Unsupported architecture")) };
            let os = std::env::consts::OS;
            (format!("https://ffmpeg.martin-riedl.de/redirect/latest/{os}/{arch}/release/{name}.zip"), String::new(), file.to_owned())
        }
        ("linux", "x86_64", "magick") => (
            "https://github.com/ImageMagick/ImageMagick/releases/download/7.1.2-35/ImageMagick-7.1.2-35-gcc-x86_64.AppImage".to_owned(), String::new(), String::new()),
        _ => return Err(anyhow!("Install {name} using your system package manager")),
    };
    std::fs::create_dir_all(root)?;
    let stage = tempfile::tempdir_in(root)?;
    let archive = stage.path().join("download");
    let curl = if cfg!(windows) { "curl.exe" } else { "curl" };
    if !command(Path::new(curl))
        .args([
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
        ])
        .arg(url)
        .arg("-o")
        .arg(&archive)
        .status()?
        .success()
    {
        return Err(anyhow!("Download failed for {name}"));
    }
    if !hash.is_empty() {
        let output = command(Path::new("certutil.exe"))
            .arg("-hashfile")
            .arg(&archive)
            .arg("SHA256")
            .output()?;
        if !output.status.success()
            || !String::from_utf8_lossy(&output.stdout)
                .lines()
                .any(|line| line.trim().eq_ignore_ascii_case(&hash))
        {
            return Err(anyhow!("Invalid SHA-256 for {name}"));
        }
    }
    let binary = if member.is_empty() {
        archive.clone()
    } else {
        let mut extract = if cfg!(windows) {
            let mut cmd = command(Path::new("tar.exe"));
            cmd.arg("-xOf").arg(&archive).arg(&member);
            cmd
        } else {
            let mut cmd = command(Path::new("unzip"));
            cmd.arg("-p").arg(&archive).arg(&member);
            cmd
        };
        let binary = stage.path().join(file);
        if !extract
            .stdout(std::fs::File::create(&binary)?)
            .status()?
            .success()
        {
            return Err(anyhow!("Could not extract {name}"));
        }
        binary
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700))?;
    }
    let mut check = command(&binary);
    if member.is_empty() {
        check.env("APPIMAGE_EXTRACT_AND_RUN", "1");
    }
    if !check
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .status()?
        .success()
    {
        return Err(anyhow!("Downloaded {name} could not start"));
    }
    let path = root.join(file);
    std::fs::rename(binary, &path)?;
    Ok(path)
}
