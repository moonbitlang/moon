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

pub(crate) struct WorkerWakeup;

impl WorkerWakeup {
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) fn wake(&self, id: Option<WorkerThreadId>, waiting: &mut bool) {
        *waiting = false;
        if let Some(id) = id {
            unsafe {
                libc::pthread_kill(id, libc::SIGUSR1);
            }
        }
    }

    pub(crate) fn wait(&self, _waiting: &mut bool) {
        let mut sig = 0;
        let mut wakeup_signal = std::mem::MaybeUninit::<libc::sigset_t>::uninit();
        unsafe {
            libc::sigemptyset(wakeup_signal.as_mut_ptr());
            let mut wakeup_signal = wakeup_signal.assume_init();
            libc::sigaddset(&mut wakeup_signal, libc::SIGUSR1);
            libc::sigwait(&wakeup_signal, &mut sig);
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
