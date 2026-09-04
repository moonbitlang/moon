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
