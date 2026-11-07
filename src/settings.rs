use crate::media::{ImageMode, VideoProfile};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Settings {
    pub video_profile: VideoProfile,
    pub image_mode: ImageMode,
    pub window_size: Option<[f32; 2]>,
}

fn root() -> Option<PathBuf> {
    if cfg!(windows) {
        Some(std::env::var_os("LOCALAPPDATA")?.into())
    } else if cfg!(target_os = "macos") {
        Some(PathBuf::from(std::env::var_os("HOME")?).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| Some(PathBuf::from(std::env::var_os("HOME")?).join(".config")))
    }
}

fn names() -> [&'static str; 3] {
    if cfg!(target_os = "linux") {
        [
            "easy-media-compress-rust",
            "easy-media-compress",
            "media-compress",
        ]
    } else {
        [
            "EasyMediaCompressRust",
            "EasyMediaCompress",
            "MediaCompress",
        ]
    }
}

impl Settings {
    pub fn load() -> Self {
        let Some(root) = root() else {
            return Self::default();
        };
        for dir in names() {
            let Ok(file) = fs::read(root.join(dir).join("settings.json")) else {
                continue;
            };
            let Ok(value) = serde_json::from_slice::<serde_json::Value>(&file) else {
                continue;
            };
            if !value.is_object() {
                continue;
            }
            let video_profile = serde_json::from_value(value["video_profile"].clone())
                .unwrap_or_else(|_| {
                    if value["video_size_mode"].as_bool() == Some(true) {
                        VideoProfile::Size
                    } else {
                        VideoProfile::Crf
                    }
                });
            let image_mode =
                serde_json::from_value(value["image_mode"].clone()).unwrap_or_default();
            let window_size = serde_json::from_value(value["window_size"].clone())
                .ok()
                .or_else(|| {
                    let geometry = value["window_geometry"].as_str()?;
                    let size = geometry.split(['+', '-']).next()?;
                    let (w, h) = size.split_once('x')?;
                    Some([w.parse().ok()?, h.parse().ok()?])
                })
                .filter(|[w, h]: &[f32; 2]| {
                    (500.0..=5000.0).contains(w) && (250.0..=4000.0).contains(h)
                });
            return Self {
                video_profile,
                image_mode,
                window_size,
            };
        }
        Self::default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        use std::io::Write;
        let Some(root) = root() else { return Ok(()) };
        let dir = root.join(names()[0]);
        fs::create_dir_all(&dir)?;
        let mut file = tempfile::NamedTempFile::new_in(&dir)?;
        file.write_all(&serde_json::to_vec_pretty(self)?)?;
        file.persist(dir.join("settings.json"))?;
        Ok(())
    }
}
