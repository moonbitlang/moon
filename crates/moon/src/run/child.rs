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
use moonbuild::section_capture::{SectionCapture, handle_stdout_async};
use moonutil::platform::macos_with_sigchild_blocked;
use tokio::process::Command;
use crate::signal_handler::make_signal_aware;

/// Run a command under the governing of `moon run`.
///
/// If `capture` is set, the output will be captured and can be processed (like
/// `moon test`). Otherwise, the output will be directly inherited from the
/// parent process (like `moon run`).
///
/// `stdout` and `stderr` is always piped if `capture` is true, to prevent the
/// subprocess changing the property of file descriptors (`node` is notorious
/// for this, see [moon#852](https://github.com/moonbitlang/moon/issues/852)).
///
/// `captures` uses a list of [`SectionCapture`] to capture part of the `stdout`
/// output since the running process might not have any other method to interact
/// with the host `moon` process.
pub async fn run<'a>(
    captures: &mut [&mut SectionCapture<'a>],
    capture: bool,
    mut cmd: Command,
) -> anyhow::Result<ExitStatus> {
    if capture {
        // If we want to capture some/all of the output, we want to set piped
        // to both streams to prevent `node` and friends changing fd blocking
        // status
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
    } else {
        assert!(
            captures.is_empty(),
            "Can't have section captures when not capturing stdout"
        );
        // We aren't capturing, so YOLO
        cmd.stdin(Stdio::inherit());
        cmd.stdout(Stdio::inherit());
        cmd.stderr(Stdio::inherit());
    }
    cmd.kill_on_drop(true); // to prevent zombie processes;

    // Preventing race conditions with SIGCHLD handlers, see definition for info
    let raw_child = macos_with_sigchild_blocked(|| {
        cmd.spawn()
            .with_context(|| format!("Failed to spawn command {:?}", cmd))
    })?;
    
    let mut child = make_signal_aware(raw_child);

    // Task only exists when capturing
    let stderr_pipe_task = child.inner_mut().stderr.take().map(|mut stderr| {
        tokio::spawn(async move {
            let mut proc_stderr = tokio::io::stderr();
            tokio::io::copy(&mut stderr, &mut proc_stderr)
                .await
                .context("Failed to pipe stderr to child process")
        })
    });

    // Since we cannot have scoped async tasks here, and we borrow the capture
    // sections, we'll handle stdout in this main task
    if capture {
        let child_stdout = child
            .inner_mut()
            .stdout
            .take()
            .expect("Child process should have stdout piped");

        if !captures.is_empty() {
            let buf_stdout = tokio::io::BufReader::new(child_stdout);
            handle_stdout_async(buf_stdout, captures).await?;
        } else {
            let mut child_stdout = child_stdout;
            let mut proc_stdout = tokio::io::stdout();
            tokio::io::copy(&mut child_stdout, &mut proc_stdout).await?;
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
