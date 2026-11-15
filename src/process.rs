use anyhow::{Context, Result, anyhow, ensure};
use std::{
    io::Read,
    path::Path,
    process::{ChildStdout, Command, Stdio},
    sync::{Mutex, MutexGuard},
    thread,
    time::Duration,
};

pub(crate) fn command(exe: &Path) -> Command {
    let mut cmd = Command::new(exe);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd
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

    pub(crate) fn lock(&self) -> MutexGuard<'_, bool> {
        self.0.lock().unwrap()
    }

    pub(crate) fn check(&self) -> Result<()> {
        ensure!(!self.is_cancelled(), Cancelled);
        Ok(())
    }
}

pub(crate) fn capture(mut reader: impl Read, limit: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    let _ = (&mut reader).take(limit).read_to_end(&mut bytes);
    let _ = std::io::copy(&mut reader, &mut std::io::sink());
    bytes
}

pub(crate) fn run_process<T: Send>(
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
            let message = String::from_utf8_lossy(&stderr);
            let message = message.trim();
            return Err(if message.is_empty() {
                anyhow!("{name} exited with {status}")
            } else {
                anyhow!("{name}: {message}")
            });
        }
        Ok((stdout, stderr))
    })
}
