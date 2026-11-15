use super::{Cancellation, Details, ImageFormat, ImageMode, Options, Tools, Update, extension};
use crate::process::{command, run_process};
use anyhow::{Context, Result, bail};
use std::{fs::File, path::Path, process::Stdio};

pub(super) fn encode(
    tools: &Tools,
    input: &Path,
    output: File,
    options: &Options,
    cancellation: &Cancellation,
    notify: &mut impl FnMut(Update),
) -> Result<()> {
    let codec = match extension(input).as_str() {
        "jpg" | "jpeg" | "jfif" => "JPEG",
        "png" => "PNG",
        "bmp" => "BMP",
        "tif" | "tiff" => "TIFF",
        "webp" => "WEBP",
        "heic" | "heif" => "HEIC",
        "avif" => "AVIF",
        _ => bail!("Unsupported image format"),
    };
    let source = format!("{codec}:-[0]");
    let output_coder = format!("{}:-", options.image_format.extension());
    let mode = options.image_mode;
    let quality = if mode == ImageMode::Lossy { "60" } else { "95" };
    notify(Update::Progress(0.1));

    // Input and output are open files. ImageMagick never interprets a user's
    // path as a coder prefix, frame range, glob, or indirect read.
    let exe = tools.magick()?;
    let mut cmd = command(exe);
    #[cfg(target_os = "linux")]
    cmd.env("APPIMAGE_EXTRACT_AND_RUN", "1");
    if let Some(directory) = exe.parent().filter(|dir| dir.join("policy.xml").is_file()) {
        cmd.env("MAGICK_CONFIGURE_PATH", directory);
    }
    cmd.env("MAGICK_THREAD_LIMIT", "2").args([
        &source,
        "-auto-orient",
        "-format",
        "%m %wx%h\n",
        "-write",
        "info:fd:2",
    ]);
    match options.image_format {
        ImageFormat::Jpg => {
            cmd.args([
                "-background",
                "white",
                "-alpha",
                "remove",
                "-alpha",
                "off",
                "-quality",
                quality,
                "-define",
                "jpeg:optimize-coding=true",
            ]);
            if mode == ImageMode::Original {
                cmd.args(["-sampling-factor", "4:4:4"]);
            } else {
                cmd.arg("-strip");
            }
        }
        ImageFormat::Webp => {
            cmd.args([
                "-strip",
                "-quality",
                quality,
                "-define",
                "webp:method=6",
                "-define",
                "webp:lossless=false",
            ]);
        }
        ImageFormat::Png => {
            cmd.args(["-define", "png:compression-level=6"]);
        }
    }
    cmd.arg(output_coder)
        .stdin(Stdio::from(File::open(input).context("open image")?))
        .stdout(Stdio::from(output));
    let (_, stderr) = run_process(&mut cmd, "ImageMagick", cancellation, |_| ())?;
    let details = String::from_utf8_lossy(&stderr);
    let resolution = details
        .lines()
        .find_map(|line| line.strip_prefix(codec).and_then(|s| s.strip_prefix(' ')))
        .unwrap_or("");
    notify(Update::Details(Details {
        codec: codec.to_owned(),
        resolution: resolution.replace('x', "×"),
        ..Details::default()
    }));
    Ok(())
}
