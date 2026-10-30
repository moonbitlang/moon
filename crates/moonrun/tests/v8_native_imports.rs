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

#![cfg(feature = "v8")]

fn run(source: &str) {
    let engine = moonrun::Engine::default();
    let module = engine
        .compile("native-imports.wasm", wat::parse_str(source).unwrap())
        .unwrap();
    assert_eq!(
        engine.run(&module, moonrun::RunOptions::default()).unwrap(),
        moonrun::RunOutcome::Completed
    );
}

#[test]
fn strings_preserve_utf16_units_and_builder_snapshots() {
    run(r#"(module
        (import "__moonbit_fs_unstable" "begin_create_string" (func $builder (result externref)))
        (import "__moonbit_fs_unstable" "string_append_char" (func $append (param externref i32)))
        (import "__moonbit_fs_unstable" "finish_create_string" (func $finish (param externref) (result externref)))
        (import "__moonbit_fs_unstable" "begin_read_string" (func $reader (param externref) (result externref)))
        (import "__moonbit_fs_unstable" "string_read_char" (func $read (param externref) (result i32)))
        (import "__moonbit_fs_unstable" "finish_read_string" (func $close (param externref)))
        (import "__moonbit_fs_unstable" "jsvalue_is_string" (func $is_string (param externref) (result i32)))
        (func $eq (param i32 i32) local.get 0 local.get 1 i32.ne if unreachable end)
        (func (export "_start") (local $b externref) (local $s externref) (local $r externref)
            call $builder local.set $b
            local.get $b call $is_string i32.const 0 call $eq
            local.get $b call $finish call $reader call $read i32.const -1 call $eq
            local.get $b i32.const 0xd800 call $append
            local.get $b i32.const 0x10041 call $append
            local.get $b i32.const 0xdcff call $append
            local.get $b i32.const 0 call $append
            local.get $b call $finish local.set $s
            local.get $s call $is_string i32.const 1 call $eq
            local.get $b i32.const 90 call $append
            local.get $s call $reader local.set $r
            local.get $r call $read i32.const 0xd800 call $eq
            local.get $r call $read i32.const 65 call $eq
            local.get $r call $read i32.const 0xdcff call $eq
            local.get $r call $read i32.const 0 call $eq
            local.get $r call $read i32.const -1 call $eq
            local.get $r call $read i32.const -1 call $eq
            local.get $r call $close
            local.get $r call $read i32.const -1 call $eq))"#);
}

#[test]
fn byte_builders_copy_on_finish_and_readers_observe_mutation() {
    run(r#"(module
        (import "__moonbit_fs_unstable" "begin_create_byte_array" (func $builder (result externref)))
        (import "__moonbit_fs_unstable" "byte_array_append_byte" (func $append (param externref i32)))
        (import "__moonbit_fs_unstable" "finish_create_byte_array" (func $finish (param externref) (result externref)))
        (import "__moonbit_fs_unstable" "begin_read_byte_array" (func $reader (param externref) (result externref)))
        (import "__moonbit_fs_unstable" "byte_array_read_byte" (func $read (param externref) (result i32)))
        (import "__moonbit_fs_unstable" "finish_read_byte_array" (func $close (param externref)))
        (import "ffi-bytes" "set" (func $set (param externref i32 i32) (result i32)))
        (import "ffi-bytes" "length" (func $length (param externref) (result i32)))
        (func $eq (param i32 i32) local.get 0 local.get 1 i32.ne if unreachable end)
        (func (export "_start") (local $b externref) (local $a externref) (local $r externref)
            call $builder local.set $b
            local.get $b call $finish call $length i32.const 0 call $eq
            local.get $b i32.const -1 call $append
            local.get $b i32.const 256 call $append
            local.get $b i32.const 300 call $append
            local.get $b call $finish local.set $a
            local.get $b i32.const 99 call $append
            local.get $a call $length i32.const 3 call $eq
            local.get $a call $reader local.set $r
            local.get $r call $read i32.const 255 call $eq
            local.get $a i32.const 1 i32.const 258 call $set i32.const 258 call $eq
            local.get $r call $read i32.const 2 call $eq
            local.get $r call $read i32.const 44 call $eq
            local.get $r call $read i32.const -1 call $eq
            local.get $r call $close
            local.get $r call $read i32.const -1 call $eq))"#);
}

#[test]
fn byte_operations_preserve_overlap_clamping_and_utf16() {
    run(r#"(module
        (import "ffi-bytes" "memory" (memory 1))
        (import "ffi-bytes" "from_memory" (func $from (param i32 i32) (result externref)))
        (import "ffi-bytes" "copy" (func $copy (param externref i32 externref i32 i32)))
        (import "ffi-bytes" "fill" (func $fill (param externref i32 i32 i32) (result externref)))
        (import "ffi-bytes" "get" (func $get (param externref i32) (result i32)))
        (import "ffi-bytes" "set" (func $set (param externref i32 i32)))
        (import "ffi-bytes" "length" (func $len (param externref) (result i32)))
        (import "ffi-bytes" "equals" (func $equals (param externref externref) (result i32)))
        (import "ffi-bytes" "asString" (func $string (param externref i32 i32) (result externref)))
        (import "__moonbit_fs_unstable" "begin_read_string" (func $reader (param externref) (result externref)))
        (import "__moonbit_fs_unstable" "string_read_char" (func $read (param externref) (result i32)))
        (data (i32.const 0) "\01\02\03\04\05\06")
        (data (i32.const 16) "\00\d8\41\00\ff\dc\99")
        (func $eq (param i32 i32) local.get 0 local.get 1 i32.ne if unreachable end)
        (func (export "_start") (local $a externref) (local $b externref) (local $r externref)
            i32.const 0 i32.const 6 call $from local.set $a
            local.get $a i32.const 1 local.get $a i32.const 0 i32.const 5 call $copy
            local.get $a i32.const 5 call $get i32.const 5 call $eq
            local.get $a i32.const 2 call $get i32.const 2 call $eq
            local.get $a i32.const 0 local.get $a i32.const 1 i32.const 5 call $copy
            local.get $a i32.const 0 call $get i32.const 1 call $eq
            local.get $a i32.const 4 call $get i32.const 5 call $eq
            local.get $a i32.const 0 local.get $a i32.const -2 i32.const 100 call $copy
            local.get $a i32.const 0 call $get i32.const 5 call $eq
            local.get $a i32.const -2 i32.const 300 i32.const 100 call $fill local.set $b
            local.get $a i32.const 4 call $get i32.const 44 call $eq
            local.get $b i32.const 5 call $get i32.const 44 call $eq
            local.get $a local.get $b call $equals i32.const 1 call $eq
            local.get $a i32.const -1 call $get i32.const 0 call $eq
            local.get $a i32.const 6 call $get i32.const 0 call $eq
            local.get $a i32.const 6 i32.const 123 call $set
            local.get $a call $len i32.const 6 call $eq
            i32.const 16 i32.const 7 call $from i32.const 0 i32.const 7 call $string
            call $reader local.set $r
            local.get $r call $read i32.const 0xd800 call $eq
            local.get $r call $read i32.const 65 call $eq
            local.get $r call $read i32.const 0xdcff call $eq
            local.get $r call $read i32.const -1 call $eq))"#);
}

#[test]
fn byte_memory_is_reacquired_after_growth_and_copies_are_independent() {
    run(r#"(module
        (import "ffi-bytes" "memory" (memory 1))
        (import "ffi-bytes" "from_memory" (func $from (param i32 i32) (result externref)))
        (import "ffi-bytes" "get" (func $get (param externref i32) (result i32)))
        (import "ffi-bytes" "length" (func $len (param externref) (result i32)))
        (func $eq (param i32 i32) local.get 0 local.get 1 i32.ne if unreachable end)
        (func (export "_start") (local $a externref)
            i32.const 0 i32.const 65 i32.store8
            i32.const 0 i32.const 1 call $from local.set $a
            i32.const 0 i32.const 66 i32.store8
            local.get $a i32.const 0 call $get i32.const 65 call $eq
            i32.const 1 memory.grow i32.const 1 call $eq
            i32.const 65536 i32.const 90 i32.store8
            i32.const 65536 i32.const 1 call $from i32.const 0 call $get i32.const 90 call $eq
            i32.const 131071 i32.const 10 call $from call $len i32.const 1 call $eq
            i32.const -1 i32.const 2 call $from call $len i32.const 0 call $eq))"#);
}

#[test]
fn array_helpers_and_string_array_end_sentinel_are_preserved() {
    run(r#"(module
        (import "__moonbit_fs_unstable" "args_get" (func $args (result externref)))
        (import "__moonbit_fs_unstable" "array_len" (func $len (param externref) (result i32)))
        (import "__moonbit_fs_unstable" "array_get" (func $get (param externref i32) (result externref)))
        (import "__moonbit_fs_unstable" "begin_read_string_array" (func $reader (param externref) (result externref)))
        (import "__moonbit_fs_unstable" "string_array_read_string" (func $read (param externref) (result externref)))
        (import "__moonbit_fs_unstable" "finish_read_string_array" (func $close (param externref)))
        (import "wasm:js-string" "equals" (func $equals (param externref externref) (result i32)))
        (import "_" "ffi_end_of_/string_array" (global $end externref))
        (func (export "_start") (local $a externref) (local $r externref)
            call $args local.set $a
            local.get $a call $len i32.const 1 i32.ne if unreachable end
            local.get $a call $reader local.set $r
            local.get $r call $read local.get $a i32.const 0 call $get
            call $equals i32.eqz if unreachable end
            local.get $r call $read global.get $end call $equals i32.eqz if unreachable end
            local.get $r call $close
            local.get $r call $read global.get $end call $equals i32.eqz if unreachable end))"#);
}

#[test]
fn host_exception_matches_the_imported_wasm_tag() {
    run(r#"(module
        (import "exception" "tag" (tag $tag))
        (import "exception" "throw" (func $throw))
        (func (export "_start")
            (block $caught
                (try_table (catch $tag $caught)
                    call $throw)
                unreachable)))"#);
}

#[test]
fn fractional_slice_indices_truncate_before_relative_clamping() {
    run(r#"(module
        (import "ffi-bytes" "memory" (memory 1))
        (import "ffi-bytes" "from_memory" (func $from (param f64 f64) (result externref)))
        (import "ffi-bytes" "length" (func $len (param externref) (result i32)))
        (import "ffi-bytes" "fill" (func $fill (param externref f64 i32 f64)))
        (import "ffi-bytes" "get" (func $get (param externref i32) (result i32)))
        (func $eq (param i32 i32) local.get 0 local.get 1 i32.ne if unreachable end)
        (func (export "_start") (local $a externref)
            f64.const -0.5 f64.const 3.5 call $from local.set $a
            local.get $a call $len i32.const 3 call $eq
            local.get $a f64.const -0.5 i32.const 65 f64.const 2.5 call $fill
            local.get $a i32.const 0 call $get i32.const 65 call $eq
            local.get $a i32.const 1 call $get i32.const 65 call $eq
            local.get $a i32.const 2 call $get i32.const 0 call $eq))"#);
}

#[test]
fn invalid_handles_and_byte_ranges_throw_catchable_exceptions() {
    for operation in [
        "i32.const -1 call $new drop",
        "ref.null extern i32.const 0 call $get drop",
        "call $builder call $read drop",
        "call $builder call $finish_bytes drop",
        "local.get $a i32.const -1 local.get $a i32.const 0 i32.const 0 call $copy",
        "local.get $a i32.const 2 local.get $a i32.const 0 i32.const 1 call $copy",
    ] {
        run(&format!(
            r#"(module
            (import "ffi-bytes" "new" (func $new (param i32) (result externref)))
            (import "ffi-bytes" "get" (func $get (param externref i32) (result i32)))
            (import "ffi-bytes" "copy" (func $copy (param externref i32 externref i32 i32)))
            (import "__moonbit_fs_unstable" "begin_create_string" (func $builder (result externref)))
            (import "__moonbit_fs_unstable" "string_read_char" (func $read (param externref) (result i32)))
            (import "__moonbit_fs_unstable" "finish_create_byte_array" (func $finish_bytes (param externref) (result externref)))
            (func (export "_start") (local $a externref)
                i32.const 2 call $new local.set $a
                (block $caught
                    (try_table (catch_all $caught) {operation})
                    unreachable)))"#
        ));
    }
}

#[test]
fn byte_to_string_conversion_does_not_depend_on_js_argument_stack_limits() {
    run(r#"(module
        (import "ffi-bytes" "new" (func $new (param i32) (result externref)))
        (import "ffi-bytes" "asString" (func $string (param externref i32 i32) (result externref)))
        (import "wasm:js-string" "length" (func $len (param externref) (result i32)))
        (func (export "_start")
            i32.const 1048576 call $new
            i32.const 0 i32.const 1048576 call $string
            call $len i32.const 524288 i32.ne if unreachable end))"#);
}
