use easy_media_compress::cli;
use std::{
    path::PathBuf,
    process::{Command, ExitCode},
};

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if !cli::wants_gui(&args) {
        return cli::run(args).into();
    }
    let exe = std::env::current_exe()
        .map(|p| p.with_file_name(format!("EasyMediaCompress{}", std::env::consts::EXE_SUFFIX)))
        .unwrap_or_else(|_| PathBuf::from("EasyMediaCompress"));
    match Command::new(exe).args(args).spawn() {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Cannot open GUI: {error}. Keep both app executables together.");
            ExitCode::FAILURE
        }
    }
}
