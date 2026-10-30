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

use crate::async_host::{AsyncHost, AsyncHostResult};
use crate::filesystem::HostFs;
use crate::run_termination::{RunTermination, TerminationRequest};

/// One synchronous import's host dependencies and Guest Memory borrow.
/// Engine values are decoded before construction; this context cannot re-enter
/// the guest or retain memory beyond the call.
pub(super) struct ImportContext<'a> {
    pub(super) host: &'a AsyncHost,
    pub(super) filesystem: &'a HostFs,
    // Defer access failures so imports that do not use memory still work before
    // memory is bound, including imports called during instantiation.
    pub(super) memory: AsyncHostResult<&'a mut [u8]>,
    pub(super) termination_request: &'a TerminationRequest,
}

impl ImportContext<'_> {
    pub(super) fn request_termination(&mut self, termination: RunTermination) {
        self.termination_request.request(termination);
    }

    pub(super) fn filesystem(&self) -> &HostFs {
        self.filesystem
    }

    pub(super) fn with_host_and_memory_mut<T>(
        &mut self,
        f: impl FnOnce(&AsyncHost, &mut [u8]) -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        let memory = self.memory.as_deref_mut().map_err(|error| *error)?;
        f(self.host, memory)
    }

    pub(super) fn with_memory_mut<T>(
        &mut self,
        f: impl FnOnce(&mut [u8]) -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        self.with_host_and_memory_mut(|_, memory| f(memory))
    }
}
