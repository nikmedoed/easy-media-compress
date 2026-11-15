use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};

mod download;
mod install;

#[derive(Default)]
pub struct Tools {
    ffmpeg: std::sync::OnceLock<Result<PathBuf, String>>,
    ffprobe: std::sync::OnceLock<Result<PathBuf, String>>,
    magick: std::sync::OnceLock<Result<PathBuf, String>>,
}

impl Tools {
    pub(crate) fn ffmpeg(&self) -> Result<&Path> {
        Self::resolve(&self.ffmpeg, "ffmpeg")
    }

    pub(crate) fn ffprobe(&self) -> Result<&Path> {
        Self::resolve(&self.ffprobe, "ffprobe")
    }

    pub(crate) fn magick(&self) -> Result<&Path> {
        Self::resolve(&self.magick, "magick")
    }

    fn resolve<'a>(
        slot: &'a std::sync::OnceLock<Result<PathBuf, String>>,
        name: &str,
    ) -> Result<&'a Path> {
        slot.get_or_init(|| {
            let file = if cfg!(windows) {
                format!("{name}.exe")
            } else {
                name.to_owned()
            };
            let family = if name == "magick" {
                "imagemagick"
            } else {
                "ffmpeg"
            };
            let mut dirs = Vec::new();
            if let Ok(exe) = std::env::current_exe()
                && let Some(dir) = exe.parent()
            {
                dirs.extend([
                    dir.join("tools").join(family),
                    dir.join(family).join("bin"),
                    dir.join(family),
                    dir.to_path_buf(),
                ]);
                #[cfg(target_os = "macos")]
                if let Some(contents) = dir.parent() {
                    dirs.insert(0, contents.join("Resources").join("tools").join(family));
                }
            }
            let override_key = if name == "magick" {
                "MEDIA_COMPRESS_MAGICK_DIR"
            } else {
                "MEDIA_COMPRESS_FFMPEG_DIR"
            };
            if let Some(dir) = std::env::var_os(override_key).filter(|v| !v.is_empty()) {
                let dir = PathBuf::from(dir);
                dirs.splice(0..0, [dir.clone(), dir.join("bin")]);
            }
            if let Some(path) = std::env::var_os("PATH") {
                dirs.extend(std::env::split_paths(&path).filter(|dir| dir.is_absolute()));
            }
            #[cfg(target_os = "macos")]
            dirs.extend([
                PathBuf::from("/opt/homebrew/bin"),
                PathBuf::from("/usr/local/bin"),
            ]);
            if let Some(path) = dirs
                .into_iter()
                .map(|dir| dir.join(&file))
                .find(|path| path.is_file())
            {
                return Ok(path);
            }
            install::install_tool(family, name, &file).map_err(|e| format!("{e:#}"))
        })
        .as_deref()
        .map_err(|err| anyhow!("{err}"))
    }
}
