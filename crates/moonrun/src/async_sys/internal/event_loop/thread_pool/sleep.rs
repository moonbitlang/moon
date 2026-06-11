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

#[allow(dead_code)]
pub(super) fn run_sleep_job(duration_ms: i32) {
    #[cfg(windows)]
    {
        // Match the native stub's `Sleep(((struct sleep_job*)job)->duration)`.
        unsafe { windows_sys::Win32::System::Threading::Sleep(duration_ms as u32) };
    }
    #[cfg(all(unix, target_os = "macos"))]
    {
        run_sleep_job_with_kqueue(duration_ms);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        run_sleep_job_with_nanosleep(duration_ms);
    }
}

#[cfg(all(unix, target_os = "macos"))]
fn run_sleep_job_with_kqueue(duration_ms: i32) {
    let kqfd = unsafe { libc::kqueue() };
    let duration = sleep_job_timespec(duration_ms);
    let mut event = std::mem::MaybeUninit::<libc::kevent>::uninit();

    // Native async intentionally uses kqueue as a timeout-only sleeper on
    // macOS because nanosleep was too imprecise on CI runners.
    unsafe {
        libc::kevent(kqfd, std::ptr::null(), 0, event.as_mut_ptr(), 1, &duration);
        libc::close(kqfd);
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn run_sleep_job_with_nanosleep(duration_ms: i32) {
    let duration = sleep_job_timespec(duration_ms);
    unsafe {
        libc::nanosleep(&duration, std::ptr::null_mut());
    }
}

#[cfg(unix)]
fn sleep_job_timespec(duration_ms: i32) -> libc::timespec {
    libc::timespec {
        tv_sec: (duration_ms / 1000) as libc::time_t,
        tv_nsec: ((duration_ms % 1000) * 1_000_000) as libc::c_long,
    }
}
