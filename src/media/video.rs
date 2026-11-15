use super::{Cancellation, Details, Options, Tools, Update, VideoProfile};
use crate::process::{capture, command, run_process};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::Stdio,
};

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

pub(super) fn encode(
    tools: &Tools,
    input: &Path,
    output: &Path,
    options: &Options,
    cancellation: &Cancellation,
    notify: &mut (impl FnMut(Update) + Send),
) -> Result<()> {
    let (stream, duration) = probe_video(tools.ffprobe()?, input, cancellation)?;
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
    let mut cmd = command(tools.ffmpeg()?);
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
