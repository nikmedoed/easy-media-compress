use super::Kind;
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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

    pub(super) fn gif_target(self) -> Option<u32> {
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
    pub(super) fn extension(self) -> &'static str {
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
