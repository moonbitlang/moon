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

use std::cell::Cell;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RunTermination {
    Exit(i32),
    KilledBySignal(i32),
}

// The current V8 adapter invokes imports synchronously on one isolate thread.
// Keeping the request independent of that adapter lets another Wasm engine
// surface the same per-run outcome without inheriting V8-specific state.
#[derive(Clone, Default)]
pub(crate) struct TerminationRequest(Rc<Cell<Option<RunTermination>>>);

impl TerminationRequest {
    pub(crate) fn request(&self, termination: RunTermination) {
        if self.0.get().is_none() {
            self.0.set(Some(termination));
        }
    }

    #[cfg(all(feature = "wasmtime", not(feature = "v8")))]
    pub(crate) fn is_requested(&self) -> bool {
        self.0.get().is_some()
    }

    pub(crate) fn take(&self) -> Option<RunTermination> {
        self.0.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_killed_by_signal() {
        let request = TerminationRequest::default();

        request.request(RunTermination::KilledBySignal(15));

        assert_eq!(request.take(), Some(RunTermination::KilledBySignal(15)));
        assert_eq!(request.take(), None);
    }

    #[test]
    fn preserves_the_first_outcome() {
        let request = TerminationRequest::default();

        request.request(RunTermination::KilledBySignal(15));
        request.request(RunTermination::Exit(1));

        assert_eq!(request.take(), Some(RunTermination::KilledBySignal(15)));
    }
}
