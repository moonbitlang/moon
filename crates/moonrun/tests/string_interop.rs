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

use moonrun::{Engine, RunOptions, RunOutcome};

#[test]
fn host_builders_and_native_builtins_share_lossless_utf16_strings() {
    let engine = Engine::default();
    let module = engine.compile("string-interop.wasm", wat::parse_str(r#"(module
        (type $chars (array (mut i16)))
        (import "_" "😄" (global $emoji (ref extern)))
        (import "__moonbit_fs_unstable" "begin_create_string" (func $builder (result externref)))
        (import "__moonbit_fs_unstable" "string_append_char" (func $append (param externref i32)))
        (import "__moonbit_fs_unstable" "finish_create_string" (func $finish (param externref) (result externref)))
        (import "__moonbit_fs_unstable" "begin_read_string" (func $reader (param externref) (result externref)))
        (import "__moonbit_fs_unstable" "string_read_char" (func $read (param externref) (result i32)))
        (import "__moonbit_fs_unstable" "jsvalue_is_string" (func $host_test (param externref) (result i32)))
        (import "wasm:js-string" "test" (func $test (param externref) (result i32)))
        (import "wasm:js-string" "length" (func $length (param externref) (result i32)))
        (import "wasm:js-string" "concat" (func $concat (param externref externref) (result (ref extern))))
        (import "wasm:js-string" "intoCharCodeArray" (func $into (param externref (ref null $chars) i32) (result i32)))
        (import "wasm:js-string" "fromCharCodeArray" (func $from (param (ref null $chars) i32 i32) (result (ref extern))))
        (func $eq (param i32 i32) local.get 0 local.get 1 i32.ne if unreachable end)
        (func (export "_start") (local $b externref) (local $s externref) (local $a (ref $chars)) (local $r externref)
            call $builder local.tee $b call $test i32.const 0 call $eq
            local.get $b call $host_test i32.const 0 call $eq
            ref.null extern call $host_test i32.const 0 call $eq
            local.get $b i32.const 0xd800 call $append
            local.get $b i32.const 0 call $append
            local.get $b call $finish global.get $emoji call $concat local.set $s
            local.get $s call $length i32.const 4 call $eq
            local.get $s call $host_test i32.const 1 call $eq
            i32.const 4 array.new_default $chars local.set $a
            local.get $s local.get $a i32.const 0 call $into i32.const 4 call $eq
            local.get $a i32.const 0 i32.const 4 call $from call $reader local.set $r
            local.get $r call $read i32.const 0xd800 call $eq
            local.get $r call $read i32.const 0 call $eq
            local.get $r call $read i32.const 0xd83d call $eq
            local.get $r call $read i32.const 0xde04 call $eq
            local.get $r call $read i32.const -1 call $eq))"#).unwrap()).unwrap();
    for _ in 0..3 {
        assert_eq!(
            engine.run(&module, RunOptions::default()).unwrap(),
            RunOutcome::Completed
        );
    }
}

#[test]
fn host_arguments_readers_and_utf8_results_are_native_strings() {
    let engine = Engine::default();
    let module = engine.compile("args.wasm", wat::parse_str(r#"(module
        (import "_" "λ😄" (global $argument (ref extern)))
        (import "__moonbit_fs_unstable" "args_get" (func $args (result externref)))
        (import "__moonbit_fs_unstable" "array_get" (func $get (param externref i32) (result externref)))
        (import "__moonbit_fs_unstable" "begin_read_string_array" (func $reader (param externref) (result externref)))
        (import "__moonbit_fs_unstable" "string_array_read_string" (func $read (param externref) (result externref)))
        (import "__moonbit_fs_unstable" "current_dir" (func $cwd (result externref)))
        (import "wasm:js-string" "equals" (func $eq (param externref externref) (result i32)))
        (import "wasm:js-string" "length" (func $length (param externref) (result i32)))
        (func (export "_start") (local $r externref)
            call $args i32.const 1 call $get global.get $argument call $eq i32.eqz if unreachable end
            call $args call $reader local.set $r
            local.get $r call $read call $length i32.eqz if unreachable end
            local.get $r call $read global.get $argument call $eq i32.eqz if unreachable end
            call $cwd call $length i32.eqz if unreachable end))"#).unwrap()).unwrap();
    assert_eq!(
        engine
            .run(&module, RunOptions::default().with_args(["λ😄"]))
            .unwrap(),
        RunOutcome::Completed
    );
}
