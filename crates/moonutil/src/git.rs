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

use std::path::Path;

#[derive(Debug, thiserror::Error)]
#[error("git command failed: `{cmd}`")]
pub struct GitCommandError {
    cmd: String,

    #[source]
    source: GitCommandErrorKind,
}

#[derive(Debug, thiserror::Error)]
pub enum GitCommandErrorKind {
    #[error(transparent)]
    IO(#[from] std::io::Error),

    #[error("non-zero exit code: {0}")]
    ExitStatus(i32),

    #[error("unknown exit code")]
    UnknownExitCode,
}

pub struct Stdios {
    stdin: std::process::Stdio,
    stdout: std::process::Stdio,
    stderr: std::process::Stdio,
}

impl Stdios {
    pub fn inherit() -> Self {
        Self {
            stdin: std::process::Stdio::inherit(),
            stdout: std::process::Stdio::inherit(),
            stderr: std::process::Stdio::inherit(),
        }
    }

    pub fn npp() -> Self {
        Self {
            stdin: std::process::Stdio::null(),
            stdout: std::process::Stdio::piped(),
            stderr: std::process::Stdio::piped(),
        }
    }
}
pub fn git_command(args: &[&str], stdios: Stdios) -> Result<std::process::Child, GitCommandError> {
    std::process::Command::new("git")
        .args(args)
        .stdin(stdios.stdin)
        .stdout(stdios.stdout)
        .stderr(stdios.stderr)
        .spawn()
        .map_err(|e| GitCommandError {
            cmd: format!("git {}", args.join(" ")),
            source: GitCommandErrorKind::IO(e),
        })
}

pub fn is_in_git_repo(path: &Path) -> Result<bool, GitCommandError> {
    let args = [
        "-C",
        path.to_str().unwrap(),
        "rev-parse",
        "--is-inside-work-tree",
    ];
    let mut output = git_command(&args, Stdios::npp())?;
    let status = output.wait();
    match status {
        Ok(status) => Ok(status.success()),
        Err(e) => Err(GitCommandError {
            cmd: format!("git {}", args.join(" ")),
            source: GitCommandErrorKind::IO(e),
        }),
    }
}

pub fn git_init_repo(path: &Path) -> Result<(), GitCommandError> {
    let args = ["-C", path.to_str().unwrap(), "init"];
    let mut git_init = git_command(&args, Stdios::inherit())?;
    let status = git_init.wait().map_err(|e| GitCommandError {
        cmd: format!("git {}", args.join(" ")),
        source: GitCommandErrorKind::IO(e),
    })?;
    if !status.success() {
        match status.code() {
            Some(code) => {
                return Err(GitCommandError {
                    cmd: format!("git {}", args.join(" ")),
                    source: GitCommandErrorKind::ExitStatus(code),
                });
            }
            None => {
                return Err(GitCommandError {
                    cmd: format!("git {}", args.join(" ")),
                    source: GitCommandErrorKind::UnknownExitCode,
                })
            }
        }
    }
    Ok(())
}
