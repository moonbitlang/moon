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

//! Handles spawning of a child process under the govern of `moon run`

use std::process::{ExitStatus, Stdio};

use anyhow::Context;
use moonbuild::section_capture::{handle_stdout_async, SectionCapture};
use tokio::process::Command;

/// Run a command under the governing of `moon run`.
///
/// `stdout` and `stderr` is always piped, to prevent the subprocess changing
/// the property of file descriptors (`node` is notorious for this, see
/// [moon#852](https://github.com/moonbitlang/moon/issues/852)).
///
/// `captures` uses a list of [`SectionCapture`] to capture part of the `stdout`
/// output since the running process might not have any other method to interact
/// with the host `moon` process.
pub async fn run<'a>(
    captures: &mut [&mut SectionCapture<'a>],
    stdin: bool,
    mut cmd: Command,
) -> anyhow::Result<ExitStatus> {
    if stdin {
        cmd.stdin(Stdio::inherit());
    } else {
        cmd.stdin(Stdio::null());
    }
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped()); // to prevent `node` and friends changing fd blocking status
    cmd.kill_on_drop(true); // to prevent zombie processes;

    let mut child = cmd
        .spawn()
        .with_context(|| format!("Failed to spawn command {:?}", cmd))?;

    let stderr_pipe_task = child.stderr.take().map(|mut stderr| {
        tokio::spawn(async move {
            let mut proc_stderr = tokio::io::stderr();
            tokio::io::copy(&mut stderr, &mut proc_stderr)
                .await
                .context("Failed to pipe stderr to child process")
        })
    });

    // Since we cannot have scoped async tasks here, and we borrow the capture
    // sections, we'll handle stdout in this main task
    let child_stdout = child
        .stdout
        .take()
        .expect("Child process should have stdout piped");
    {
        let mut buf_stdout = tokio::io::BufReader::new(child_stdout);
        if !captures.is_empty() {
            handle_stdout_async(buf_stdout, captures).await?;
        } else {
            let mut stdout = tokio::io::stdout();
            tokio::io::copy_buf(&mut buf_stdout, &mut stdout).await?;
        }
    }

    // Wait for the child process to finish
    let status = child
        .wait()
        .await
        .context("Failed to wait for child process")?;

    if let Some(task) = stderr_pipe_task {
        task.await
            .expect("Failed to pipe stderr to child process")?;
    }

    Ok(status)
}
