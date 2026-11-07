use anyhow::{Context, Result, anyhow, bail, ensure};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsStr,
    fs::File,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{ChildStdout, Command, Stdio},
    sync::Mutex,
    thread,
    time::Duration,
};

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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VideoProfile {
    #[default]
    Crf,
    Size,
    Crf480,
    Crf720,
    Gif120,
    Gif180,
    Gif,
    Gif480,
}

impl VideoProfile {
    pub fn label(self) -> &'static str {
        match self {
            Self::Crf => "MP4",
            Self::Size => "5 MB",
            Self::Crf480 => "MP4 480p",
            Self::Crf720 => "MP4 720p",
            Self::Gif120 => "GIF 120p",
            Self::Gif180 => "GIF 180p",
            Self::Gif => "GIF 240p",
            Self::Gif480 => "GIF 480p",
        }
    }

    pub fn alternate(self) -> Self {
        if self == Self::Crf {
            Self::Size
        } else {
            Self::Crf
        }
    }

    fn gif_target(self) -> Option<u32> {
        match self {
            Self::Gif120 => Some(120),
            Self::Gif180 => Some(180),
            Self::Gif => Some(240),
            Self::Gif480 => Some(480),
            _ => None,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageMode {
    #[default]
    Lossy,
    Original,
}

impl ImageMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Lossy => "lossy",
            Self::Original => "original",
        }
    }

    pub fn alternate(self) -> Self {
        match self {
            Self::Lossy => Self::Original,
            Self::Original => Self::Lossy,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ImageFormat {
    #[default]
    #[value(alias = "jpeg")]
    Jpg,
    Png,
    Webp,
}

impl ImageFormat {
    fn extension(self) -> &'static str {
        match self {
            Self::Jpg => "jpg",
            Self::Png => "png",
            Self::Webp => "webp",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Options {
    pub video_profile: VideoProfile,
    pub image_mode: ImageMode,
    pub image_format: ImageFormat,
    pub crf: u8,
    pub preset: String,
    pub only: Option<Kind>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            video_profile: VideoProfile::Crf,
            image_mode: ImageMode::Lossy,
            image_format: ImageFormat::Jpg,
            crf: 30,
            preset: "slow".into(),
            only: None,
        }
    }
}

impl Options {
    pub fn label(&self, kind: Kind) -> &'static str {
        match kind {
            Kind::Video => self.video_profile.label(),
            Kind::Image => self.image_mode.label(),
        }
    }

    pub fn alternate(&self, kind: Kind) -> Self {
        let mut next = self.clone();
        match kind {
            Kind::Video => next.video_profile = self.video_profile.alternate(),
            Kind::Image => next.image_mode = self.image_mode.alternate(),
        }
        next
    }

    pub fn output_name(&self, input: &Path, kind: Kind, serial: usize) -> PathBuf {
        let (suffix, ext) = match kind {
            Kind::Video if self.video_profile.gif_target().is_some() => ("_converted", "gif"),
            Kind::Video if self.video_profile == VideoProfile::Size => ("_smaller", "mp4"),
            Kind::Video => ("_compressed", "mp4"),
            Kind::Image if self.image_mode == ImageMode::Lossy => {
                ("_compressed", self.image_format.extension())
            }
            Kind::Image => ("_converted", self.image_format.extension()),
        };
        let mut name = input.file_stem().unwrap_or_default().to_os_string();
        if serial > 1 {
            name.push(format!(" ({serial})"));
        }
        name.push(format!("{suffix}.{ext}"));
        input.with_file_name(name)
    }
}

#[derive(Debug, Default)]
pub struct Details {
    pub codec: String,
    pub duration: String,
    pub resolution: String,
}

#[derive(Default)]
pub struct Tools {
    pub(crate) ffmpeg: std::sync::OnceLock<Result<PathBuf, String>>,
    pub(crate) ffprobe: std::sync::OnceLock<Result<PathBuf, String>>,
    pub(crate) magick: std::sync::OnceLock<Result<PathBuf, String>>,
}

impl Tools {
    fn resolve<'a>(
        &self,
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
            dirs.into_iter()
                .map(|dir| dir.join(&file))
                .find(|path| path.is_file())
                .ok_or_else(|| {
                    format!("{file} was not found. Keep the tools folder beside the portable app.")
                })
        })
        .as_deref()
        .map_err(|err| anyhow!("{err}"))
    }
}

fn command(exe: &Path) -> Command {
    let mut cmd = Command::new(exe);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}

fn magick_command(exe: &Path) -> Command {
    let mut cmd = command(exe);
    if let Some(directory) = exe.parent().filter(|dir| dir.join("policy.xml").is_file()) {
        cmd.env("MAGICK_CONFIGURE_PATH", directory);
    }
    cmd
}

#[derive(Debug, Deserialize)]
struct Probe {
    #[serde(default)]
    streams: Vec<Stream>,
    #[serde(default)]
    format: Container,
}

#[derive(Debug, Deserialize)]
struct Stream {
    width: u32,
    height: u32,
    #[serde(default)]
    codec_name: String,
    #[serde(default)]
    tags: Tags,
    #[serde(default)]
    side_data_list: Vec<SideData>,
}

#[derive(Debug, Default, Deserialize)]
struct Tags {
    rotate: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SideData {
    rotation: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct Container {
    duration: Option<String>,
}

fn failed_tool(name: &str, status: std::process::ExitStatus, stderr: &[u8]) -> anyhow::Error {
    let message = String::from_utf8_lossy(stderr);
    let message = message.trim();
    if message.is_empty() {
        anyhow!("{name} exited with {status}")
    } else {
        anyhow!("{name}: {message}")
    }
}

#[derive(Default)]
pub struct Cancellation(Mutex<bool>);

#[derive(Debug)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Conversion cancelled")
    }
}

impl std::error::Error for Cancelled {}

impl Cancellation {
    pub fn is_cancelled(&self) -> bool {
        *self.0.lock().unwrap()
    }

    pub fn cancel(&self) {
        *self.0.lock().unwrap() = true;
    }

    fn check(&self) -> Result<()> {
        ensure!(!self.is_cancelled(), Cancelled);
        Ok(())
    }
}

fn capture(mut reader: impl Read, limit: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    let _ = (&mut reader).take(limit).read_to_end(&mut bytes);
    let _ = std::io::copy(&mut reader, &mut std::io::sink());
    bytes
}

fn run_process<T: Send>(
    cmd: &mut Command,
    name: &str,
    cancellation: &Cancellation,
    read_stdout: impl FnOnce(Option<ChildStdout>) -> T + Send,
) -> Result<(T, Vec<u8>)> {
    cancellation.check()?;
    let mut child = cmd
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("start {name}"))?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take().expect("piped stderr");
    thread::scope(|scope| {
        let stdout = scope.spawn(move || read_stdout(stdout));
        let stderr = scope.spawn(move || capture(stderr, 128 * 1024));
        let status = loop {
            if cancellation.is_cancelled() {
                break Err(Cancelled.into());
            }
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) => thread::sleep(Duration::from_millis(20)),
                Err(error) => break Err(error).with_context(|| format!("wait for {name}")),
            }
        };
        if status.is_err() {
            let _ = child.kill();
            child.wait().with_context(|| format!("reap {name}"))?;
        }
        let status = status?;
        let stdout = stdout.join().expect("stdout reader");
        let stderr = stderr.join().expect("stderr reader");
        if !status.success() {
            return Err(failed_tool(name, status, &stderr));
        }
        Ok((stdout, stderr))
    })
}

fn probe_video(exe: &Path, path: &Path, cancellation: &Cancellation) -> Result<(Stream, f64)> {
    let mut cmd = command(exe);
    cmd
        .args(["-v", "error", "-select_streams", "V:0", "-show_entries",
            "format=duration:stream=width,height,codec_name:stream_tags=rotate:stream_side_data=rotation", "-of", "json", "-i"])
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped());
    let (stdout, _) = run_process(&mut cmd, "ffprobe", cancellation, |stdout| {
        capture(stdout.expect("piped stdout"), 1024 * 1024)
    })?;
    let probe: Probe = serde_json::from_slice(&stdout).context("read video metadata")?;
    let mut stream = probe
        .streams
        .into_iter()
        .next()
        .context("No video stream found")?;
    let rotation = stream
        .tags
        .rotate
        .as_deref()
        .and_then(|s| s.parse::<f64>().ok())
        .or_else(|| stream.side_data_list.iter().find_map(|data| data.rotation))
        .unwrap_or(0.0);
    if (rotation as i32).rem_euclid(180) != 0 {
        std::mem::swap(&mut stream.width, &mut stream.height);
    }
    ensure!(
        stream.width > 0 && stream.height > 0,
        "Invalid video dimensions"
    );
    let duration = probe
        .format
        .duration
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|s| s.is_finite() && *s > 0.0)
        .unwrap_or(0.0);
    Ok((stream, duration))
}

fn even(n: f64) -> u32 {
    (n as u32 / 2 * 2).max(2)
}

fn video_args(stream: &Stream, duration: f64, options: &Options) -> Result<Vec<String>> {
    let mut args = Vec::new();
    let mut push = |args_in: &[&str]| args.extend(args_in.iter().map(|s| (*s).to_owned()));
    let profile = options.video_profile;
    if let Some(target) = profile.gif_target() {
        let filter = format!(
            r"fps=15,scale=if(lt(iw\,ih)\,{target}\,-2):if(lt(iw\,ih)\,-2\,{target}):flags=lanczos"
        );
        push(&["-vf", &filter, "-loop", "0"]);
        return Ok(args);
    }

    push(&[
        "-map",
        "0:V:0",
        "-map",
        "0:a:0?",
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        "-movflags",
        "faststart",
    ]);
    let (w, h) = (stream.width, stream.height);
    let mut scaled = false;
    if profile == VideoProfile::Size {
        ensure!(
            duration > 0.0,
            "The 5 MB profile needs a known video duration"
        );
        let budget = 4.5 * 1024.0 * 1024.0 * 8.0 / duration - 64_000.0;
        ensure!(
            budget >= 1_000.0,
            "The video is too long for the 5 MB profile at 64 kb/s audio"
        );
        let mut scale = 1.0;
        while budget < f64::from(w) * f64::from(h) * scale * scale * 0.1 {
            scale *= 0.9;
        }
        let filter = format!(
            "scale={}:{}",
            even(f64::from(w) * scale),
            even(f64::from(h) * scale)
        );
        push(&[
            "-vf",
            &filter,
            "-b:v",
            &(budget as u64).to_string(),
            "-c:a",
            "aac",
            "-b:a",
            "64000",
        ]);
        scaled = true;
    } else {
        let target = match profile {
            VideoProfile::Crf480 => 480,
            VideoProfile::Crf720 => 720,
            _ => 0,
        };
        if target > 0 && w.min(h) > target {
            let scale = f64::from(target) / f64::from(w.min(h));
            let filter = format!(
                "scale={}:{}",
                even(f64::from(w) * scale),
                even(f64::from(h) * scale)
            );
            push(&["-vf", &filter]);
            scaled = true;
        }
        push(&[
            "-preset",
            &options.preset,
            "-crf",
            &options.crf.to_string(),
            "-c:a",
            "aac",
            "-b:a",
            "128k",
        ]);
    }
    if !scaled && (w % 2 != 0 || h % 2 != 0) {
        push(&["-vf", "pad=ceil(iw/2)*2:ceil(ih/2)*2"]);
    }
    Ok(args)
}

fn video(
    tools: &Tools,
    input: &Path,
    output: &Path,
    options: &Options,
    cancellation: &Cancellation,
    notify: &mut (impl FnMut(Update) + Send),
) -> Result<()> {
    let (stream, duration) = probe_video(
        tools.resolve(&tools.ffprobe, "ffprobe")?,
        input,
        cancellation,
    )?;
    let seconds = duration as u64;
    let time = if duration > 0.0 {
        if seconds >= 3600 {
            format!(
                "{}:{:02}:{:02}",
                seconds / 3600,
                seconds / 60 % 60,
                seconds % 60
            )
        } else {
            format!("{:02}:{:02}", seconds / 60, seconds % 60)
        }
    } else {
        String::new()
    };
    notify(Update::Details(Details {
        codec: stream.codec_name.clone(),
        duration: time,
        resolution: format!("{}×{}", stream.width, stream.height),
    }));

    let args = video_args(&stream, duration, options)?;
    let mut cmd = command(tools.resolve(&tools.ffmpeg, "ffmpeg")?);
    cmd.args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-i"])
        .arg(input)
        .args(args)
        .args(["-stats_period", "0.2", "-progress", "pipe:1", "-nostats"])
        .arg(output)
        .stdin(Stdio::null())
        .stdout(Stdio::piped());
    run_process(&mut cmd, "ffmpeg", cancellation, |stdout| {
        let stdout = stdout.expect("piped stdout");
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if duration > 0.0 {
                let seconds = line
                    .strip_prefix("out_time_us=")
                    .and_then(|value| value.parse::<f64>().ok());
                if let Some(micros) = seconds {
                    notify(Update::Progress(
                        (micros / 1_000_000.0 / duration).clamp(0.0, 0.99) as f32,
                    ));
                }
            }
        }
    })?;
    Ok(())
}

fn image(
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
    let mut cmd = magick_command(tools.resolve(&tools.magick, "magick")?);
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
        Kind::Video => video(
            tools,
            input,
            temp.path(),
            options,
            cancellation,
            &mut notify,
        )?,
        Kind::Image => image(
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
    let cancelled = cancellation.0.lock().unwrap();
    ensure!(!*cancelled, Cancelled);
    temp.persist_noclobber(output)
        .context("save converted file")?;
    Ok(bytes)
}
