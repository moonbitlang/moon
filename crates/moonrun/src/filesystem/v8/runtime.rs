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

//! V8 adapter for runtime values exposed through the unstable filesystem object.

use crate::runtime::Env;
use crate::util::get_ref;
use crate::v8::builder::{ArgsExt, ObjectExt, ScopeExt};
use std::any::Any;
use std::sync::Arc;

struct RuntimeArgs(Vec<String>);

fn args_get(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let args = unsafe { get_ref::<RuntimeArgs>(&args) };
    let result = v8::Array::new(scope, args.0.len() as i32);

    for (index, arg) in args.0.iter().enumerate() {
        let arg = scope.string(arg);
        let _ = result.set_index(scope, index as u32, arg.into());
    }
    ret.set(result.into());
}

fn set_env_var(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let environment = unsafe { get_ref::<Env>(&args) };
    let key = args.string_lossy(scope, 0);
    let value = args.string_lossy(scope, 1);

    // The legacy guest ABI has no error channel for environment mutations.
    let _ = environment.set(key.into(), value.into());

    ret.set_undefined()
}

fn unset_env_var(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let environment = unsafe { get_ref::<Env>(&args) };
    let key = args.string_lossy(scope, 0);
    // The legacy guest ABI has no error channel for environment mutations.
    let _ = environment.unset(key.as_ref());
    ret.set_undefined()
}

fn get_env_var(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let environment = unsafe { get_ref::<Env>(&args) };
    let key = args.string_lossy(scope, 0);
    let value = environment
        .get(key.as_ref())
        .and_then(|value| value.into_string().ok())
        .unwrap_or_default();
    let value = scope.string(&value);
    ret.set(value.into());
}

fn get_env_var_exists(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let environment = unsafe { get_ref::<Env>(&args) };
    let key = args.string_lossy(scope, 0);
    ret.set_bool(environment.get(key.as_ref()).is_some());
}

fn get_env_vars(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let environment = unsafe { get_ref::<Env>(&args) };
    let result = v8::Array::new(scope, 0);
    let mut index = 0;
    for (name, value) in environment.entries() {
        let (Some(name), Some(value)) = (name.to_str(), value.to_str()) else {
            continue;
        };
        let key = scope.string(name);
        let val = scope.string(value);
        result.set_index(scope, index, key.into()).unwrap();
        result.set_index(scope, index + 1, val.into()).unwrap();
        index += 2;
    }
    ret.set(result.into());
}

pub(super) fn register<'s>(
    obj: v8::Local<'s, v8::Object>,
    scope: &mut v8::HandleScope<'s>,
    wasm_file_name: &str,
    args: &[String],
    environment: Arc<Env>,
    dtors: &mut Vec<Box<dyn Any>>,
) {
    let environment_ptr = Arc::as_ptr(&environment);
    dtors.push(Box::new(environment));

    set_env_func(obj, scope, "env_get_var", get_env_var, environment_ptr);
    set_env_func(obj, scope, "set_env_var", set_env_var, environment_ptr);
    set_env_func(obj, scope, "unset_env_var", unset_env_var, environment_ptr);
    set_env_func(obj, scope, "get_env_vars", get_env_vars, environment_ptr);
    set_env_func(obj, scope, "get_env_var", get_env_var, environment_ptr);
    set_env_func(
        obj,
        scope,
        "get_env_var_exists",
        get_env_var_exists,
        environment_ptr,
    );

    let args = Box::new(RuntimeArgs(
        std::iter::once(wasm_file_name.to_owned())
            .chain(args.iter().cloned())
            .collect(),
    ));
    let args_ptr = &*args as *const RuntimeArgs;
    let data = v8::External::new(scope, args_ptr as *mut std::ffi::c_void);
    let function = v8::Function::builder(args_get)
        .data(data.into())
        .build(scope)
        .unwrap();
    obj.set_value(scope, "args_get", function.into());
    dtors.push(args);
}

fn set_env_func<'s>(
    obj: v8::Local<'s, v8::Object>,
    scope: &mut v8::HandleScope<'s>,
    name: &str,
    callback: impl v8::MapFnTo<v8::FunctionCallback>,
    environment: *const Env,
) {
    let data = v8::External::new(scope, environment as *mut std::ffi::c_void);
    let function = v8::Function::builder(callback)
        .data(data.into())
        .build(scope)
        .unwrap();
    obj.set_value(scope, name, function.into());
}
