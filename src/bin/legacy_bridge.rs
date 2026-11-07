#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod windows {
    use anyhow::{Context, Result, ensure};
    use std::{
        env,
        fs::File,
        io::{self, Read, Seek, SeekFrom, Write},
        os::windows::{ffi::OsStringExt, process::CommandExt},
        path::PathBuf,
        process::Command,
    };
    use windows_sys::Win32::{
        System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_SZ, RegGetValueW},
        UI::WindowsAndMessaging::{MB_ICONERROR, MessageBoxW},
    };

    const MAGIC: &[u8; 8] = b"EMCSETUP";

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain([0]).collect()
    }

    fn installed_dir() -> Option<PathBuf> {
        let mut data = [0u16; 32768];
        let mut bytes = size_of_val(&data) as u32;
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                wide(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\EasyMediaCompressRust_is1").as_ptr(),
                wide("InstallLocation").as_ptr(),
                RRF_RT_REG_SZ, std::ptr::null_mut(),
                data.as_mut_ptr().cast(), &mut bytes,
            )
        };
        (status == 0 && bytes > 2)
            .then(|| std::ffi::OsString::from_wide(&data[..bytes as usize / 2 - 1]).into())
    }

    fn install() -> Result<bool> {
        let mut source = File::open(env::current_exe()?)?;
        let footer = source.seek(SeekFrom::End(-16))?;
        let mut size = [0; 8];
        let mut magic = [0; 8];
        source.read_exact(&mut size)?;
        source.read_exact(&mut magic)?;
        let size = u64::from_le_bytes(size);
        ensure!(
            &magic == MAGIC && size > 0 && size < footer,
            "Missing embedded installer"
        );
        source.seek(SeekFrom::Start(footer - size))?;
        let directory = tempfile::Builder::new()
            .prefix("easy-media-compress-setup-")
            .tempdir()?;
        let installer = directory.path().join("EasyMediaCompress-Setup-x64.exe");
        let mut file = File::create(&installer)?;
        ensure!(
            io::copy(&mut source.take(size), &mut file)? == size,
            "Incomplete installer"
        );
        file.flush()?;
        drop(file);
        // The bridge owns relaunch so the original arguments are forwarded once.
        let result = Command::new(installer)
            .args(["/NORESTART", "/LEGACYBRIDGE"])
            .status()?;
        if result.success() {
            return Ok(true);
        }
        // Inno Setup uses 2 and 5 for cancellation before/during installation.
        ensure!(
            matches!(result.code(), Some(2 | 5)),
            "Setup exited with {result}"
        );
        Ok(false)
    }

    fn version(value: &str) -> Option<[u32; 3]> {
        value
            .trim()
            .split('.')
            .map(str::parse::<u32>)
            .collect::<Result<Vec<_>, _>>()
            .ok()?
            .try_into()
            .ok()
    }

    pub fn run() -> Result<()> {
        let current = version(env!("CARGO_PKG_VERSION")).expect("numeric release version");
        let up_to_date = installed_dir().is_some_and(|dir| {
            Command::new(dir.join("compress.exe"))
                .arg("--version")
                .creation_flags(0x0800_0000)
                .output()
                .ok()
                .filter(|output| output.status.success())
                .and_then(|output| version(&String::from_utf8_lossy(&output.stdout)))
                .is_some_and(|installed| installed >= current)
                && dir.join("EasyMediaCompress.exe").is_file()
        });
        if !up_to_date && !install()? {
            return Ok(());
        }
        let dir = installed_dir().context("Could not find the installed application")?;
        Command::new(dir.join("EasyMediaCompress.exe"))
            .args(env::args_os().skip(1))
            .spawn()
            .context("Could not launch the installed application")?;
        Ok(())
    }

    pub fn show_error(message: &str) {
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                wide(message).as_ptr(),
                wide("Easy Media Compress").as_ptr(),
                MB_ICONERROR,
            );
        }
    }
}

fn main() -> std::process::ExitCode {
    #[cfg(windows)]
    if let Err(error) = windows::run() {
        windows::show_error(&format!("{error:#}"));
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}
