// moon: The build system and package manager for MoonBit.
// Copyright (C) 2024 International Digital Economy Academy
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug)]
struct CommandOutput {
    stdout: String,
    stderr: String,
    exit_code: u8,
}

fn canonicalize_or_self(path: &Path) -> PathBuf {
    dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn moon_home() -> Option<PathBuf> {
    if let Ok(moon_home) = std::env::var("MOON_HOME") {
        return Some(PathBuf::from(moon_home));
    }

    home::home_dir().map(|h| h.join(".moon"))
}

fn normalize_output(output: &str, workdir: &Path) -> String {
    let mut redactions = snapbox::Redactions::new();

    redactions
        .insert("[WORK_DIR]", canonicalize_or_self(workdir))
        .expect("valid WORK_DIR redaction");

    if let Some(moon_home) = moon_home() {
        redactions
            .insert("[MOON_HOME]", canonicalize_or_self(&moon_home))
            .expect("valid MOON_HOME redaction");
    }

    let normalized = output
        .replace("\\\\", "\\")
        .replace("${WORK_DIR}", "[WORK_DIR]")
        .replace("$MOON_HOME", "[MOON_HOME]");

    redactions.redact(&normalized).replace("\r\n", "\n")
}

fn run_process(program: &Path, args: &[&str], workdir: &Path) -> CommandOutput {
    match Command::new(program)
        .args(args)
        .current_dir(workdir)
        .output()
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
            let mut stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            stderr = stderr
                .split('\n')
                .filter(|line| !line.starts_with("Blocking waiting for file lock"))
                .collect::<Vec<_>>()
                .join("\n");

            CommandOutput {
                stdout,
                stderr,
                exit_code: output.status.code().map_or(255, |code| code as u8),
            }
        }
        Err(err) => CommandOutput {
            stdout: String::new(),
            stderr: err.to_string(),
            exit_code: err.raw_os_error().map_or(255, |code| code as u8),
        },
    }
}

fn run_xcat(args: &[&str], workdir: &Path) -> CommandOutput {
    let stdout = if args.is_empty() {
        "no file specified".to_string()
    } else {
        let file_path = workdir.join(args[0]);
        match fs::read_to_string(file_path) {
            Ok(content) => content,
            Err(err) => format!("failed to read file: {}", err),
        }
    };

    CommandOutput {
        stdout,
        stderr: String::new(),
        exit_code: 0,
    }
}

fn run_xls(args: &[&str], workdir: &Path) -> CommandOutput {
    let dir = if args.is_empty() {
        workdir.to_path_buf()
    } else {
        workdir.join(args[0])
    };

    let stdout = match fs::read_dir(dir) {
        Ok(entries) => {
            let mut files = entries
                .filter_map(Result::ok)
                .map(|entry| entry.file_name().to_string_lossy().to_string())
                .collect::<Vec<_>>();
            files.sort();
            files.join(" ")
        }
        Err(err) => format!("failed to list files: {}", err),
    };

    CommandOutput {
        stdout,
        stderr: String::new(),
        exit_code: 0,
    }
}

pub(crate) fn execute_command(cmd: &str, args: &[&str], workdir: &Path, moon_bin: &Path) -> String {
    let output = match cmd {
        "moon" => run_process(moon_bin, args, workdir),
        "xcat" => run_xcat(args, workdir),
        "xls" => run_xls(args, workdir),
        _ => run_process(Path::new(cmd), args, workdir),
    };

    let actual = if output.stderr.is_empty() {
        output.stdout
    } else {
        format!("{}\n{}", output.stdout, output.stderr)
    };
    let actual = if output.exit_code != 0 {
        format!("[{}]\n{}", output.exit_code, actual)
    } else {
        actual
    };

    normalize_output(&actual, workdir)
}
