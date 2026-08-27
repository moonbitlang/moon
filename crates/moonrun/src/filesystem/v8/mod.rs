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

//! V8 adapter for MoonBit's unstable filesystem import object.

mod runtime;
mod whole_file;

use std::any::Any;
use std::sync::Arc;

use crate::filesystem::HostFs;
use crate::runtime::Env;

pub(crate) fn init_env<'s>(
    obj: v8::Local<'s, v8::Object>,
    scope: &mut v8::HandleScope<'s>,
    wasm_file_name: &str,
    args: &[String],
    environment: Arc<Env>,
    filesystem: Arc<HostFs>,
    dtors: &mut Vec<Box<dyn Any>>,
) {
    runtime::register(obj, scope, wasm_file_name, args, environment, dtors);
    whole_file::register(obj, scope, filesystem, dtors);
}
