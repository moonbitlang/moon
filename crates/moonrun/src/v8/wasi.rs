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

//! V8 setup for the shared WASIp1 registrations and per-run Host state.

use super::context::V8RunContext;
use crate::wasi::{self, WasiContext};
use std::{any::Any, rc::Rc};

pub(super) fn init_env<'s>(
    obj: v8::Local<'s, v8::Object>,
    scope: &mut v8::HandleScope<'s>,
    wasm_file_name: &str,
    args: &[String],
    run_context: &V8RunContext,
    dtors: &mut Vec<Box<dyn Any>>,
) {
    wasi::imports::register(&mut super::wasm_imports::Registry {
        object: obj,
        scope,
        module: wasi::imports::MODULE,
        data: Rc::new(WasiContext::new(
            wasm_file_name,
            args,
            run_context.runtime(),
            run_context.termination_request().clone(),
        )),
        memory: Rc::clone(run_context.memory_binding()),
        retained: dtors,
    })
    .unwrap();
}
