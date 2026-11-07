use crate::{
    jobs::{Event, Queue},
    media::{ImageFormat, ImageMode, Kind, Options, Update, VideoProfile},
};
use clap::Parser;
use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    io::{self, IsTerminal, Write},
    path::PathBuf,
};

#[derive(Debug, Parser)]
#[command(
    name = "compress",
    version,
    about = "Compress videos and photos. No arguments opens the GUI."
)]
struct Mixed {
    #[arg(required = true, value_name = "FILE_OR_FOLDER")]
    inputs: Vec<PathBuf>,
    #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u8).range(0..=51))]
    crf: u8,
    #[arg(long, default_value = "slow")]
    preset: String,
    #[arg(long, value_enum, default_value = "crf")]
    video_profile: VideoProfile,
    #[arg(long, value_enum, default_value = "lossy")]
    image_mode: ImageMode,
    #[arg(long, value_enum, default_value = "jpg")]
    image_format: ImageFormat,
}

#[derive(Debug, Parser)]
#[command(
    name = "compress image",
    about = "Convert photos. Folders are searched recursively."
)]
struct Photos {
    #[arg(required = true, value_name = "FILE_OR_FOLDER")]
    inputs: Vec<PathBuf>,
    #[arg(long, value_enum, default_value = "lossy")]
    mode: ImageMode,
    #[arg(long, value_enum, default_value = "jpg")]
    format: ImageFormat,
}

#[derive(Debug, Parser)]
#[command(
    name = "compress 5",
    about = "Convert videos to the legacy 5 MB profile."
)]
struct Size {
    #[arg(required = true, value_name = "FILE_OR_FOLDER")]
    inputs: Vec<PathBuf>,
}

pub fn wants_gui(args: &[OsString]) -> bool {
    args.first()
        .is_none_or(|arg| arg == "gui" || arg == "--gui")
}

fn parse(args: Vec<OsString>) -> Result<(Vec<PathBuf>, Options), clap::Error> {
    let first = args.first().and_then(|arg| arg.to_str());
    match first {
        Some("image" | "images" | "photo" | "photos") => {
            let photos = Photos::try_parse_from(
                std::iter::once("compress image".into()).chain(args.into_iter().skip(1)),
            )?;
            Ok((
                photos.inputs,
                Options {
                    only: Some(Kind::Image),
                    image_mode: photos.mode,
                    image_format: photos.format,
                    ..Options::default()
                },
            ))
        }
        Some("5") => {
            let size = Size::try_parse_from(
                std::iter::once("compress 5".into()).chain(args.into_iter().skip(1)),
            )?;
            Ok((
                size.inputs,
                Options {
                    only: Some(Kind::Video),
                    video_profile: VideoProfile::Size,
                    ..Options::default()
                },
            ))
        }
        _ => {
            // Python accepts these historical multi-letter single-dash flags.
            let mut literal = false;
            let args = args.into_iter().map(|arg| {
                if arg == "--" {
                    literal = true;
                }
                if !literal
                    && let Some(arg) = arg.to_str()
                    && (arg == "-crf"
                        || arg.starts_with("-crf=")
                        || arg == "-preset"
                        || arg.starts_with("-preset="))
                {
                    return format!("-{arg}").into();
                }
                arg
            });
            let opts = Mixed::try_parse_from(std::iter::once("compress".into()).chain(args))?;
            Ok((
                opts.inputs,
                Options {
                    crf: opts.crf,
                    preset: opts.preset,
                    video_profile: opts.video_profile,
                    image_mode: opts.image_mode,
                    image_format: opts.image_format,
                    ..Options::default()
                },
            ))
        }
    }
}

pub fn run(args: Vec<OsString>) -> u8 {
    if args
        .first()
        .is_some_and(|arg| arg == "version" || arg == "--version" || arg == "-V")
    {
        let _ = writeln!(io::stdout(), "{}", crate::VERSION);
        return 0;
    }
    let (inputs, options) = match parse(args) {
        Ok(config) => config,
        Err(error) => {
            let _ = error.print();
            return if error.use_stderr() { 2 } else { 0 };
        }
    };
    let only = options.only;
    let queue = Queue::new(|| {});
    queue.submit(inputs, options);
    let mut jobs = HashMap::new();
    let mut done_scanning = false;
    let mut failed = false;
    let mut found = false;
    let terminal = io::stderr().is_terminal();
    for event in &queue.events {
        match event {
            Event::Queued(job) => {
                found = true;
                jobs.insert(job.id, (job, 0));
            }
            Event::Started(id) => {
                let (job, _) = &jobs[&id];
                let _ = writeln!(
                    io::stderr(),
                    "Starting {}: {}",
                    job.options.label(job.kind),
                    job.input.display()
                );
            }
            Event::Update(id, Update::Progress(progress)) if terminal => {
                let (job, last) = jobs.get_mut(&id).expect("queued job");
                let step = (progress * 10.0) as u8;
                if step > *last {
                    *last = step;
                    let name = job.input.file_name().unwrap_or(OsStr::new(""));
                    let _ = writeln!(io::stderr(), "{}: {}%", name.to_string_lossy(), step * 10);
                }
            }
            Event::Finished(id, result) => {
                let (job, _) = jobs.remove(&id).expect("queued job");
                match result {
                    Ok(bytes) => {
                        let _ = writeln!(
                            io::stdout(),
                            "{} ({:.2} MB)",
                            job.output.display(),
                            mb(bytes)
                        );
                    }
                    Err(error) => {
                        failed = true;
                        let _ = writeln!(io::stderr(), "Error {}: {error}", job.input.display());
                    }
                }
            }
            Event::Cancelled(id) => {
                jobs.remove(&id);
                failed = true;
            }
            Event::ScanProblem(error) => {
                failed = true;
                let _ = writeln!(io::stderr(), "{error}");
            }
            Event::ScanDone => done_scanning = true,
            Event::Update(..) => {}
        }
        if done_scanning && jobs.is_empty() {
            break;
        }
    }
    if !found && !failed {
        let message = match only {
            Some(Kind::Video) => "No videos found.",
            Some(Kind::Image) => "No images found.",
            None => "No supported media found.",
        };
        let _ = writeln!(io::stdout(), "{message}");
    }
    u8::from(failed)
}

pub fn mb(bytes: u64) -> f64 {
    bytes as f64 / 1_048_576.0
}
