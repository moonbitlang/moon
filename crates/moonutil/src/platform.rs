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

//! Platform-specific utilities and helpers.

/// Try to work around MacOS hanging on waiting for child. There might be a race
/// condition between the child process and the parent waiting for it.
///
/// Related tokio issue:
/// - https://github.com/tokio-rs/tokio/issues/6770
/// - https://github.com/tokio-rs/tokio/pull/6953
#[cfg(unix)]
pub fn unix_with_sigchild_blocked<T>(f: impl FnOnce() -> T) -> T {
    // block SIGCHLD to avoid race condition with spawn and signal handler registration
    unsafe {
        let mut mask: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut mask);
        libc::sigaddset(&mut mask, libc::SIGCHLD);
        libc::sigprocmask(libc::SIG_BLOCK, &mask, std::ptr::null_mut());
    }
    let res = f();
    unsafe {
        let mut mask: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut mask);
        libc::sigaddset(&mut mask, libc::SIGCHLD);
        libc::sigprocmask(libc::SIG_UNBLOCK, &mask, std::ptr::null_mut());
    }
    res
}

#[cfg(not(unix))]
pub fn unix_with_sigchild_blocked<T>(f: impl FnOnce() -> T) -> T {
    f()
}
