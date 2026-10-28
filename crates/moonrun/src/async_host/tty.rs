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
        self.authorize_tty()?;
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

    /// Fail unless the policy allows terminal access. Checked on every
    /// operation that changes the terminal mode, since a guest may also reach
    /// the terminal through a filesystem Handle.
    fn authorize_tty(&self) -> AsyncHostResult<()> {
        if !self.tty_allowed {
            crate::policy::sandbox_denied("terminal access", None)?;
        }
        Ok(())
    }

    /// Return the current terminal state, see `tty::state_size`.
    pub(crate) fn tty_get_state(
        &self,
        input: HostHandle,
        output: HostHandle,
    ) -> AsyncHostResult<Vec<u8>> {
        self.authorize_tty()?;
        let input = self.acquire_resource_of_class(input, ResourceClass::File)?;
        #[cfg(windows)]
        let output = self.acquire_resource_of_class(output, ResourceClass::File)?;
        #[cfg(unix)]
        let _ = output;
        tty::get_state(
            &input,
            #[cfg(windows)]
            &output,
        )
    }

    /// Apply a terminal state, see `tty::state_size`.
    pub(crate) fn tty_set_state(
        &self,
        input: HostHandle,
        output: HostHandle,
        state: &[u8],
    ) -> AsyncHostResult<()> {
        self.authorize_tty()?;
        let input = self.acquire_resource_of_class(input, ResourceClass::File)?;
        #[cfg(windows)]
        let output = self.acquire_resource_of_class(output, ResourceClass::File)?;
        #[cfg(unix)]
        let _ = output;
        tty::set_state(
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
