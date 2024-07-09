use std::io::Write;

pub mod bench;
pub mod build;
pub mod bundle;
pub mod check;
pub mod doc_http;
pub mod dry_run;
pub mod entry;
pub mod expect;
pub mod fmt;
pub mod gen;
pub mod new;
pub mod runtest;
pub mod section_capture;
pub mod upgrade;

use sysinfo::{ProcessExt, System, SystemExt};

pub const MOON_PID_NAME: &str = ".moon.pid";

pub fn bail_moon_check_is_running(p: &std::path::Path) -> anyhow::Result<i32> {
    anyhow::bail!(
        "`moon check` is already running. If you are certain it is not running, you may want to manually delete `{}` and try again.",
        p.to_str().unwrap_or(MOON_PID_NAME)
    )
}

pub fn write_current_pid(
    target_dir: &std::path::Path,
    pid_path: &std::path::Path,
) -> anyhow::Result<()> {
    std::fs::create_dir_all(target_dir)?;
    let pid = std::process::id();
    let mut pid_file = std::fs::File::create(pid_path)?;
    pid_file.write_all(pid.to_string().as_bytes())?;
    Ok(())
}

pub fn watcher_is_running(pid_path: &std::path::Path) -> anyhow::Result<bool> {
    if !pid_path.exists() {
        return Ok(false);
    }

    let pid = std::fs::read_to_string(pid_path)?;
    let pid = pid.parse::<usize>()?;
    let pid = sysinfo::Pid::from(pid);
    let mut sys = System::new();
    sys.refresh_processes();
    if let Some(p) = sys.process(pid) {
        if p.name() == "moon" {
            Ok(true)
        } else {
            Ok(false)
        }
    } else {
        Ok(false)
    }
}
