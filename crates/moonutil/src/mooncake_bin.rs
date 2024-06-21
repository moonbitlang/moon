//! Interacts with the `mooncake` binary

use std::path::PathBuf;

fn determine_moon_bin() -> Option<PathBuf> {
    // Check if the `mooncake` binary is in the executable's directory
    let curr_exe = std::env::current_exe();
    if let Ok(curr_exe) = curr_exe {
        let mut moon_bin = curr_exe.clone();
        moon_bin.set_file_name("moon");
        #[cfg(windows)]
        {
            moon_bin.set_extension("exe");
        }
        if moon_bin.is_file() {
            return Some(moon_bin);
        }
    }
    None
}

pub fn call_moon_from_mooncake() -> std::process::Command {
    std::process::Command::new(determine_moon_bin().unwrap_or_else(|| "moon".into()))
}

fn determine_mooncake_bin() -> Option<PathBuf> {
    // Check if the `mooncake` binary is in the executable's directory
    let curr_exe = std::env::current_exe();
    if let Ok(curr_exe) = curr_exe {
        let mut mooncake_bin = curr_exe.clone();
        mooncake_bin.set_file_name("mooncake");
        #[cfg(windows)]
        {
            mooncake_bin.set_extension("exe");
        }
        if mooncake_bin.is_file() {
            return Some(mooncake_bin);
        }
    }
    None
}

pub fn call_mooncake() -> std::process::Command {
    std::process::Command::new(determine_mooncake_bin().unwrap_or_else(|| "mooncake".into()))
}
