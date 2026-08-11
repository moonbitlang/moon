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
    context.request_termination(RunTermination::KilledBySignal(signal))
}
}
