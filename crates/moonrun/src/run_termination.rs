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

use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RunTermination {
    Exit(i32),
    KilledBySignal(i32),
}

// Guest imports request termination on the isolate thread, while an embedding
// controller may request it from another thread before interrupting the engine.
// Keeping the request independent of either adapter lets another Wasm engine
// surface the same per-run outcome without inheriting V8-specific state.
#[derive(Clone, Default)]
pub(crate) struct TerminationRequest(Arc<Mutex<Option<RunTermination>>>);

impl TerminationRequest {
    pub(crate) fn request(&self, termination: RunTermination) {
        let mut requested = self.0.lock().unwrap();
        if requested.is_none() {
            *requested = Some(termination);
        }
    }

    pub(crate) fn take(&self) -> Option<RunTermination> {
        self.0.lock().unwrap().take()
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
