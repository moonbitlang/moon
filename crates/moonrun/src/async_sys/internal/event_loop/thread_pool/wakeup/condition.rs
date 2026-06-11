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

pub(crate) type WorkerThreadId = libc::pthread_t;

pub(crate) struct WorkerWakeup {
    mutex: libc::pthread_mutex_t,
    cond: libc::pthread_cond_t,
}

impl WorkerWakeup {
    pub(crate) fn new() -> Self {
        let mut mutex = std::mem::MaybeUninit::<libc::pthread_mutex_t>::uninit();
        let mut cond = std::mem::MaybeUninit::<libc::pthread_cond_t>::uninit();
        unsafe {
            libc::pthread_mutex_init(mutex.as_mut_ptr(), std::ptr::null());
            libc::pthread_cond_init(cond.as_mut_ptr(), std::ptr::null());
            Self {
                mutex: mutex.assume_init(),
                cond: cond.assume_init(),
            }
        }
    }

    pub(crate) fn wake(&mut self, _id: Option<WorkerThreadId>, waiting: &mut bool) {
        unsafe {
            libc::pthread_mutex_lock(&mut self.mutex);
            *waiting = false;
            libc::pthread_cond_signal(&mut self.cond);
            libc::pthread_mutex_unlock(&mut self.mutex);
        }
    }

    pub(crate) fn wait(&mut self, waiting: &mut bool) {
        unsafe {
            libc::pthread_mutex_lock(&mut self.mutex);
            if *waiting {
                loop {
                    // Keep parity with async's native macOS workaround: retry
                    // pthread_cond_wait when it spuriously reports EINVAL.
                    while libc::pthread_cond_wait(&mut self.cond, &mut self.mutex) == libc::EINVAL {
                    }
                    if !*waiting {
                        break;
                    }
                }
            }
            libc::pthread_mutex_unlock(&mut self.mutex);
        }
    }
}

impl Drop for WorkerWakeup {
    fn drop(&mut self) {
        unsafe {
            libc::pthread_mutex_destroy(&mut self.mutex);
            libc::pthread_cond_destroy(&mut self.cond);
        }
    }
}

pub(crate) fn cancel_running_worker(id: Option<WorkerThreadId>) -> i32 {
    let Some(id) = id else {
        return 0;
    };
    unsafe {
        libc::pthread_kill(id, libc::SIGUSR2);
    }
    0
}
