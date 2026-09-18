// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
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
    let output = git_command(&args, Stdios::npp())?.wait_with_output();
    match output {
        Ok(output) => Ok(output.status.success()),
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
                });
            }
        }
    }
    Ok(())
}

#[test]
fn test_bad_git_command() {
    fn fake_git_command(
        args: &[&str],
        stdios: Stdios,
    ) -> Result<std::process::Child, GitCommandError> {
        std::process::Command::new("fake_git")
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
    let child = fake_git_command(&["pull"], Stdios::inherit());
    let e = child.unwrap_err();
    let err = anyhow::anyhow!(e);
    let err_msg = format!("{err:?}");
    assert!(err_msg.contains("git command failed: `git pull`"));
    assert!(err_msg.contains("Caused by:"));
}
