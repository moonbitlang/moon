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

use std::sync::OnceLock;
use std::time::Instant;

static MONOTONIC_CLOCK_ORIGIN: OnceLock<Instant> = OnceLock::new();

pub(super) fn get_ms_since_epoch(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let origin = MONOTONIC_CLOCK_ORIGIN.get_or_init(Instant::now);
    let millis = i64::try_from(origin.elapsed().as_millis()).unwrap_or(i64::MAX);
    ret.set(v8::BigInt::new_from_i64(scope, millis).into());
}
