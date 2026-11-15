use anyhow::{Context, Result, ensure};
use std::{ffi::OsStr, path::Path};

mod image;
mod options;
mod video;

pub use crate::process::{Cancellation, Cancelled};
pub use crate::tools::Tools;
pub use options::{ImageFormat, ImageMode, Options, VideoProfile};

pub const VIDEO_EXTS: &[&str] = &["mp4", "mkv", "avi", "mov", "flv", "wmv", "webm"];
pub const IMAGE_EXTS: &[&str] = &[
    "jpg", "jpeg", "jfif", "png", "bmp", "tif", "tiff", "webp", "heic", "heif", "avif",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Video,
    Image,
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(OsStr::to_str)
        .unwrap_or("")
        .to_ascii_lowercase()
}

pub fn kind(path: &Path) -> Option<Kind> {
    let ext = extension(path);
    if VIDEO_EXTS.contains(&ext.as_str()) {
        Some(Kind::Video)
    } else if IMAGE_EXTS.contains(&ext.as_str()) {
        Some(Kind::Image)
    } else {
        None
    }
}

#[derive(Debug, Default)]
pub struct Details {
    pub codec: String,
    pub duration: String,
    pub resolution: String,
}

#[derive(Debug)]
pub enum Update {
    Progress(f32),
    Details(Details),
}

pub fn convert(
    tools: &Tools,
    input: &Path,
    output: &Path,
    kind: Kind,
    options: &Options,
    cancellation: &Cancellation,
    mut notify: impl FnMut(Update) + Send,
) -> Result<u64> {
    cancellation.check()?;
    let suffix = format!(".{}", extension(output));
    let temp = tempfile::Builder::new()
        .prefix(".compress-")
        .suffix(&suffix)
        .tempfile_in(output.parent().context("Output has no parent folder")?)
        .context("create temporary output beside source")?;
    match kind {
        Kind::Video => video::encode(
            tools,
            input,
            temp.path(),
            options,
            cancellation,
            &mut notify,
        )?,
        Kind::Image => image::encode(
            tools,
            input,
            temp.reopen().context("open temporary output")?,
            options,
            cancellation,
            &mut notify,
        )?,
    }
    let bytes = temp.as_file().metadata().context("read output size")?.len();
    ensure!(bytes > 0, "The encoder produced an empty file");
    // Serialize publication with cancellation so a stopped job cannot publish afterward.
    let cancelled = cancellation.lock();
    ensure!(!*cancelled, Cancelled);
    temp.persist_noclobber(output)
        .context("save converted file")?;
    Ok(bytes)
}
