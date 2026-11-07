#![cfg_attr(windows, windows_subsystem = "windows")]

mod gui;
#[cfg(windows)]
mod inbox;
#[cfg(windows)]
mod updates;

use easy_media_compress::cli;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if !cli::wants_gui(&args) {
        #[cfg(windows)]
        // The console-subsystem compress.exe is preferable for shell scripts
        // that need to wait for an exit code or pipe stdout.
        unsafe {
            use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
        return cli::run(args).into();
    }
    let inputs = args.into_iter().skip(1).map(Into::into).collect();
    #[cfg(windows)]
    let (inputs, inbox) = {
        let mut inputs = inputs;
        match inbox::Inbox::open(&mut inputs) {
            Ok(Some(inbox)) => (inputs, inbox),
            Ok(None) => return ExitCode::SUCCESS,
            Err(error) => {
                show_error(&format!(
                    "Could not send files to Easy Media Compress:\n{error:#}"
                ));
                return ExitCode::FAILURE;
            }
        }
    };
    match gui::run(
        inputs,
        #[cfg(windows)]
        inbox,
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            show_error(&format!("Could not open Easy Media Compress:\n{error}"));
            ExitCode::FAILURE
        }
    }
}

#[cfg(windows)]
fn show_error(message: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MessageBoxW};
    let wide = |s: &str| {
        s.encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>()
    };
    let title = wide(easy_media_compress::APP_NAME);
    let text = wide(message);
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_ICONERROR,
        );
    }
}

#[cfg(not(windows))]
fn show_error(message: &str) {
    eprintln!("{message}");
}
