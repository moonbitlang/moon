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

//! The `exception` ABI. The per-run adapter retains the constructor and the
//! exact imported tag through strong V8 handles; V8 owns both objects.

use super::builder::ObjectExt;
use super::context::{callback_context, wasm_constructor};
use anyhow::Context;
use std::any::Any;
use v8::{FunctionCallbackArguments, HandleScope, Local};

struct ExceptionImports {
    constructor: v8::Global<v8::Function>,
    tag: v8::Global<v8::Object>,
}

fn throw<'s>(
    scope: &mut HandleScope<'s>,
    args: FunctionCallbackArguments<'s>,
    _ret: v8::ReturnValue,
) {
    // SAFETY: install supplies this exact type and retains the box until the
    // run ends, after guest code can no longer invoke the callback.
    let state = unsafe { callback_context::<ExceptionImports>(&args) };
    let constructor = Local::new(scope, &state.constructor);
    let tag = Local::new(scope, &state.tag);
    let values = v8::Array::new(scope, 0);
    let options = v8::Object::new(scope);
    let trace_stack = v8::Boolean::new(scope, true);
    options.set_value(scope, "traceStack", trace_stack.into());
    let Some(exception) =
        constructor.new_instance(scope, &[tag.into(), values.into(), options.into()])
    else {
        return;
    };
    scope.throw_exception(exception.into());
}

pub(super) fn install<'s>(
    scope: &mut HandleScope<'s>,
    imports: Local<'s, v8::Object>,
    retained: &mut Vec<Box<dyn Any>>,
) -> anyhow::Result<()> {
    let tag_constructor = wasm_constructor(scope, "Tag")?;
    let tag_type = v8::Object::new(scope);
    let parameters = v8::Array::new(scope, 0);
    tag_type.set_value(scope, "parameters", parameters.into());
    let tag = tag_constructor
        .new_instance(scope, &[tag_type.into()])
        .context("failed to create Moonrun's WebAssembly exception tag")?;
    let constructor = wasm_constructor(scope, "Exception")?;
    let state = Box::new(ExceptionImports {
        constructor: v8::Global::new(scope, constructor),
        tag: v8::Global::new(scope, tag),
    });
    let data = v8::External::new(scope, std::ptr::from_ref(&*state).cast_mut().cast());
    let throw = v8::Function::builder(throw)
        .data(data.into())
        .build(scope)
        .context("failed to create Moonrun's exception callback")?;
    retained.push(state);
    imports.set_value(scope, "tag", tag.into());
    imports.set_value(scope, "throw", throw.into());
    Ok(())
}
