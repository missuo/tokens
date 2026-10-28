//! Windowless launcher for the scheduled autosubmit task on Windows.
//!
//! `tokens.exe` is a console program, so every time Task Scheduler starts it
//! Windows opens a console window for it, and an interval of a few minutes
//! means a terminal flashing up all day. This binary is built for the GUI
//! subsystem, so starting it shows nothing. It runs the `tokens.exe` next to it
//! with `CREATE_NO_WINDOW`, which gives that process a console with no window;
//! anything `tokens.exe` spawns in turn inherits that console, so it stays
//! hidden too. Same pattern as `pythonw.exe` / `javaw.exe`.
//!
//! It is built on every target so the workspace compiles the same everywhere,
//! but only the Windows packages ship it.

#![cfg_attr(windows, windows_subsystem = "windows")]

use std::process::{Command, Stdio};

#[cfg(windows)]
const TOKENS_EXE: &str = "tokens.exe";
#[cfg(not(windows))]
const TOKENS_EXE: &str = "tokens";

fn main() {
    let Ok(launcher) = std::env::current_exe() else {
        std::process::exit(1);
    };

    let mut command = Command::new(launcher.with_file_name(TOKENS_EXE));
    // A GUI-subsystem process has no standard handles to pass on. Hand the
    // child NUL explicitly instead of letting it inherit nothing.
    command
        .args(std::env::args_os().skip(1))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    // Task Scheduler records this as the task's last result, so pass the
    // child's exit code through rather than flattening it to success/failure.
    let code = match command.status() {
        Ok(status) => status.code().unwrap_or(1),
        Err(_) => 1,
    };
    std::process::exit(code);
}
