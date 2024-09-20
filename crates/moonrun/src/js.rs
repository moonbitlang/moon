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

/*

fn begin_create_string() -> StringCreateHandle = "__moonbit_fs_unstable" "begin_create_string"

fn string_append_char(handle : StringCreateHandle, ch : Char) = "__moonbit_fs_unstable" "string_append_char"

fn finish_create_string(handle : StringCreateHandle) -> ExternString = "__moonbit_fs_unstable" "finish_create_string"


fn begin_read_string(s : ExternString) -> StringReadHandle = "__moonbit_fs_unstable" "begin_read_string"

/// Read one char from the string, returns -1 if the end of the string is reached.
/// The number returned is the unicode codepoint of the character.
fn string_read_char(handle : StringReadHandle) -> Int = "__moonbit_fs_unstable" "string_read_char"

fn finish_read_string(handle : StringReadHandle) = "__moonbit_fs_unstable" "finish_read_string"

*/

use v8::{Function, Local, Object};

const INIT_JS_API: &str = r#"
    (() => function(obj) {
        // String ops

        function begin_create_string() {
            return { s: "" }
        }

        function string_append_char(handle, ch) {
            handle.s += String.fromCharCode(ch)
        }

        function finish_create_string(handle) {
            return handle.s
        }

        function begin_read_string(s) {
            return { s: s, i: 0 }
        }

        function string_read_char(handle) {
            if (handle.i >= handle.s.length) {
                return -1
            }
            return handle.s.charCodeAt(handle.i++)
        }

        function finish_read_string(handle) {
            return
        }

        function begin_read_byte_array(arr) {
            return { arr: arr, i: 0 }
        }

        function byte_array_read_byte(handle) {
            if (handle.i >= handle.arr.length) {
                return -1
            }
            return handle.arr[handle.i++]
        }

        function finish_read_byte_array(handle) {
            return
        }

        function begin_create_byte_array() {
            return { arr: [] }
        }

        function byte_array_append_byte(handle, byte) {
            handle.arr.push(byte)
        }

        function finish_create_byte_array(handle) {
            return new Uint8Array(handle.arr)
        }

        function begin_read_string_array(arr) {
            return { arr: arr, i: 0 }
        }

        function string_array_read_string(handle) {
            if (handle.i >= handle.arr.length) {
                return "ffi_end_of_/string_array"
            }
            return handle.arr[handle.i++]
        }

        function finish_read_string_array(handle) {
            return
        }

        // Array ops

        function array_len(arr) {
            return arr.length
        }

        function array_get(arr, idx) {
            return arr[idx]
        }

        // JSValue

        function jsvalue_is_string(v) {
            return typeof v === "string"
        }

        obj.begin_create_string = begin_create_string
        obj.string_append_char = string_append_char
        obj.finish_create_string = finish_create_string
        obj.begin_read_string = begin_read_string
        obj.string_read_char = string_read_char
        obj.finish_read_string = finish_read_string

        obj.begin_read_byte_array = begin_read_byte_array
        obj.byte_array_read_byte = byte_array_read_byte
        obj.finish_read_byte_array = finish_read_byte_array
        obj.begin_create_byte_array = begin_create_byte_array
        obj.byte_array_append_byte = byte_array_append_byte
        obj.finish_create_byte_array = finish_create_byte_array

        obj.begin_read_string_array = begin_read_string_array
        obj.string_array_read_string = string_array_read_string
        obj.finish_read_string_array = finish_read_string_array

        obj.array_len = array_len
        obj.array_get = array_get

        obj.jsvalue_is_string = jsvalue_is_string
    })()
"#;

pub fn init_env<'s>(
    obj: v8::Local<'s, Object>,
    scope: &mut v8::HandleScope<'s>,
) -> v8::Local<'s, Object> {
    let code = v8::String::new(scope, INIT_JS_API).unwrap();
    let code_origin = super::create_script_origin(scope, "js_api_init");
    let script = v8::Script::compile(scope, code, Some(&code_origin)).unwrap();
    let func = script.run(scope).unwrap();
    let func: Local<Function> = func.try_into().unwrap();
    let undefined = v8::undefined(scope);
    func.call(scope, undefined.into(), &[obj.into()]);
    obj
}
