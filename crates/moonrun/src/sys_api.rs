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

use crate::v8_builder::{ArgsExt, ObjectExt, ScopeExt};
use std::io::IsTerminal;

fn construct_args_list<'s>(
    wasm_file_name: &str,
    args: &[String],
    scope: &mut v8::HandleScope<'s>,
) -> v8::Local<'s, v8::Array> {
    // argv: [program, ..args]
    let arr = v8::Array::new(scope, (args.len() + 1) as i32);

    let program = scope.string(wasm_file_name);
    arr.set_index(scope, 0, program.into());
    for (i, arg) in args.iter().enumerate() {
        let arg = scope.string(arg);
        arr.set_index(scope, (i + 1) as u32, arg.into());
    }
    arr
}

fn construct_env_vars<'s>(scope: &mut v8::HandleScope<'s>) -> v8::Local<'s, v8::Map> {
    let map = v8::Map::new(scope);
    for (k, v) in std::env::vars() {
        let key = scope.string(&k);
        let val = scope.string(&v);
        map.set(scope, key.into(), val.into());
    }
    map
}
fn set_env_var(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let key = args.string_lossy(scope, 0);
    let value = args.string_lossy(scope, 1);

    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var(&key, &value) };

    ret.set_undefined()
}

fn unset_env_var(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let key = args.string_lossy(scope, 0);
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var(&key) };
    ret.set_undefined()
}

fn get_env_var(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let key = args.string_lossy(scope, 0);
    let value = std::env::var(&key).unwrap_or_default();
    let value = scope.string(&value);
    ret.set(value.into());
}

fn get_env_var_exists(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let key = args.string_lossy(scope, 0);
    ret.set_bool(std::env::var(key).is_ok());
}

fn get_env_vars(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = v8::Array::new(scope, 0);
    let mut index = 0;
    for (k, v) in std::env::vars() {
        let key = scope.string(&k);
        let val = scope.string(&v);
        result.set_index(scope, index, key.into()).unwrap();
        result.set_index(scope, index + 1, val.into()).unwrap();
        index += 2;
    }
    ret.set(result.into());
}

pub fn init_env<'s>(
    obj: v8::Local<'s, v8::Object>,
    scope: &mut v8::HandleScope<'s>,
    wasm_file_name: &str,
    args: &[String],
) {
    let args_list = construct_args_list(wasm_file_name, args, scope);
    let env_vars = construct_env_vars(scope);
    let env_obj = v8::Object::new(scope);
    let env_vars_key = scope.string("env_vars").into();
    env_obj.set(scope, env_vars_key, env_vars.into());
    let args_key = scope.string("args").into();
    env_obj.set(scope, args_key, args_list.into());
    let stderr_is_tty_key = scope.string("stderr_is_tty").into();
    let stderr_is_tty = v8::Boolean::new(scope, std::io::stderr().is_terminal()).into();
    env_obj.set(scope, stderr_is_tty_key, stderr_is_tty);

    // Expose the run env for the unified JS glue in `template/js_glue.js`.
    let global_proxy = scope.get_current_context().global(scope);
    let run_env_key = scope.string("__moonbit_run_env");
    global_proxy.set(scope, run_env_key.into(), env_obj.into());

    obj.set_func(scope, "set_env_var", set_env_var);
    obj.set_func(scope, "unset_env_var", unset_env_var);
    obj.set_func(scope, "get_env_vars", get_env_vars);
    obj.set_func(scope, "get_env_var", get_env_var);
    obj.set_func(scope, "get_env_var_exists", get_env_var_exists);
}
