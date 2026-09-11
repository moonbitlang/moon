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

fn compile(name: &str, source: &str) -> (moonrun::Engine, moonrun::Module) {
    let engine = moonrun::Engine::default();
    let module = engine
        .compile(name, wat::parse_str(source).unwrap())
        .unwrap();
    (engine, module)
}

#[test]
fn engine_backend_classifies_uncaught_wasm_failures_as_guest_failures() {
    let cases = [
        (
            "module-start-trap.wasm",
            r#"(module
                (func $module_start unreachable)
                (start $module_start))"#,
            moonrun::RunOptions::default(),
        ),
        (
            "exported-start-trap.wasm",
            r#"(module (func (export "_start") unreachable))"#,
            moonrun::RunOptions::default(),
        ),
        (
            "wasm-exception.wasm",
            r#"(module
                (import "exception" "tag" (tag $tag))
                (import "exception" "throw" (func $throw))
                (func (export "_start") call $throw))"#,
            moonrun::RunOptions::default(),
        ),
        (
            "js-string-trap.wasm",
            r#"(module
                (import "wasm:js-string" "fromCodePoint"
                    (func $from_code_point (param i32) (result (ref extern))))
                (func (export "_start")
                    i32.const -1
                    call $from_code_point
                    drop))"#,
            moonrun::RunOptions::default(),
        ),
        (
            "test-finish-trap.wasm",
            r#"(module
                (func (export "moonbit_test_driver_internal_execute")
                    (param externref i32))
                (func (export "moonbit_test_driver_finish") unreachable))"#,
            moonrun::RunOptions::default()
                .with_test_args(r#"{"package":"moon/core","file_and_index":[]}"#),
        ),
    ];

    for (name, source, options) in cases {
        let (engine, module) = compile(name, source);
        assert_eq!(
            engine.run(&module, options).unwrap(),
            moonrun::RunOutcome::Exited(1),
            "{name}"
        );
    }
}

#[test]
fn engine_backend_keeps_handled_test_failures_inside_the_test_driver() {
    let (engine, module) = compile(
        "test-execute-trap.wasm",
        r#"(module
            (func (export "moonbit_test_driver_internal_execute")
                (param externref i32)
                unreachable)
            (func (export "moonbit_test_driver_finish")))"#,
    );

    assert_eq!(
        engine
            .run(
                &module,
                moonrun::RunOptions::default().with_test_args(
                    r#"{"package":"moon/core","file_and_index":[["main_test.mbt",[{"start":0,"end":1}]]]}"#,
                ),
            )
            .unwrap(),
        moonrun::RunOutcome::Completed
    );
}

#[test]
fn engine_backend_preserves_explicit_run_termination() {
    let (engine, module) = compile(
        "exit.wasm",
        r#"(module
            (import "__moonbit_sys_unstable" "exit" (func $exit (param i32)))
            (func (export "_start") i32.const 7 call $exit))"#,
    );

    assert_eq!(
        engine.run(&module, moonrun::RunOptions::default()).unwrap(),
        moonrun::RunOutcome::Exited(7)
    );
}

#[test]
fn engine_backend_preserves_instantiation_link_errors() {
    let (engine, module) = compile(
        "missing-import.wasm",
        r#"(module
            (import "__moonbit_sys_unstable" "missing" (func)))"#,
    );

    let error = engine
        .run(&module, moonrun::RunOptions::default())
        .unwrap_err();
    let error = format!("{error:#}");
    assert!(
        error.contains("failed to instantiate `missing-import.wasm`")
            && error.contains("__moonbit_sys_unstable")
            && error.contains("missing"),
        "{error}"
    );
}

#[test]
fn engine_backend_preserves_host_adapter_errors() {
    let cases = [
        (
            "module-start-host-error.wasm",
            r#"(module
                (import "moonbit:ffi/memory-sanitizer" "register-object-free"
                    (func $free (param i32)))
                (func $module_start i32.const 1 call $free)
                (start $module_start))"#,
            moonrun::RunOptions::default(),
            Some("failed to instantiate"),
        ),
        (
            "exported-start-host-error.wasm",
            r#"(module
                (import "moonbit:ffi/memory-sanitizer" "register-object-free"
                    (func $free (param i32)))
                (func (export "_start") i32.const 1 call $free))"#,
            moonrun::RunOptions::default(),
            None,
        ),
        (
            "test-execute-host-error.wasm",
            r#"(module
                (import "moonbit:ffi/memory-sanitizer" "register-object-free"
                    (func $free (param i32)))
                (func (export "moonbit_test_driver_internal_execute")
                    (param externref i32)
                    i32.const 1
                    call $free)
                (func (export "moonbit_test_driver_finish")))"#,
            moonrun::RunOptions::default().with_test_args(
                r#"{"package":"moon/core","file_and_index":[["main_test.mbt",[{"start":0,"end":1}]]]}"#,
            ),
            Some("failed to execute a MoonBit test"),
        ),
        (
            "test-finish-host-error.wasm",
            r#"(module
                (import "moonbit:ffi/memory-sanitizer" "register-object-free"
                    (func $free (param i32)))
                (func (export "moonbit_test_driver_internal_execute")
                    (param externref i32))
                (func (export "moonbit_test_driver_finish")
                    i32.const 1
                    call $free))"#,
            moonrun::RunOptions::default()
                .with_test_args(r#"{"package":"moon/core","file_and_index":[]}"#),
            Some("failed to finish the MoonBit test driver"),
        ),
    ];

    for (name, source, options, expected_context) in cases {
        let (engine, module) = compile(name, source);
        let error = engine.run(&module, options).unwrap_err();
        let error = format!("{error:#}");
        assert!(
            expected_context.is_none_or(|context| error.contains(context))
                && error.contains("register-object-free failed")
                && error.contains("invalid object 1"),
            "{error}"
        );
    }
}
