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

use super::context::ImportContext;
use super::provenance::ported_imports;
use crate::run_termination::RunTermination;

pub(super) fn exit(context: &mut ImportContext<'_, '_>, code: i32) {
    context.request_termination(RunTermination::Exit(code))
}

ported_imports! {
#[ported(
    source = "src/internal/event_loop/signal.c",
    original = "moonbitlang_async_terminate_process_by_signal"
)]
pub(super) fn terminate_process_by_signal(
    context: &mut ImportContext<'_, '_>,
    signal: i32,
) {
    // The native implementation terminates its process here. Moonrun instead
    // records the equivalent outcome so the outer adapter can apply it after
    // the Run has torn down.
    context.request_termination(RunTermination::KilledBySignal(signal))
}
}
