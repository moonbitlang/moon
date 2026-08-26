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

//! Receiving and forwarding an inherited Moonrun Policy handle.
//!
//! [`PolicyTransfer`] owns the received OS resource and may either be consumed
//! by moonrun or converted into a [`PolicyRelay`]. A relay cannot read policy
//! contents and can be attached to at most one delegated moonrun command.

use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, Read};
use std::process::Command;

use anyhow::Context;

use crate::constants::MOONRUN_INHERITED_POLICY;

#[derive(Debug)]
pub struct PolicyTransfer {
    file: File,
}

impl PolicyTransfer {
    /// Wrap a newly created policy file and make it non-inheritable until a
    /// process adapter deliberately attaches it to one child.
    pub fn from_file(file: File) -> io::Result<Self> {
        #[cfg(unix)]
        let file = {
            use std::os::fd::{AsRawFd, FromRawFd};

            if file.as_raw_fd() <= libc::STDERR_FILENO {
                let fd = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 3) };
                if fd < 0 {
                    return Err(io::Error::last_os_error());
                }
                unsafe { File::from_raw_fd(fd) }
            } else {
                file
            }
        };
        #[cfg(unix)]
        set_cloexec(std::os::fd::AsRawFd::as_raw_fd(&file), true)?;
        #[cfg(windows)]
        set_inheritable(&file, false)?;

        Ok(Self { file })
    }

    /// Take ownership of the inherited policy handle and consume its reserved
    /// environment marker before ordinary environment materialization begins.
    pub fn take_from_env() -> anyhow::Result<Option<Self>> {
        let Some(token) = std::env::var_os(MOONRUN_INHERITED_POLICY) else {
            return Ok(None);
        };
        // Process entry runs before worker threads. Removing the marker here
        // prevents registry children and env.from_host="*" from seeing it.
        unsafe {
            std::env::remove_var(MOONRUN_INHERITED_POLICY);
        }
        Self::from_token(&token).map(Some)
    }

    /// Remove an inherited-policy marker from a process that cannot forward
    /// it. A valid handle is closed; a malformed marker is ignored because it
    /// cannot describe a policy resource owned by this process.
    pub fn discard_from_env() {
        let _ = Self::take_from_env();
    }

    pub fn read(mut self) -> anyhow::Result<Vec<u8>> {
        let mut contents = Vec::new();
        self.file
            .read_to_end(&mut contents)
            .context("failed to read inherited Moonrun Policy")?;
        Ok(contents)
    }

    /// Restrict this transfer to the intermediary role used by moonx.
    pub fn into_relay(self) -> PolicyRelay {
        PolicyRelay { transfer: self }
    }

    fn from_token(token: &OsStr) -> anyhow::Result<Self> {
        let token = token
            .to_str()
            .context("inherited Moonrun Policy handle is not a decimal OS handle")?;

        #[cfg(unix)]
        {
            use std::os::fd::FromRawFd;

            let fd = token
                .parse::<libc::c_int>()
                .context("invalid inherited Moonrun Policy file descriptor")?;
            anyhow::ensure!(
                fd > libc::STDERR_FILENO,
                "invalid inherited Moonrun Policy file descriptor"
            );
            // SAFETY: the producer transferred this descriptor to the process;
            // the environment marker is consumed exactly once at entry.
            let file = unsafe { File::from_raw_fd(fd) };
            Self::from_file(file)
                .context("failed to isolate inherited Moonrun Policy file descriptor")
        }

        #[cfg(windows)]
        {
            use std::os::windows::io::FromRawHandle;
            use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;

            let value = token
                .parse::<usize>()
                .context("invalid inherited Moonrun Policy handle")?;
            let handle = value as *mut std::ffi::c_void;
            anyhow::ensure!(
                !handle.is_null() && handle != INVALID_HANDLE_VALUE,
                "invalid inherited Moonrun Policy handle"
            );
            // SAFETY: the producer transferred this handle to the process; the
            // environment marker is consumed exactly once at entry.
            let file = unsafe { File::from_raw_handle(handle) };
            Self::from_file(file).context("failed to isolate inherited Moonrun Policy handle")
        }
    }

    #[cfg(unix)]
    fn token(&self) -> String {
        use std::os::fd::AsRawFd;

        self.file.as_raw_fd().to_string()
    }

    #[cfg(windows)]
    fn token(&self) -> String {
        use std::os::windows::io::AsRawHandle;

        (self.file.as_raw_handle() as usize).to_string()
    }

    #[cfg(unix)]
    fn attach_platform(&self, command: &mut Command) {
        use std::os::fd::AsRawFd;
        use std::os::unix::process::CommandExt;

        let fd = self.file.as_raw_fd();
        // SAFETY: the hook only changes one already-open descriptor using
        // async-signal-safe fcntl operations.
        unsafe {
            command.pre_exec(move || set_cloexec(fd, false));
        }
    }

    #[cfg(windows)]
    fn attach_platform(&self, _command: &mut Command) -> io::Result<()> {
        set_inheritable(&self.file, true)
    }

    #[cfg(unix)]
    fn stop_inheriting(&self) -> io::Result<()> {
        use std::os::fd::AsRawFd;

        set_cloexec(self.file.as_raw_fd(), true)
    }

    #[cfg(windows)]
    fn stop_inheriting(&self) -> io::Result<()> {
        set_inheritable(&self.file, false)
    }
}

/// An opaque, one-shot transfer held by an intermediary moonx process.
#[derive(Debug)]
pub struct PolicyRelay {
    transfer: PolicyTransfer,
}

impl PolicyRelay {
    /// Attach this handle to one child command. The returned guard keeps the
    /// inheritance window scoped to that command and restores close-on-exec
    /// state if process creation returns.
    pub fn attach_to(self, command: &mut Command) -> io::Result<PolicyRelayGuard> {
        command.env(MOONRUN_INHERITED_POLICY, self.transfer.token());
        #[cfg(unix)]
        self.transfer.attach_platform(command);
        #[cfg(windows)]
        self.transfer.attach_platform(command)?;
        Ok(PolicyRelayGuard {
            relay: self,
            active: true,
        })
    }
}

#[cfg(unix)]
impl std::os::fd::AsRawFd for PolicyTransfer {
    fn as_raw_fd(&self) -> std::os::fd::RawFd {
        std::os::fd::AsRawFd::as_raw_fd(&self.file)
    }
}

#[cfg(windows)]
impl std::os::windows::io::AsRawHandle for PolicyTransfer {
    fn as_raw_handle(&self) -> std::os::windows::io::RawHandle {
        std::os::windows::io::AsRawHandle::as_raw_handle(&self.file)
    }
}

#[must_use = "the relay guard must remain alive until process creation finishes"]
pub struct PolicyRelayGuard {
    relay: PolicyRelay,
    active: bool,
}

impl PolicyRelayGuard {
    pub fn finish(mut self) -> io::Result<()> {
        self.relay.transfer.stop_inheriting()?;
        self.active = false;
        Ok(())
    }
}

impl Drop for PolicyRelayGuard {
    fn drop(&mut self) {
        if self.active {
            let _ = self.relay.transfer.stop_inheriting();
        }
    }
}

#[cfg(unix)]
fn set_cloexec(fd: libc::c_int, enabled: bool) -> io::Result<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    if flags < 0 {
        return Err(io::Error::last_os_error());
    }
    let flags = if enabled {
        flags | libc::FD_CLOEXEC
    } else {
        flags & !libc::FD_CLOEXEC
    };
    if unsafe { libc::fcntl(fd, libc::F_SETFD, flags) } < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn set_inheritable(file: &File, inheritable: bool) -> io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::{HANDLE_FLAG_INHERIT, SetHandleInformation};

    let flags = if inheritable { HANDLE_FLAG_INHERIT } else { 0 };
    if unsafe { SetHandleInformation(file.as_raw_handle(), HANDLE_FLAG_INHERIT, flags) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::io::{Seek, Write};

    use super::*;

    #[cfg(unix)]
    fn policy(contents: &[u8]) -> PolicyTransfer {
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(contents).unwrap();
        file.rewind().unwrap();
        PolicyTransfer::from_file(file).unwrap()
    }

    #[cfg(unix)]
    #[test]
    fn relay_attaches_the_policy_fd_to_one_command() {
        use std::os::fd::AsRawFd;
        use std::os::unix::fs::MetadataExt;

        let transfer = policy(b"policy");
        let fd = transfer.as_raw_fd();
        // Keep the anonymous file alive after the relay closes its descriptor.
        // This makes its identity stable even if another parallel test reuses
        // the descriptor number immediately.
        let identity_guard = transfer.file.try_clone().unwrap();
        let identity = identity_guard.metadata().unwrap();
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        assert_ne!(flags, -1);
        assert_ne!(flags & libc::FD_CLOEXEC, 0);

        let mut command = Command::new("/bin/sh");
        command.args(["-c", "cat <&\"$MOONRUN_INHERITED_POLICY\""]);
        let relay = transfer.into_relay().attach_to(&mut command).unwrap();
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        assert_ne!(flags, -1);
        assert_ne!(flags & libc::FD_CLOEXEC, 0);
        let output = command.output().unwrap();
        relay.finish().unwrap();

        assert!(output.status.success());
        assert_eq!(output.stdout, b"policy");
        let mut current = unsafe { std::mem::zeroed::<libc::stat>() };
        if unsafe { libc::fstat(fd, &mut current) } == 0 {
            assert_ne!(
                (current.st_dev as u64, current.st_ino as u64),
                (identity.dev(), identity.ino()),
                "relay retained its policy file descriptor",
            );
        } else {
            assert_eq!(io::Error::last_os_error().raw_os_error(), Some(libc::EBADF));
        }
        drop(identity_guard);
    }

    #[cfg(windows)]
    #[test]
    fn relay_scopes_and_consumes_the_windows_handle() {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Foundation::{
            GetHandleInformation, HANDLE_FLAG_INHERIT, SetHandleInformation,
        };

        let file = tempfile::tempfile().unwrap();
        assert_ne!(
            unsafe {
                SetHandleInformation(
                    file.as_raw_handle(),
                    HANDLE_FLAG_INHERIT,
                    HANDLE_FLAG_INHERIT,
                )
            },
            0
        );
        let transfer = PolicyTransfer::from_file(file).unwrap();
        let handle = transfer.as_raw_handle();
        let mut flags = 0;
        assert_ne!(unsafe { GetHandleInformation(handle, &mut flags) }, 0);
        assert_eq!(flags & HANDLE_FLAG_INHERIT, 0);

        let mut command = Command::new("cmd.exe");
        let relay = transfer.into_relay().attach_to(&mut command).unwrap();
        assert_ne!(unsafe { GetHandleInformation(handle, &mut flags) }, 0);
        assert_ne!(flags & HANDLE_FLAG_INHERIT, 0);

        relay.finish().unwrap();
    }

    #[test]
    fn stdio_tokens_are_rejected() {
        assert!(PolicyTransfer::from_token(OsStr::new("2")).is_err());
    }
}
