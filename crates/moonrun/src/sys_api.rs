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

        function env_get_vars() {
            let result = []
            for (let [key, value] of run_env.env_vars) {
                result.push(key)
                result.push(value)
            }
            return result
        }

        obj.env_get_var = env_get_var
        obj.args_get = args_get
        obj.env_get_vars = env_get_vars
    })()
"#;

fn construct_args_list<'s>(
    args: &[String],
    scope: &mut v8::HandleScope<'s>,
) -> v8::Local<'s, v8::Array> {
    let cli_args: Vec<String> = std::env::args()
        // path of moonrun and the path of the wasm file
        .take(2)
        .chain(args.iter().cloned())
        .collect();

    let arr = v8::Array::new(scope, args.len() as i32);
    for (i, arg) in cli_args.iter().enumerate() {
        let arg = v8::String::new(scope, arg).unwrap();
        arr.set_index(scope, i as u32, arg.into());
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

pub fn init_env<'s>(
    obj: v8::Local<'s, v8::Object>,
    scope: &mut v8::HandleScope<'s>,
    args: &[String],
) -> v8::Local<'s, v8::Object> {
    let code = v8::String::new(scope, INIT_SYS_API).unwrap();
    let code_origin = super::create_script_origin(scope, "sys_api_init");
    let script = v8::Script::compile(scope, code, Some(&code_origin)).unwrap();
    let func = script.run(scope).unwrap();
    let func: v8::Local<v8::Function> = func.try_into().unwrap();

    // Construct the object to pass to the JS function
    let args_list = construct_args_list(args, scope);
    let env_vars = construct_env_vars(scope);
    let env_obj = v8::Object::new(scope);
    let env_vars_key = v8::String::new(scope, "env_vars").unwrap().into();
    env_obj.set(scope, env_vars_key, env_vars.into());
    let args_key = v8::String::new(scope, "args").unwrap().into();
    env_obj.set(scope, args_key, args_list.into());

    let undefined = v8::undefined(scope);
    func.call(scope, undefined.into(), &[obj.into(), env_obj.into()]);
    obj
}
