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

//! Sleep job executor ported from
//! `moonbitlang/async/src/internal/event_loop/thread_pool.c`.

use crate::async_sys::ported_fns;

ported_fns! {
    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "sleep_job_worker"
    )]
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
