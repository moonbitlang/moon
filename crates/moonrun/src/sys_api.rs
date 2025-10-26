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

const INIT_SYS_API: &str = r#"
    (() => function(obj, run_env) {
        // Return the value of the environment variable
        function env_get_var(name) {
            return run_env.env_vars.get(name) || ""
        }

        // Get the list of arguments passed to the program
        function args_get() {
            return run_env.args
        }

        obj.env_get_var = env_get_var
        obj.args_get = args_get
    })()
"#;

fn construct_args_list<'s>(
    wasm_file_name: &str,
    args: &[String],
    scope: &mut v8::HandleScope<'s>,
) -> v8::Local<'s, v8::Array> {
    // argv: [program, ..args]
    let arr = v8::Array::new(scope, (args.len() + 1) as i32);

    let program = v8::String::new(scope, wasm_file_name).unwrap();
    arr.set_index(scope, 0, program.into());
    for (i, arg) in args.iter().enumerate() {
        let arg = v8::String::new(scope, arg).unwrap();
        arr.set_index(scope, (i + 1) as u32, arg.into());
    }
    arr
}

fn construct_env_vars<'s>(scope: &mut v8::HandleScope<'s>) -> v8::Local<'s, v8::Map> {
    let map = v8::Map::new(scope);
    for (k, v) in std::env::vars() {
        let key = v8::String::new(scope, &k).unwrap();
        let val = v8::String::new(scope, &v).unwrap();
        map.set(scope, key.into(), val.into());
    }
    map
}

fn set_env_var(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let key = args.get(0);
    let key = key.to_string(scope).unwrap();
    let key = key.to_rust_string_lossy(scope);

    let value = args.get(1);
    let value = value.to_string(scope).unwrap();
    let value = value.to_rust_string_lossy(scope);

    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var(&key, &value) };

    ret.set_undefined()
}

fn unset_env_var(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let key = args.get(0);
    let key = key.to_string(scope).unwrap();
    let key = key.to_rust_string_lossy(scope);
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var(&key) };
    ret.set_undefined()
}

fn get_env_var(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let key = args.get(0);
    let key = key.to_string(scope).unwrap();
    let key = key.to_rust_string_lossy(scope);
    let value = std::env::var(&key).unwrap_or_default();
    let value = v8::String::new(scope, &value).unwrap();
    ret.set(value.into());
}

fn get_env_var_exists(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let key = args.get(0);
    let key = key.to_string(scope).unwrap();
    let key = key.to_rust_string_lossy(scope);
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
        let key = v8::String::new(scope, &k).unwrap();
        let val = v8::String::new(scope, &v).unwrap();
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
) -> v8::Local<'s, v8::Object> {
    let code = v8::String::new(scope, INIT_SYS_API).unwrap();
    let code_origin = super::create_script_origin(scope, "sys_api_init");
    let script = v8::Script::compile(scope, code, Some(&code_origin)).unwrap();
    let func = script.run(scope).unwrap();
    let func: v8::Local<v8::Function> = func.try_into().unwrap();

    // Construct the object to pass to the JS function
    let args_list = construct_args_list(wasm_file_name, args, scope);
    let env_vars = construct_env_vars(scope);
    let env_obj = v8::Object::new(scope);
    let env_vars_key = v8::String::new(scope, "env_vars").unwrap().into();
    env_obj.set(scope, env_vars_key, env_vars.into());
    let args_key = v8::String::new(scope, "args").unwrap().into();
    env_obj.set(scope, args_key, args_list.into());

    let undefined = v8::undefined(scope);
    func.call(scope, undefined.into(), &[obj.into(), env_obj.into()]);

    let set_env_var = v8::FunctionTemplate::new(scope, set_env_var);
    let set_env_var = set_env_var.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "set_env_var").unwrap();
    obj.set(scope, ident.into(), set_env_var.into());

    let unset_env_var = v8::FunctionTemplate::new(scope, unset_env_var);
    let unset_env_var = unset_env_var.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "unset_env_var").unwrap();
    obj.set(scope, ident.into(), unset_env_var.into());

    let get_env_vars = v8::FunctionTemplate::new(scope, get_env_vars);
    let get_env_vars = get_env_vars.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "get_env_vars").unwrap();
    obj.set(scope, ident.into(), get_env_vars.into());

    let get_env_var = v8::FunctionTemplate::new(scope, get_env_var);
    let get_env_var = get_env_var.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "get_env_var").unwrap();
    obj.set(scope, ident.into(), get_env_var.into());

    let get_env_var_exists = v8::FunctionTemplate::new(scope, get_env_var_exists);
    let get_env_var_exists = get_env_var_exists.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "get_env_var_exists").unwrap();
    obj.set(scope, ident.into(), get_env_var_exists.into());

    obj
}
