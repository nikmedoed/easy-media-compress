use anyhow::{Context, Result};
use std::{
    env,
    ffi::OsString,
    fs::{self, File, TryLockError},
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

pub struct Inbox {
    _lock: File,
    directory: PathBuf,
}

impl Inbox {
    /// A secondary launch publishes a complete request before waiting for
    /// acknowledgement. If the window exits in the meantime, it takes over.
    pub fn open(inputs: &mut Vec<PathBuf>) -> Result<Option<Self>> {
        let directory =
            PathBuf::from(env::var_os("LOCALAPPDATA").context("LOCALAPPDATA is unavailable")?)
                .join("EasyMediaCompressRust")
                .join(if cfg!(debug_assertions) {
                    "inbox-dev"
                } else {
                    "inbox"
                });
        Self::open_in(directory, inputs)
    }

    fn open_in(directory: PathBuf, inputs: &mut Vec<PathBuf>) -> Result<Option<Self>> {
        fs::create_dir_all(&directory)?;
        let lock = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(directory.join("window.lock"))?;
        if try_lock(&lock)? {
            return Ok(Some(Self {
                _lock: lock,
                directory,
            }));
        }

        // Resolve relative paths in the sender's working directory.
        let paths = inputs
            .iter()
            .map(|path| {
                let path = std::path::absolute(path)?;
                Ok(path.as_os_str().encode_wide().collect::<Vec<_>>())
            })
            .collect::<std::io::Result<Vec<_>>>()?;
        let mut request = tempfile::Builder::new()
            .prefix("pending-")
            .tempfile_in(&directory)?;
        serde_json::to_writer(request.as_file_mut(), &paths)?;
        let published = request.path().with_extension("request");
        request.persist_noclobber(&published)?;
        inputs.clear();

        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if !published.try_exists()? {
                return Ok(None);
            }
            if try_lock(&lock)? {
                // The old window might have consumed this just before closing.
                return Ok(published.try_exists()?.then_some(Self {
                    _lock: lock,
                    directory,
                }));
            }
            if Instant::now() >= deadline {
                // The window can be blocked in the system file picker. It will
                // read the request when it resumes, or on the next launch.
                return Ok(None);
            }
            thread::sleep(Duration::from_millis(35));
        }
    }

    pub fn receive(&self) -> Result<Option<Vec<PathBuf>>> {
        for entry in self.directory.read_dir()? {
            let path = entry?.path();
            if path.extension().is_none_or(|ext| ext != "request") {
                continue;
            }
            let result = read_request(&path);
            fs::remove_file(&path)?;
            return result.map(Some);
        }
        Ok(None)
    }
}

fn try_lock(file: &File) -> Result<bool> {
    match file.try_lock() {
        Ok(()) => Ok(true),
        Err(TryLockError::WouldBlock) => Ok(false),
        Err(TryLockError::Error(error)) => Err(error.into()),
    }
}

fn read_request(path: &Path) -> Result<Vec<PathBuf>> {
    let file = File::open(path)?;
    anyhow::ensure!(
        file.metadata()?.len() <= 512 * 1024,
        "Explorer request is too large"
    );
    let paths = serde_json::from_reader::<_, Vec<Vec<u16>>>(file)?;
    Ok(paths
        .into_iter()
        .map(|path| OsString::from_wide(&path).into())
        .collect())
}
