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

pub(crate) type WorkerThreadId = windows_sys::Win32::Foundation::HANDLE;

pub(crate) struct WorkerWakeup {
    event: windows_sys::Win32::Foundation::HANDLE,
}

impl WorkerWakeup {
    pub(crate) fn new() -> Self {
        use windows_sys::Win32::System::Threading::CreateEventA;

        let event = unsafe { CreateEventA(std::ptr::null(), 0, 0, std::ptr::null()) };
        Self { event }
    }

    pub(crate) fn wake(&self, _id: Option<WorkerThreadId>, waiting: &mut bool) {
        use windows_sys::Win32::System::Threading::SetEvent;

        *waiting = false;
        unsafe {
            SetEvent(self.event);
        }
    }

    pub(crate) fn wait(&self, _waiting: &mut bool) {
        use windows_sys::Win32::System::Threading::{INFINITE, WaitForSingleObject};

        unsafe {
            WaitForSingleObject(self.event, INFINITE);
        }
    }
}

impl Drop for WorkerWakeup {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.event);
        }
    }
}

pub(crate) fn cancel_running_worker(id: Option<WorkerThreadId>) -> i32 {
    use windows_sys::Win32::Foundation::{ERROR_NOT_FOUND, GetLastError};
    use windows_sys::Win32::System::IO::CancelSynchronousIo;

    let Some(id) = id else {
        return 0;
    };
    if unsafe { CancelSynchronousIo(id) } != 0 {
        1
    } else if unsafe { GetLastError() } == ERROR_NOT_FOUND {
        0
    } else {
        -1
    }
}
