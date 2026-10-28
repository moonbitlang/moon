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

use super::{AsyncHost, AsyncHostResult, HostHandle};
use crate::async_sys::tty;
use crate::resource::ResourceClass;

impl AsyncHost {
    /// Open the controlling terminal as
    /// `[input, wakeup, control, output]`. On Unix the control Handle is the
    /// input Handle, and the guest closes only the input Handle.
    pub(crate) fn tty_open(&self) -> AsyncHostResult<[HostHandle; 4]> {
        if !self.tty_allowed {
            crate::policy::sandbox_denied("terminal access", None)?;
        }
        let tty = tty::open()?;
        let mut handles = self.handles.borrow_mut();
        let input = handles.insert_resource(tty.input);
        let wakeup = handles.insert_resource(tty.wakeup);
        #[cfg(windows)]
        let control = handles.insert_resource(tty.control);
        #[cfg(unix)]
        let control = input;
        let output = handles.insert_resource(tty.output);
        Ok([input, wakeup, control, output])
    }

    /// Put the terminal in raw mode and return the old state, which the guest
    /// keeps and passes back to `tty_leave_raw`.
    pub(crate) fn tty_enter_raw(
        &self,
        input: HostHandle,
        output: HostHandle,
    ) -> AsyncHostResult<Vec<u8>> {
        let input = self.acquire_resource_of_class(input, ResourceClass::File)?;
        #[cfg(windows)]
        let output = self.acquire_resource_of_class(output, ResourceClass::File)?;
        #[cfg(unix)]
        let _ = output;
        tty::enter_raw(
            &input,
            #[cfg(windows)]
            &output,
        )
    }

    /// Restore a state returned by `tty_enter_raw`.
    pub(crate) fn tty_leave_raw(
        &self,
        input: HostHandle,
        output: HostHandle,
        state: &[u8],
    ) -> AsyncHostResult<()> {
        let input = self.acquire_resource_of_class(input, ResourceClass::File)?;
        #[cfg(windows)]
        let output = self.acquire_resource_of_class(output, ResourceClass::File)?;
        #[cfg(unix)]
        let _ = output;
        tty::leave_raw(
            &input,
            #[cfg(windows)]
            &output,
            state,
        )
    }

    pub(crate) fn tty_size(&self, output: HostHandle) -> AsyncHostResult<i64> {
        let output = self.acquire_resource_of_class(output, ResourceClass::File)?;
        tty::size(&output)
    }
}
