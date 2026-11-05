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

use moonrun::{Engine, RunOptions, RunOutcome, WorkingDirectory};

#[cfg(unix)]
mod errno {
    pub const ACCESS: i32 = libc::EACCES;
    pub const FAULT: i32 = libc::EFAULT;
    pub const INVAL: i32 = libc::EINVAL;
    pub const MISSING: i32 = libc::ENOENT;
}

#[cfg(windows)]
mod errno {
    use windows_sys::Win32::Foundation::*;
    pub const ACCESS: i32 = ERROR_ACCESS_DENIED as i32;
    pub const FAULT: i32 = ERROR_INVALID_ADDRESS as i32;
    pub const INVAL: i32 = ERROR_INVALID_PARAMETER as i32;
    pub const MISSING: i32 = ERROR_ENVVAR_NOT_FOUND as i32;
}

fn error_globals() -> String {
    format!(
        r#"(global $access i32 (i32.const {access}))
           (global $fault i32 (i32.const {fault}))
           (global $inval i32 (i32.const {inval}))
           (global $missing i32 (i32.const {missing}))"#,
        access = errno::ACCESS,
        fault = errno::FAULT,
        inval = errno::INVAL,
        missing = errno::MISSING,
    )
}

const IMPORTS: &str = r#"
    (import "moonbitlang/core" "env/current_dir" (func $cwd (result i64)))
    (import "moonbitlang/core" "env/get_env_var" (func $get (param i32 i32) (result i64)))
    (import "moonbitlang/core" "env/set_env_var" (func $set (param i32 i32 i32 i32) (result i32)))
    (import "moonbitlang/core" "env/unset_env_var" (func $unset (param i32 i32) (result i32)))
    (import "moonbitlang/async" "c_buffer/is_null" (func $is_null (param i64) (result i32)))
    (import "moonbitlang/async" "c_buffer/length" (func $length (param i64) (result i32)))
    (import "moonbitlang/async" "c_buffer/blit_from_c" (func $copy (param i64 i32 i32 i32 i32)))
    (import "moonbitlang/async" "c_buffer/free" (func $free (param i64)))
    (import "moonbitlang/async" "os_error/get_errno" (func $errno (result i32)))
"#;

fn native_literal(value: impl AsRef<std::ffi::OsStr>) -> (String, usize) {
    #[cfg(unix)]
    let bytes = {
        use std::os::unix::ffi::OsStrExt;
        value.as_ref().as_bytes().to_vec()
    };
    #[cfg(windows)]
    let bytes = {
        use std::os::windows::ffi::OsStrExt;
        value
            .as_ref()
            .encode_wide()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>()
    };
    let literal = bytes.iter().map(|b| format!("\\{b:02x}")).collect();
    (literal, bytes.len())
}

fn run(body: &str, options: RunOptions) {
    let engine = Engine::default();
    let errors = error_globals();
    let module = engine
        .compile(
            "core-environment.wasm",
            wat::parse_str(format!(
                r#"(module {IMPORTS}
                    {body}
                    {errors}
                    (memory (export "memory") 1)
                    (func $eq (param i32 i32)
                        local.get 0 local.get 1 i32.ne if unreachable end)
                    (func $bytes_eq (param $left i32) (param $right i32) (param $len i32)
                        (local $i i32)
                        (block $done (loop $next
                            (br_if $done (i32.eq (local.get $i) (local.get $len)))
                            (call $eq
                                (i32.load8_u (i32.add (local.get $left) (local.get $i)))
                                (i32.load8_u (i32.add (local.get $right) (local.get $i))))
                            (local.set $i (i32.add (local.get $i) (i32.const 1)))
                            (br $next)))))"#
            ))
            .unwrap(),
        )
        .unwrap();
    assert_eq!(engine.run(&module, options).unwrap(), RunOutcome::Completed);
}

#[test]
fn environment_snapshot_uses_native_encoding_without_a_terminator() {
    let (key, key_len) = native_literal("KEY");
    let (value, len) = native_literal("λ🦀");
    run(
        &format!(
            r#"(data (i32.const 0) "{key}")
        (data (i32.const 64) "{value}")
        (data (i32.const 128) "........")
        (func (export "_start") (local $snapshot i64)
            (local.set $snapshot (call $get (i32.const 0) (i32.const {key_len})))
            (call $eq (call $is_null (local.get $snapshot)) (i32.const 0))
            (call $eq (call $length (local.get $snapshot)) (i32.const {len}))
            (call $copy (local.get $snapshot) (i32.const 0) (i32.const 128) (i32.const 0) (i32.const {len}))
            (call $free (local.get $snapshot))
            (call $bytes_eq (i32.const 128) (i32.const 64) (i32.const {len}))
            (call $eq (i32.load8_u (i32.const {end})) (i32.const 46)))"#,
            end = 128 + len
        ),
        RunOptions::default().with_environment([("KEY", "λ🦀")]),
    );
}

#[test]
fn environment_snapshots_survive_mutation_and_unset() {
    let (key, key_len) = native_literal("KEY");
    let (old, old_len) = native_literal("a");
    let (new, new_len) = native_literal("longer");
    run(
        &format!(
            r#"(data (i32.const 0) "{key}")
        (data (i32.const 32) "{old}")
        (data (i32.const 64) "{new}")
        (func (export "_start") (local $old i64) (local $new i64)
            (local.set $old (call $get (i32.const 0) (i32.const {key_len})))
            (call $eq (call $set (i32.const 0) (i32.const {key_len}) (i32.const 64) (i32.const {new_len})) (i32.const 0))
            (local.set $new (call $get (i32.const 0) (i32.const {key_len})))
            (call $eq (call $unset (i32.const 0) (i32.const {key_len})) (i32.const 0))
            (call $eq (call $is_null (call $get (i32.const 0) (i32.const {key_len}))) (i32.const 1))
            (call $eq (call $errno) (global.get $missing))
            (call $eq (call $length (local.get $old)) (i32.const {old_len}))
            (call $eq (call $length (local.get $new)) (i32.const {new_len}))
            (call $copy (local.get $old) (i32.const 0) (i32.const 128) (i32.const 0) (i32.const {old_len}))
            (call $free (local.get $old))
            (call $copy (local.get $new) (i32.const 0) (i32.const 160) (i32.const 0) (i32.const {new_len}))
            (call $free (local.get $new))
            (call $bytes_eq (i32.const 128) (i32.const 32) (i32.const {old_len}))
            (call $bytes_eq (i32.const 160) (i32.const 64) (i32.const {new_len})))"#
        ),
        RunOptions::default().with_environment([("KEY", "a")]),
    );
}

#[test]
fn empty_value_has_a_live_handle_but_missing_variable_does_not() {
    let (key, key_len) = native_literal("KEY");
    let (missing, missing_len) = native_literal("missing");
    run(
        &format!(
            r#"(data (i32.const 0) "{key}")
        (data (i32.const 32) "{missing}")
        (func (export "_start") (local $snapshot i64)
            (local.set $snapshot (call $get (i32.const 0) (i32.const {key_len})))
            (call $eq (call $is_null (local.get $snapshot)) (i32.const 0))
            (call $eq (call $length (local.get $snapshot)) (i32.const 0))
            (call $free (local.get $snapshot))
            (call $eq (call $is_null (call $get (i32.const 32) (i32.const {missing_len}))) (i32.const 1))
            (call $eq (call $errno) (global.get $missing)))"#
        ),
        RunOptions::default().with_environment([("KEY", "")]),
    );
}

#[test]
fn snapshot_outlives_input_reuse_and_guest_memory_growth() {
    let (key, key_len) = native_literal("KEY");
    let (value, len) = native_literal("keep");
    run(
        &format!(
            r#"(data (i32.const 0) "{key}")
        (data (i32.const 32) "{value}")
        (func (export "_start") (local $snapshot i64)
            (local.set $snapshot (call $get (i32.const 0) (i32.const {key_len})))
            (i32.store (i32.const 0) (i32.const 0))
            (drop (memory.grow (i32.const 1)))
            (call $copy (local.get $snapshot) (i32.const 0) (i32.const 65536) (i32.const 0) (i32.const {len}))
            (call $free (local.get $snapshot))
            (call $bytes_eq (i32.const 65536) (i32.const 32) (i32.const {len})))"#
        ),
        RunOptions::default().with_environment([("KEY", "keep")]),
    );
}

#[test]
fn invalid_input_cannot_mutate_environment() {
    let (key, key_len) = native_literal("KEY");
    let (value, value_len) = native_literal("keep");
    let (nul, unit_len) = native_literal("\0");
    let (invalid, _) = native_literal("=");
    // Native name/NUL constraints and invalid unsigned memory ranges.
    for (key_range, value_range, expected) in [
        ((0, key_len), (64, unit_len), errno::INVAL),
        ((96, unit_len), (32, value_len), errno::INVAL),
        ((0, 0), (32, value_len), errno::INVAL),
        ((0, key_len), (65536, unit_len), errno::FAULT),
        ((0, key_len), (1, u32::MAX as usize), errno::FAULT),
        ((u32::MAX as usize, 2), (32, value_len), errno::FAULT),
    ] {
        run(
            &format!(
                r#"(data (i32.const 0) "{key}")
                (data (i32.const 32) "{value}")
                (data (i32.const 64) "{nul}")
                (data (i32.const 96) "{invalid}")
                (func (export "_start") (local $snapshot i64)
                    (call $eq (call $set (i32.const {kp}) (i32.const {kl}) (i32.const {vp}) (i32.const {vl})) (i32.const {expected}))
                    (local.set $snapshot (call $get (i32.const 0) (i32.const {key_len})))
                    (call $eq (call $length (local.get $snapshot)) (i32.const {value_len}))
                    (call $copy (local.get $snapshot) (i32.const 0) (i32.const 128) (i32.const 0) (i32.const {value_len}))
                    (call $free (local.get $snapshot))
                    (call $bytes_eq (i32.const 128) (i32.const 32) (i32.const {value_len})))"#,
                kp = key_range.0,
                kl = key_range.1,
                vp = value_range.0,
                vl = value_range.1
            ),
            RunOptions::default().with_environment([("KEY", "keep")]),
        );
    }
}

#[test]
fn invalid_lookup_input_returns_null_and_native_error() {
    for (pointer, length, expected) in [
        (65536, 1, errno::FAULT),
        (1, u32::MAX, errno::FAULT),
        (u32::MAX, 1, errno::FAULT),
    ] {
        run(
            &format!(
                r#"(data (i32.const 0) "\ff...")
                (func (export "_start")
                    (call $eq (call $is_null (call $get (i32.const {pointer}) (i32.const {length}))) (i32.const 1))
                    (call $eq (call $errno) (i32.const {expected}))
                    (call $eq (i32.load (i32.const 0)) (i32.const 0x2e2e2eff)))"#,
            ),
            RunOptions::default(),
        );
    }
}

#[test]
fn unset_removes_only_a_valid_named_variable() {
    let (key, key_len) = native_literal("KEY");
    let (invalid, unit_len) = native_literal("=");
    let (nul, _) = native_literal("\0");
    run(
        &format!(
            r#"(data (i32.const 0) "{key}")
        (data (i32.const 32) "{invalid}")
        (data (i32.const 64) "{nul}")
        (func (export "_start") (local $snapshot i64)
            (call $eq (call $unset (i32.const -1) (i32.const {key_len})) (global.get $fault))
            (call $eq (call $unset (i32.const 32) (i32.const {unit_len})) (global.get $inval))
            (call $eq (call $unset (i32.const 64) (i32.const {unit_len})) (global.get $inval))
            (local.set $snapshot (call $get (i32.const 0) (i32.const {key_len})))
            (call $eq (call $is_null (local.get $snapshot)) (i32.const 0))
            (call $free (local.get $snapshot))
            (call $eq (call $unset (i32.const 0) (i32.const {key_len})) (i32.const 0))
            (call $eq (call $is_null (call $get (i32.const 0) (i32.const {key_len}))) (i32.const 1))
            (call $eq (call $errno) (global.get $missing)))"#
        ),
        RunOptions::default().with_environment([("KEY", "value")]),
    );
}

#[test]
fn environment_state_is_shared_with_legacy_and_wasi() {
    let (key, key_len) = native_literal("KEY");
    let (value, value_len) = native_literal("λ🦀");
    let source = format!(
        r#"
        (import "_" "KEY" (global $key (ref extern)))
        (import "_" "λ🦀" (global $value (ref extern)))
        (import "__moonbit_fs_unstable" "get_env_var" (func $legacy_get (param externref) (result externref)))
        (import "__moonbit_fs_unstable" "set_env_var" (func $legacy_set (param externref externref)))
        (import "__moonbit_fs_unstable" "unset_env_var" (func $legacy_unset (param externref)))
        (import "__moonbit_fs_unstable" "get_env_var_exists" (func $exists (param externref) (result i32)))
        (import "wasm:js-string" "equals" (func $equals (param externref externref) (result i32)))
        (import "wasi_snapshot_preview1" "environ_sizes_get" (func $sizes (param i32 i32) (result i32)))
        (import "wasi_snapshot_preview1" "environ_get" (func $entries (param i32 i32) (result i32)))
        (data (i32.const 0) "{key}")
        (data (i32.const 16) "{value}")
        (func (export "_start") (local $snapshot i64)
            (call $eq (call $exists (global.get $key)) (i32.const 0))
            (call $eq (call $set (i32.const 0) (i32.const {key_len}) (i32.const 16) (i32.const {value_len})) (i32.const 0))
            (call $eq (call $equals (call $legacy_get (global.get $key)) (global.get $value)) (i32.const 1))
            (call $eq (call $sizes (i32.const 32) (i32.const 36)) (i32.const 0))
            (call $eq (i32.load (i32.const 32)) (i32.const 1))
            (call $eq (i32.load (i32.const 36)) (i32.const 11))
            (call $eq (call $entries (i32.const 40) (i32.const 64)) (i32.const 0))
            (call $eq (i32.load (i32.const 64)) (i32.const 0x3d59454b)) ;; KEY=
            (call $eq (i32.load16_u (i32.const 68)) (i32.const 0xbbce))
            (call $eq (i32.load (i32.const 70)) (i32.const 0x80a69ff0))
            (call $legacy_unset (global.get $key))
            (call $eq (call $is_null (call $get (i32.const 0) (i32.const {key_len}))) (i32.const 1))
            (call $eq (call $errno) (global.get $missing))
            (call $legacy_set (global.get $key) (global.get $value))
            (local.set $snapshot (call $get (i32.const 0) (i32.const {key_len})))
            (call $copy (local.get $snapshot) (i32.const 0) (i32.const 64) (i32.const 0) (i32.const {value_len}))
            (call $free (local.get $snapshot))
            (call $bytes_eq (i32.const 64) (i32.const 16) (i32.const {value_len}))
            (call $eq (call $unset (i32.const 0) (i32.const {key_len})) (i32.const 0))
            (call $eq (call $exists (global.get $key)) (i32.const 0)))"#
    );
    let ambient = std::env::var_os("KEY");
    run(
        &source,
        RunOptions::default().with_environment([] as [(&str, &str); 0]),
    );
    assert_eq!(std::env::var_os("KEY"), ambient);
}

#[test]
fn freed_snapshot_cannot_alias_a_replacement_buffer() {
    let (key, key_len) = native_literal("KEY");
    let engine = Engine::default();
    let module = engine
        .compile(
            "stale-snapshot.wasm",
            wat::parse_str(format!(
                r#"(module
        {IMPORTS}
        (memory (export "memory") 1)
        (data (i32.const 0) "{key}")
        (func (export "_start") (local $old i64) (local $new i64)
            (local.set $old (call $get (i32.const 0) (i32.const {key_len})))
            (call $free (local.get $old))
            (local.set $new (call $get (i32.const 0) (i32.const {key_len})))
            ;; Reusing the slot must not make the stale Handle valid again.
            (if (i64.eq (local.get $old) (local.get $new)) (then unreachable))
            (call $free (local.get $new))
            (drop (call $length (local.get $old)))))"#
            ))
            .unwrap(),
        )
        .unwrap();
    let error = engine
        .run(
            &module,
            RunOptions::default().with_environment([("KEY", "value")]),
        )
        .unwrap_err();
    assert!(
        format!("{error:#}").contains("c_buffer/length"),
        "{error:#}"
    );
}

#[test]
fn non_unicode_environment_snapshot_preserves_native_bytes() {
    #[cfg(unix)]
    let (value, key, key_len, expected, len) = {
        use std::os::unix::ffi::OsStringExt;
        (std::ffi::OsString::from_vec(vec![0xff]), "KEY", 3, 0xff, 1)
    };
    #[cfg(windows)]
    let (value, key, key_len, expected, len) = {
        use std::os::windows::ffi::OsStringExt;
        (
            std::ffi::OsString::from_wide(&[0xd800]),
            r"K\00E\00Y\00",
            6,
            0xd800,
            2,
        )
    };
    run(
        &format!(
            r#"(data (i32.const 0) "{key}")
        (func (export "_start") (local $snapshot i64)
            (local.set $snapshot (call $get (i32.const 0) (i32.const {key_len})))
            (call $eq (call $is_null (local.get $snapshot)) (i32.const 0))
            (call $eq (call $length (local.get $snapshot)) (i32.const {len}))
            (call $copy (local.get $snapshot) (i32.const 0) (i32.const 64) (i32.const 0) (i32.const {len}))
            (call $free (local.get $snapshot))
            (call $eq (i32.load16_u (i32.const 64)) (i32.const {expected})))"#
        ),
        RunOptions::default().with_environment([("KEY", value)]),
    );
}

#[test]
fn non_unicode_names_and_values_roundtrip_from_unaligned_input() {
    #[cfg(unix)]
    let (key, key_len, value, len) = (r"K\ff", 2, r"\fe\ff", 2);
    #[cfg(windows)]
    let (key, key_len, value, len) = (r"K\00\00\d8", 4, r"\00\dc\00\d8", 4);
    run(
        &format!(
            r#"(data (i32.const 1) "{key}")
        (data (i32.const 17) "{value}")
        (func (export "_start") (local $snapshot i64)
            (call $eq (call $set (i32.const 1) (i32.const {key_len}) (i32.const 17) (i32.const {len})) (i32.const 0))
            (local.set $snapshot (call $get (i32.const 1) (i32.const {key_len})))
            (call $eq (call $length (local.get $snapshot)) (i32.const {len}))
            (call $copy (local.get $snapshot) (i32.const 0) (i32.const 64) (i32.const 0) (i32.const {len}))
            (call $free (local.get $snapshot))
            (call $bytes_eq (i32.const 64) (i32.const 17) (i32.const {len}))
            (call $eq (call $unset (i32.const 1) (i32.const {key_len})) (i32.const 0))
            (call $eq (call $is_null (call $get (i32.const 1) (i32.const {key_len}))) (i32.const 1))
            (call $eq (call $errno) (global.get $missing)))"#
        ),
        RunOptions::default().with_environment([] as [(&str, &str); 0]),
    );
}

#[cfg(windows)]
#[test]
fn windows_rejects_incomplete_code_units_without_mutation() {
    run(
        r#"(data (i32.const 0) "K\00E\00Y\00")
        (data (i32.const 16) "x\00")
        (func (export "_start") (local $snapshot i64)
            (call $eq (call $is_null (call $get (i32.const 0) (i32.const 5))) (i32.const 1))
            (call $eq (call $errno) (global.get $inval))
            (call $eq (call $set (i32.const 0) (i32.const 5) (i32.const 16) (i32.const 2)) (global.get $inval))
            (call $eq (call $set (i32.const 0) (i32.const 6) (i32.const 16) (i32.const 1)) (global.get $inval))
            (call $eq (call $unset (i32.const 0) (i32.const 5)) (global.get $inval))
            (local.set $snapshot (call $get (i32.const 0) (i32.const 6)))
            (call $eq (call $length (local.get $snapshot)) (i32.const 2))
            (call $copy (local.get $snapshot) (i32.const 0) (i32.const 64) (i32.const 0) (i32.const 2))
            (call $free (local.get $snapshot))
            (call $eq (i32.load16_u (i32.const 64)) (i32.const 107)))"#,
        RunOptions::default().with_environment([("KEY", "k")]),
    );
}

#[test]
fn missing_memory_reports_fault_instead_of_trapping() {
    let engine = Engine::default();
    let errors = error_globals();
    let module = engine.compile("core-no-memory.wasm", wat::parse_str(format!(r#"(module
        {IMPORTS}
        {errors}
        (func (export "_start")
            (if (i32.eqz (call $is_null (call $get (i32.const 0) (i32.const 1)))) (then unreachable))
            (if (i32.ne (call $errno) (global.get $fault)) (then unreachable))
            (if (i32.ne (call $set (i32.const 0) (i32.const 1) (i32.const 4) (i32.const 1)) (global.get $fault)) (then unreachable))
            (if (i32.ne (call $unset (i32.const 0) (i32.const 1)) (global.get $fault)) (then unreachable))))"#)).unwrap()).unwrap();
    assert_eq!(
        engine.run(&module, RunOptions::default()).unwrap(),
        RunOutcome::Completed
    );
}

#[test]
fn cwd_snapshot_needs_no_guest_memory_even_during_start() {
    let engine = Engine::default();
    let module = engine
        .compile(
            "cwd-no-memory.wasm",
            wat::parse_str(format!(
                r#"(module
        {IMPORTS}
        (func $initialize (local $snapshot i64)
            (local.set $snapshot (call $cwd))
            (if (call $is_null (local.get $snapshot)) (then unreachable))
            (if (i32.eqz (call $length (local.get $snapshot))) (then unreachable))
            (call $free (local.get $snapshot)))
        (start $initialize)
        (func (export "_start") (call $initialize)))"#
            ))
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        engine.run(&module, RunOptions::default()).unwrap(),
        RunOutcome::Completed
    );
}

#[test]
fn cwd_uses_the_configured_directory_and_matches_legacy() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("cwd-λ");
    std::fs::create_dir(&path).unwrap();
    let expected = dunce::canonicalize(&path).unwrap();
    let (bytes, len) = native_literal(&expected);
    let expected = expected.to_str().unwrap();
    run(
        &format!(
            r#"
        (import "_" {expected:?} (global $expected (ref extern)))
        (import "__moonbit_fs_unstable" "current_dir" (func $legacy (result externref)))
        (import "wasm:js-string" "equals" (func $equals (param externref externref) (result i32)))
        (data (i32.const 1024) "{bytes}")
        (func (export "_start") (local $snapshot i64)
            (call $eq (call $equals (call $legacy) (global.get $expected)) (i32.const 1))
            (local.set $snapshot (call $cwd))
            (call $eq (call $length (local.get $snapshot)) (i32.const {len}))
            (call $copy (local.get $snapshot) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const {len}))
            (call $free (local.get $snapshot))
            (call $bytes_eq (i32.const 0) (i32.const 1024) (i32.const {len})))"#
        ),
        RunOptions::default().with_working_directory(WorkingDirectory::Fixed(path)),
    );
}

// macOS's default filesystem rejects this name with EILSEQ during creation.
// Raw Unix string encoding is still tested independently on macOS.
#[cfg(any(target_os = "linux", windows))]
#[test]
fn cwd_snapshot_preserves_non_unicode_path() {
    #[cfg(unix)]
    let name = {
        use std::os::unix::ffi::OsStringExt;
        std::ffi::OsString::from_vec(vec![b'c', 0xff])
    };
    #[cfg(windows)]
    let name = {
        use std::os::windows::ffi::OsStringExt;
        std::ffi::OsString::from_wide(&[b'c' as u16, 0xd800])
    };
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join(name);
    std::fs::create_dir(&path).unwrap();
    let (expected, len) = native_literal(dunce::canonicalize(&path).unwrap());
    run(
        &format!(
            r#"(data (i32.const 1024) "{expected}")
        (func (export "_start") (local $snapshot i64)
            (local.set $snapshot (call $cwd))
            (call $eq (call $length (local.get $snapshot)) (i32.const {len}))
            (call $copy (local.get $snapshot) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const {len}))
            (call $free (local.get $snapshot))
            (call $bytes_eq (i32.const 0) (i32.const 1024) (i32.const {len})))"#
        ),
        RunOptions::default().with_working_directory(WorkingDirectory::Fixed(path)),
    );
}

#[test]
fn cwd_keeps_the_existing_read_permission_check() {
    let root = tempfile::tempdir().unwrap();
    let policy = root.path().join("policy.toml");
    std::fs::write(&policy, "[fs]\nread = []\nwrite = []\n").unwrap();
    run(
        r#"(data (i32.const 32) "....")
        (func (export "_start")
            (call $eq (call $is_null (call $cwd)) (i32.const 1))
            (call $eq (call $errno) (global.get $access))
            (call $eq (i32.load (i32.const 32)) (i32.const 0x2e2e2e2e)))"#,
        RunOptions::default().with_policy_file(policy).unwrap(),
    );
}

#[test]
fn compiler_produced_guest_uses_bulk_environment_abi() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let directory = moon_test_util::test_dir::TestDir::from_case_root(
        manifest.join("tests/test_cases"),
        "test_core_environment.in",
        false,
    );
    let moon = escargot::CargoBuild::new()
        .manifest_path(manifest.join("../moon/Cargo.toml"))
        .bin("moon")
        .current_release()
        .run()
        .expect("build moon");
    let cwd = directory.join("工作目录");
    std::fs::create_dir(&cwd).unwrap();
    let cwd = dunce::canonicalize(cwd).unwrap();
    for (profile, flag) in [("debug", "--debug"), ("release", "--release")] {
        snapbox::cmd::Command::new(moon.path())
            .current_dir(&directory)
            .args(["build", "--target", "wasm", flag])
            .assert()
            .success();
        snapbox::cmd::Command::new(env!("CARGO_BIN_EXE_moonrun"))
            .current_dir(&cwd)
            .env("MOONRUN_EXPECTED_CWD", &cwd)
            .env_remove("MOONRUN_批量_🔑")
            .env_remove("MOONRUN_BULK_SNAPSHOT")
            .env_remove("MOONRUN_BULK_RAW")
            .env("MOONBIT_ASYNC_CHECK_FD_LEAK", "1")
            .arg(directory.join(format!("_build/wasm/{profile}/build/main/main.wasm")))
            .assert()
            .success()
            .stdout_eq("bulk environment ok\n")
            .stderr_eq("");
    }
}
