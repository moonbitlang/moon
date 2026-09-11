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
