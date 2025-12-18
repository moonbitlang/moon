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

fn run(source: &str) -> String {
    moonutil::moon_pkg::parse(source)
        .and_then(moonutil::package::convert_pkg_dsl_to_package)
        .map(|pkg| format!("{:#?}", pkg))
        .unwrap_or_else(|e| format!("Error: {:?}", e))
}

#[test]
fn expect_import() {
    let actual = run(r#"
      import {
        "path/to/pkg1",
        "path/to/another1" as @another1,
      }

      import "test" {
        "path/to/pkg2",
        "path/to/another2" as @another2,
      }

      import "wbtest" {
        "path/to/pkg3",
        "path/to/another3" as @another3,
      }
    "#);
    expect_test::expect![[r#"
        MoonPkg {
            name: None,
            is_main: false,
            force_link: false,
            sub_package: None,
            imports: [
                Simple(
                    "path/to/pkg1",
                ),
                Alias {
                    path: "path/to/another1",
                    alias: Some(
                        "another1",
                    ),
                    sub_package: false,
                },
            ],
            wbtest_imports: [
                Simple(
                    "path/to/pkg3",
                ),
                Alias {
                    path: "path/to/another3",
                    alias: Some(
                        "another3",
                    ),
                    sub_package: false,
                },
            ],
            test_imports: [
                Simple(
                    "path/to/pkg2",
                ),
                Alias {
                    path: "path/to/another2",
                    alias: Some(
                        "another2",
                    ),
                    sub_package: false,
                },
            ],
            formatter: MoonPkgFormatter {
                ignore: {},
            },
            link: None,
            warn_list: None,
            alert_list: None,
            targets: None,
            pre_build: None,
            bin_name: None,
            bin_target: None,
            supported_targets: {
                Wasm,
                WasmGC,
                Js,
                Native,
                LLVM,
            },
            native_stub: None,
            virtual_pkg: None,
            implement: None,
            overrides: None,
            max_concurrent_tests: None,
        }"#]]
    .assert_eq(&actual);
}

#[test]
fn expect_is_main() {
    let actual = run(r#"
      options("is-main": true)
    "#);
    expect_test::expect![[r#"
        MoonPkg {
            name: None,
            is_main: true,
            force_link: false,
            sub_package: None,
            imports: [],
            wbtest_imports: [],
            test_imports: [],
            formatter: MoonPkgFormatter {
                ignore: {},
            },
            link: None,
            warn_list: None,
            alert_list: None,
            targets: None,
            pre_build: None,
            bin_name: None,
            bin_target: None,
            supported_targets: {
                Wasm,
                WasmGC,
                Js,
                Native,
                LLVM,
            },
            native_stub: None,
            virtual_pkg: None,
            implement: None,
            overrides: None,
            max_concurrent_tests: None,
        }"#]]
    .assert_eq(&actual);
}

#[test]
fn expect_formatter() {
    let actual = run(r#"
    options(
      formatter: { "ignore": ["file1.mbt", "file2.mbt", "file3.mbt"] }
    )
    "#);
    expect_test::expect![[r#"
        MoonPkg {
            name: None,
            is_main: false,
            force_link: false,
            sub_package: None,
            imports: [],
            wbtest_imports: [],
            test_imports: [],
            formatter: MoonPkgFormatter {
                ignore: {
                    "file1.mbt",
                    "file2.mbt",
                    "file3.mbt",
                },
            },
            link: None,
            warn_list: None,
            alert_list: None,
            targets: None,
            pre_build: None,
            bin_name: None,
            bin_target: None,
            supported_targets: {
                Wasm,
                WasmGC,
                Js,
                Native,
                LLVM,
            },
            native_stub: None,
            virtual_pkg: None,
            implement: None,
            overrides: None,
            max_concurrent_tests: None,
        }"#]]
    .assert_eq(&actual);
}

#[test]
fn expect_supported_targets() {
    let actual = run(r#"
      options(
        "supported-targets": ["wasm", "js", "wasm-gc"]
      )
    "#);
    expect_test::expect![[r#"
        MoonPkg {
            name: None,
            is_main: false,
            force_link: false,
            sub_package: None,
            imports: [],
            wbtest_imports: [],
            test_imports: [],
            formatter: MoonPkgFormatter {
                ignore: {},
            },
            link: None,
            warn_list: None,
            alert_list: None,
            targets: None,
            pre_build: None,
            bin_name: None,
            bin_target: None,
            supported_targets: {
                Wasm,
                Js,
                WasmGC,
            },
            native_stub: None,
            virtual_pkg: None,
            implement: None,
            overrides: None,
            max_concurrent_tests: None,
        }"#]]
    .assert_eq(&actual);
}

#[test]
fn expect_build() {
    let actual = run(r#"
    options(
      "pre-build": [
        {
          "command": "exe1 $input -o $output",
          "input": "source1.mbt",
          "output": "output1.wasm",
        },
        {
          "command": "exe2 $input -o $output",
          "input": "source2.mbt",
          "output": "output2.wasm"
        }
      ] 
    )
    "#);
    expect_test::expect![[r#"
        MoonPkg {
            name: None,
            is_main: false,
            force_link: false,
            sub_package: None,
            imports: [],
            wbtest_imports: [],
            test_imports: [],
            formatter: MoonPkgFormatter {
                ignore: {},
            },
            link: None,
            warn_list: None,
            alert_list: None,
            targets: None,
            pre_build: Some(
                [
                    MoonPkgGenerate {
                        input: String(
                            "source1.mbt",
                        ),
                        output: String(
                            "output1.wasm",
                        ),
                        command: "exe1 $input -o $output",
                    },
                    MoonPkgGenerate {
                        input: String(
                            "source2.mbt",
                        ),
                        output: String(
                            "output2.wasm",
                        ),
                        command: "exe2 $input -o $output",
                    },
                ],
            ),
            bin_name: None,
            bin_target: None,
            supported_targets: {
                Wasm,
                WasmGC,
                Js,
                Native,
                LLVM,
            },
            native_stub: None,
            virtual_pkg: None,
            implement: None,
            overrides: None,
            max_concurrent_tests: None,
        }"#]]
    .assert_eq(&actual);
}

#[test]
fn expect_warnings() {
    let actual = run(r#"
    options(
      warnings: "-fragile_match-deprecated_syntax+unused_variable+todo@unused_variable@todo",
    )
    "#);
    expect_test::expect![[r#"
        MoonPkg {
            name: None,
            is_main: false,
            force_link: false,
            sub_package: None,
            imports: [],
            wbtest_imports: [],
            test_imports: [],
            formatter: MoonPkgFormatter {
                ignore: {},
            },
            link: None,
            warn_list: None,
            alert_list: None,
            targets: None,
            pre_build: None,
            bin_name: None,
            bin_target: None,
            supported_targets: {
                Wasm,
                WasmGC,
                Js,
                Native,
                LLVM,
            },
            native_stub: None,
            virtual_pkg: None,
            implement: None,
            overrides: None,
            max_concurrent_tests: None,
        }"#]]
    .assert_eq(&actual);
}

#[test]
fn expect_options() {
    let actual = run(r#"
    options(
      "virtual_pkg": { "has_default": false },
      "implement": "string",
      "overrides": [ "string1", "string2" ],
      "bin_name": "name",
      "bin_target": "wasm",
      "native_stub": [ "stub.c", "another_stub.c" ],
      "max_concurrent_tests": 4,
      "targets": {
        "file.mbt": "js",
        "platform_specific.mbt": ["or", ["and", "native", "release", ["not", "wasm"], ["not","debug"]], "llvm"]
      },
      "link": {
        "wasm": {
          "exports": ["f1", "f2"], 
          "heap_start_address": 100,
          "import_memory": {
            "module": "string",
            "name": "string",
          },
          "memory_limits": { "min": 1, "max": 10 },
          "shared_memory": true,
          "export_memory_name": "name",
          "flags": ["--flag1", "--flag2"]
        },
        "wasm_gc": {
          "exports": ["f1", "f2"],
          "import_memory": {
            "module": "string",
            "name": "string",
          },
          "memory_limits": { "min": 1, "max": 10 },
          "shared_memory": true,
          "export_memory_name": "name",
          "flags": ["--flag1", "--flag2"],
          "use_js_builtin_string": true,
          "imported_string_constants": "string",
        },
        "js": {
          "exports": ["f1", "f2"],
          "format": "esm", 
        },
        "native": {
          "exports": ["f1", "f2"],
          "cc": "gcc",
          "cflags": ["-flag1", "-flag2"],
          "cc_link_flags": "-flag1",
          "stub_cc": "gcc",
          "stub_cflags": "-flag2",
          "stub_cc_link_flags": "-flag1",
          "stub_lib_deps": ["lib1", "lib2"],
        },
      },
    )
    "#);
    expect_test::expect![[r#"
        MoonPkg {
            name: None,
            is_main: false,
            force_link: false,
            sub_package: None,
            imports: [],
            wbtest_imports: [],
            test_imports: [],
            formatter: MoonPkgFormatter {
                ignore: {},
            },
            link: Some(
                Link {
                    wasm: Some(
                        WasmLinkConfig {
                            exports: Some(
                                [
                                    "f1",
                                    "f2",
                                ],
                            ),
                            heap_start_address: None,
                            import_memory: None,
                            memory_limits: None,
                            shared_memory: None,
                            export_memory_name: None,
                            flags: Some(
                                [
                                    "--flag1",
                                    "--flag2",
                                ],
                            ),
                        },
                    ),
                    wasm_gc: None,
                    js: Some(
                        JsLinkConfig {
                            exports: Some(
                                [
                                    "f1",
                                    "f2",
                                ],
                            ),
                            format: Some(
                                ESM,
                            ),
                        },
                    ),
                    native: Some(
                        NativeLinkConfig {
                            exports: Some(
                                [
                                    "f1",
                                    "f2",
                                ],
                            ),
                            cc: Some(
                                "gcc",
                            ),
                            cc_flags: None,
                            cc_link_flags: None,
                            stub_cc: None,
                            stub_cc_flags: None,
                            stub_cc_link_flags: None,
                            stub_lib_deps: None,
                        },
                    ),
                },
            ),
            warn_list: None,
            alert_list: None,
            targets: Some(
                {
                    "file.mbt": Atom(
                        Target(
                            Js,
                        ),
                    ),
                    "platform_specific.mbt": Condition(
                        Or,
                        [
                            Condition(
                                And,
                                [
                                    Atom(
                                        Target(
                                            Native,
                                        ),
                                    ),
                                    Atom(
                                        OptLevel(
                                            Release,
                                        ),
                                    ),
                                    Condition(
                                        Not,
                                        [
                                            Atom(
                                                Target(
                                                    Wasm,
                                                ),
                                            ),
                                        ],
                                    ),
                                    Condition(
                                        Not,
                                        [
                                            Atom(
                                                OptLevel(
                                                    Debug,
                                                ),
                                            ),
                                        ],
                                    ),
                                ],
                            ),
                            Atom(
                                Target(
                                    LLVM,
                                ),
                            ),
                        ],
                    ),
                },
            ),
            pre_build: None,
            bin_name: Some(
                "name",
            ),
            bin_target: Some(
                Wasm,
            ),
            supported_targets: {
                Wasm,
                WasmGC,
                Js,
                Native,
                LLVM,
            },
            native_stub: Some(
                [
                    "stub.c",
                    "another_stub.c",
                ],
            ),
            virtual_pkg: Some(
                VirtualPkg {
                    has_default: false,
                },
            ),
            implement: Some(
                "string",
            ),
            overrides: Some(
                [
                    "string1",
                    "string2",
                ],
            ),
            max_concurrent_tests: Some(
                4,
            ),
        }"#]]
    .assert_eq(&actual);
}

#[test]
fn expect_max_concurrent_tests() {
    let actual = run(r#"
      options(
        "max_concurrent_tests": 1,
      ) 
    "#);
    expect_test::expect![[r#"
        MoonPkg {
            name: None,
            is_main: false,
            force_link: false,
            sub_package: None,
            imports: [],
            wbtest_imports: [],
            test_imports: [],
            formatter: MoonPkgFormatter {
                ignore: {},
            },
            link: None,
            warn_list: None,
            alert_list: None,
            targets: None,
            pre_build: None,
            bin_name: None,
            bin_target: None,
            supported_targets: {
                Wasm,
                WasmGC,
                Js,
                Native,
                LLVM,
            },
            native_stub: None,
            virtual_pkg: None,
            implement: None,
            overrides: None,
            max_concurrent_tests: Some(
                1,
            ),
        }"#]]
    .assert_eq(&actual);
}
