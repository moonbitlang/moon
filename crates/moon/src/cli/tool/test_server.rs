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
    ffi::OsString,
    fs::File,
    io::{BufRead, Read, Seek, SeekFrom, Write},
    path::PathBuf,
};

use anyhow::Context;
use clap::Parser;
use colored::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, clap::Parser)]
pub(crate) struct TestServerSubcommand {}

#[derive(Debug, Deserialize)]
struct TestServerRequest {
    cwd: PathBuf,
    args: Vec<String>,
    envs: Vec<(String, String)>,
    response_path: PathBuf,
}

#[derive(Debug, Serialize)]
struct TestServerResponse {
    exit_code: i32,
    stdout: String,
    stderr: String,
}

pub(crate) fn run_test_server(_: TestServerSubcommand) -> anyhow::Result<i32> {
    let stdin = std::io::stdin();
    let mut reader = std::io::BufReader::new(stdin.lock());
    let mut line = String::new();

    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Ok(0);
        }

        let request: TestServerRequest =
            serde_json::from_str(&line).context("failed to decode test-server request")?;
        let response = execute_request(&request);
        let response =
            serde_json::to_vec(&response).context("failed to encode test-server response")?;
        std::fs::write(&request.response_path, response).with_context(|| {
            format!(
                "failed to write test-server response to {}",
                request.response_path.display()
            )
        })?;
    }
}

fn execute_request(request: &TestServerRequest) -> TestServerResponse {
    match capture_stdio(|| run_request(request)) {
        Ok(response) => response,
        Err(err) => TestServerResponse {
            exit_code: -1,
            stdout: String::new(),
            stderr: format!("{}: {:?}\n", "error".red().bold(), err),
        },
    }
}

fn run_request(request: &TestServerRequest) -> i32 {
    let _env_guard = ScopedEnv::apply(&request.envs);

    if let Err(err) = std::env::set_current_dir(&request.cwd) {
        eprintln!(
            "{}: failed to change directory to {}: {}",
            "error".red().bold(),
            request.cwd.display(),
            err
        );
        return -1;
    }

    let cli = match parse_request_args(&request.args) {
        Ok(cli) => cli,
        Err(code) => return code,
    };

    crate::run_parsed_cli(cli, crate::TracingMode::Skip)
}

fn parse_request_args(args: &[String]) -> Result<crate::cli::MoonBuildCli, i32> {
    let argv = std::iter::once(OsString::from("moon")).chain(args.iter().map(OsString::from));
    match crate::cli::MoonBuildCli::try_parse_from(argv) {
        Ok(cli) => Ok(cli),
        Err(err) => {
            let code = err.exit_code();
            let _ = err.print();
            Err(code)
        }
    }
}

fn capture_stdio(run: impl FnOnce() -> i32) -> anyhow::Result<TestServerResponse> {
    let mut stdout_capture = tempfile::tempfile().context("failed to create stdout capture")?;
    let mut stderr_capture = tempfile::tempfile().context("failed to create stderr capture")?;
    let redirect =
        StdioRedirect::new(&stdout_capture, &stderr_capture).context("failed to redirect stdio")?;

    let exit_code = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)) {
        Ok(code) => code,
        Err(_) => -1,
    };

    std::io::stdout()
        .flush()
        .context("failed to flush captured stdout")?;
    std::io::stderr()
        .flush()
        .context("failed to flush captured stderr")?;
    redirect
        .restore()
        .context("failed to restore process stdio")?;

    Ok(TestServerResponse {
        exit_code,
        stdout: read_capture(&mut stdout_capture)?,
        stderr: read_capture(&mut stderr_capture)?,
    })
}

fn read_capture(file: &mut File) -> anyhow::Result<String> {
    let mut bytes = Vec::new();
    file.seek(SeekFrom::Start(0))
        .context("failed to rewind captured output")?;
    file.read_to_end(&mut bytes)
        .context("failed to read captured output")?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

struct ScopedEnv {
    previous: Vec<(String, Option<OsString>)>,
}

impl ScopedEnv {
    fn apply(envs: &[(String, String)]) -> Self {
        let previous = envs
            .iter()
            .map(|(name, _)| (name.clone(), std::env::var_os(name)))
            .collect::<Vec<_>>();

        // This server handles one request at a time in a dedicated helper process,
        // so temporarily mutating the process environment is isolated to that process.
        for (name, value) in envs {
            unsafe { std::env::set_var(name, value) };
        }

        Self { previous }
    }
}

impl Drop for ScopedEnv {
    fn drop(&mut self) {
        for (name, value) in self.previous.drain(..).rev() {
            match value {
                Some(value) => unsafe { std::env::set_var(&name, value) },
                None => unsafe { std::env::remove_var(&name) },
            }
        }
    }
}

struct StdioRedirect {
    inner: platform::PlatformStdioRedirect,
}

impl StdioRedirect {
    fn new(stdout_capture: &File, stderr_capture: &File) -> anyhow::Result<Self> {
        Ok(Self {
            inner: platform::PlatformStdioRedirect::new(stdout_capture, stderr_capture)?,
        })
    }

    fn restore(self) -> anyhow::Result<()> {
        self.inner.restore()
    }
}

#[cfg(unix)]
mod platform {
    use std::{
        fs::File,
        io,
        os::fd::{AsRawFd, RawFd},
    };

    use anyhow::Context;

    pub(super) struct PlatformStdioRedirect {
        saved_stdout: RawFd,
        saved_stderr: RawFd,
    }

    impl PlatformStdioRedirect {
        pub(super) fn new(stdout_capture: &File, stderr_capture: &File) -> anyhow::Result<Self> {
            let saved_stdout = dup(libc::STDOUT_FILENO).context("failed to dup stdout")?;
            let saved_stderr = match dup(libc::STDERR_FILENO) {
                Ok(saved_stderr) => saved_stderr,
                Err(err) => {
                    close_fd(saved_stdout);
                    return Err(err).context("failed to dup stderr");
                }
            };

            if let Err(err) = dup2(stdout_capture.as_raw_fd(), libc::STDOUT_FILENO) {
                close_fd(saved_stdout);
                close_fd(saved_stderr);
                return Err(err).context("failed to redirect stdout");
            }

            if let Err(err) = dup2(stderr_capture.as_raw_fd(), libc::STDERR_FILENO) {
                let _ = dup2(saved_stdout, libc::STDOUT_FILENO);
                close_fd(saved_stdout);
                close_fd(saved_stderr);
                return Err(err).context("failed to redirect stderr");
            }

            Ok(Self {
                saved_stdout,
                saved_stderr,
            })
        }

        pub(super) fn restore(mut self) -> anyhow::Result<()> {
            let stdout_restore =
                dup2(self.saved_stdout, libc::STDOUT_FILENO).context("failed to restore stdout");
            let stderr_restore =
                dup2(self.saved_stderr, libc::STDERR_FILENO).context("failed to restore stderr");
            close_fd(self.saved_stdout);
            close_fd(self.saved_stderr);
            self.saved_stdout = -1;
            self.saved_stderr = -1;
            stdout_restore?;
            stderr_restore?;
            Ok(())
        }
    }

    impl Drop for PlatformStdioRedirect {
        fn drop(&mut self) {
            if self.saved_stdout >= 0 {
                let _ = dup2(self.saved_stdout, libc::STDOUT_FILENO);
                close_fd(self.saved_stdout);
                self.saved_stdout = -1;
            }

            if self.saved_stderr >= 0 {
                let _ = dup2(self.saved_stderr, libc::STDERR_FILENO);
                close_fd(self.saved_stderr);
                self.saved_stderr = -1;
            }
        }
    }

    fn dup(fd: RawFd) -> io::Result<RawFd> {
        let duplicated = unsafe { libc::dup(fd) };
        if duplicated == -1 {
            Err(io::Error::last_os_error())
        } else {
            Ok(duplicated)
        }
    }

    fn dup2(from: RawFd, to: RawFd) -> io::Result<()> {
        if unsafe { libc::dup2(from, to) } == -1 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn close_fd(fd: RawFd) {
        let _ = unsafe { libc::close(fd) };
    }
}

#[cfg(windows)]
mod platform {
    use std::{fs::File, io, os::windows::io::AsRawHandle};

    use anyhow::{Context, anyhow};
    use windows_sys::Win32::{
        Foundation::{CloseHandle, DUPLICATE_SAME_ACCESS, HANDLE},
        System::{
            Console::{GetStdHandle, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE, SetStdHandle},
            Threading::{DuplicateHandle, GetCurrentProcess},
        },
    };

    pub(super) struct PlatformStdioRedirect {
        saved_stdout_fd: i32,
        saved_stderr_fd: i32,
        capture_stdout_fd: i32,
        capture_stderr_fd: i32,
        saved_stdout_handle: HANDLE,
        saved_stderr_handle: HANDLE,
    }

    impl PlatformStdioRedirect {
        pub(super) fn new(stdout_capture: &File, stderr_capture: &File) -> anyhow::Result<Self> {
            let saved_stdout_fd = dup_fd(libc::STDOUT_FILENO).context("failed to dup stdout")?;
            let saved_stderr_fd = match dup_fd(libc::STDERR_FILENO) {
                Ok(saved_stderr_fd) => saved_stderr_fd,
                Err(err) => {
                    close_fd(saved_stdout_fd);
                    return Err(err).context("failed to dup stderr");
                }
            };

            let capture_stdout_fd = match open_fd(stdout_capture) {
                Ok(fd) => fd,
                Err(err) => {
                    close_fd(saved_stdout_fd);
                    close_fd(saved_stderr_fd);
                    return Err(err).context("failed to open stdout capture");
                }
            };
            let capture_stderr_fd = match open_fd(stderr_capture) {
                Ok(fd) => fd,
                Err(err) => {
                    close_fd(saved_stdout_fd);
                    close_fd(saved_stderr_fd);
                    close_fd(capture_stdout_fd);
                    return Err(err).context("failed to open stderr capture");
                }
            };

            let saved_stdout_handle = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
            let saved_stderr_handle = unsafe { GetStdHandle(STD_ERROR_HANDLE) };

            if let Err(err) = set_capture_stdio(
                stdout_capture,
                stderr_capture,
                saved_stdout_fd,
                saved_stderr_fd,
                saved_stdout_handle,
                capture_stdout_fd,
                capture_stderr_fd,
            ) {
                close_fd(saved_stdout_fd);
                close_fd(saved_stderr_fd);
                close_fd(capture_stdout_fd);
                close_fd(capture_stderr_fd);
                return Err(err);
            }

            Ok(Self {
                saved_stdout_fd,
                saved_stderr_fd,
                capture_stdout_fd,
                capture_stderr_fd,
                saved_stdout_handle,
                saved_stderr_handle,
            })
        }

        pub(super) fn restore(mut self) -> anyhow::Result<()> {
            let stdout_restore = dup2_fd(self.saved_stdout_fd, libc::STDOUT_FILENO)
                .context("failed to restore stdout fd");
            let stderr_restore = dup2_fd(self.saved_stderr_fd, libc::STDERR_FILENO)
                .context("failed to restore stderr fd");
            let stdout_handle_restore = set_std_handle(STD_OUTPUT_HANDLE, self.saved_stdout_handle)
                .context("failed to restore stdout handle");
            let stderr_handle_restore = set_std_handle(STD_ERROR_HANDLE, self.saved_stderr_handle)
                .context("failed to restore stderr handle");

            close_fd(self.capture_stdout_fd);
            close_fd(self.capture_stderr_fd);
            close_fd(self.saved_stdout_fd);
            close_fd(self.saved_stderr_fd);

            self.capture_stdout_fd = -1;
            self.capture_stderr_fd = -1;
            self.saved_stdout_fd = -1;
            self.saved_stderr_fd = -1;

            stdout_restore?;
            stderr_restore?;
            stdout_handle_restore?;
            stderr_handle_restore?;
            Ok(())
        }
    }

    impl Drop for PlatformStdioRedirect {
        fn drop(&mut self) {
            if self.saved_stdout_fd >= 0 {
                let _ = dup2_fd(self.saved_stdout_fd, libc::STDOUT_FILENO);
                close_fd(self.saved_stdout_fd);
                self.saved_stdout_fd = -1;
            }

            if self.saved_stderr_fd >= 0 {
                let _ = dup2_fd(self.saved_stderr_fd, libc::STDERR_FILENO);
                close_fd(self.saved_stderr_fd);
                self.saved_stderr_fd = -1;
            }

            if self.capture_stdout_fd >= 0 {
                close_fd(self.capture_stdout_fd);
                self.capture_stdout_fd = -1;
            }

            if self.capture_stderr_fd >= 0 {
                close_fd(self.capture_stderr_fd);
                self.capture_stderr_fd = -1;
            }

            let _ = set_std_handle(STD_OUTPUT_HANDLE, self.saved_stdout_handle);
            let _ = set_std_handle(STD_ERROR_HANDLE, self.saved_stderr_handle);
        }
    }

    fn set_capture_stdio(
        stdout_capture: &File,
        stderr_capture: &File,
        saved_stdout_fd: i32,
        saved_stderr_fd: i32,
        saved_stdout_handle: HANDLE,
        capture_stdout_fd: i32,
        capture_stderr_fd: i32,
    ) -> anyhow::Result<()> {
        dup2_fd(capture_stdout_fd, libc::STDOUT_FILENO).context("failed to redirect stdout fd")?;
        if let Err(err) = dup2_fd(capture_stderr_fd, libc::STDERR_FILENO) {
            let _ = dup2_fd(saved_stdout_fd, libc::STDOUT_FILENO);
            return Err(err).context("failed to redirect stderr fd");
        }

        if let Err(err) =
            set_std_handle(STD_OUTPUT_HANDLE, stdout_capture.as_raw_handle() as HANDLE)
        {
            let _ = dup2_fd(saved_stdout_fd, libc::STDOUT_FILENO);
            let _ = dup2_fd(saved_stderr_fd, libc::STDERR_FILENO);
            return Err(err).context("failed to redirect stdout handle");
        }

        if let Err(err) = set_std_handle(STD_ERROR_HANDLE, stderr_capture.as_raw_handle() as HANDLE)
        {
            let _ = set_std_handle(STD_OUTPUT_HANDLE, saved_stdout_handle);
            let _ = dup2_fd(saved_stdout_fd, libc::STDOUT_FILENO);
            let _ = dup2_fd(saved_stderr_fd, libc::STDERR_FILENO);
            return Err(err).context("failed to redirect stderr handle");
        }
        Ok(())
    }

    fn open_fd(file: &File) -> anyhow::Result<i32> {
        let process = unsafe { GetCurrentProcess() };
        let mut duplicated = 0;
        let ok = unsafe {
            DuplicateHandle(
                process,
                file.as_raw_handle() as HANDLE,
                process,
                &mut duplicated,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error()).context("failed to duplicate capture handle");
        }

        let fd = unsafe { libc::_open_osfhandle(duplicated as isize, 0) };
        if fd == -1 {
            let _ = unsafe { CloseHandle(duplicated) };
            return Err(anyhow!("failed to convert capture handle to fd"));
        }

        Ok(fd)
    }

    fn dup_fd(fd: i32) -> io::Result<i32> {
        let duplicated = unsafe { libc::_dup(fd) };
        if duplicated == -1 {
            Err(io::Error::last_os_error())
        } else {
            Ok(duplicated)
        }
    }

    fn dup2_fd(from: i32, to: i32) -> io::Result<()> {
        if unsafe { libc::_dup2(from, to) } == -1 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn close_fd(fd: i32) {
        let _ = unsafe { libc::_close(fd) };
    }

    fn set_std_handle(which: u32, handle: HANDLE) -> io::Result<()> {
        if unsafe { SetStdHandle(which, handle) } == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}
