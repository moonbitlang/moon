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
            kind: MoonPkg,
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
            supported_targets: {},
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
      is_main = true
    "#);
    expect_test::expect![[r#"
        MoonPkg {
            kind: MoonPkg,
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
            supported_targets: {},
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
      formatter(ignore=["file1.mbt", "file2.mbt", "file3.mbt"])
    "#);
    expect_test::expect![[r#"
        MoonPkg {
            kind: MoonPkg,
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
            supported_targets: {},
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
      supported_targets([wasm, js, native, wasm_gc])
    "#);
    expect_test::expect![[r#"
        MoonPkg {
            kind: MoonPkg,
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
                Js,
                WasmGC,
                Wasm,
                Native,
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
    build(
      command="exe1 $input -o $output",
      input="source1.mbt",
      output="output1.wasm"
    )
    build(
      command="exe2 $input -o $output",
      input="source2.mbt",
      output="output2.wasm"
    )
    "#);
    expect_test::expect![[r#"
        MoonPkg {
            kind: MoonPkg,
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
                        input: Array(
                            [
                                "source1.mbt",
                            ],
                        ),
                        output: Array(
                            [
                                "output1.wasm",
                            ],
                        ),
                        command: "exe1 $input -o $output",
                    },
                    MoonPkgGenerate {
                        input: Array(
                            [
                                "source2.mbt",
                            ],
                        ),
                        output: Array(
                            [
                                "output2.wasm",
                            ],
                        ),
                        command: "exe2 $input -o $output",
                    },
                ],
            ),
            bin_name: None,
            bin_target: None,
            supported_targets: {},
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
      warnings(
        off=[fragile_match, deprecated_syntax],
        on=[unused_variable, todo],
        as_error=[unused_variable, todo],
      ) 
    "#);
    expect_test::expect![[r#"
        MoonPkg {
            kind: MoonPkg,
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
            warn_list: Some(
                "-fragile_match-deprecated_syntax+unused_variable+todo",
            ),
            alert_list: None,
            targets: None,
            pre_build: None,
            bin_name: None,
            bin_target: None,
            supported_targets: {},
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
    options({
      "virtual_pkg": { "has_default": false },
      "implement": "string",
      "overrides": [ "string1", "string2" ],
      "bin_name": "name",
      "bin_target": wasm,
      "native_stub": [ "stub.c", "another_stub.c" ],
      "max_concurrent_tests": 4,
      "link": {
        "wasm": {
          "exports": [f1, f2], 
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
          "exports": [f1, f2],
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
          "exports": [f1, f2],
          "format": esm, 
        },
        "native": {
          "exports": [f1, f2],
          "cc": "gcc",
          "cflags": ["-flag1", "-flag2"],
          "cc_link_flags": "-flag1",
          "stub_cc": "gcc",
          "stub_cflags": "-flag2",
          "stub_cc_link_flags": "-flag1",
          "stub_lib_deps": ["lib1", "lib2"],
        },
        "targets": {
          "file.mbt": js,
          "platform_specific.mbt": or(and(native, release, not(wasm), not(debug)), llvm)
        },
      },
    })
    "#);
    expect_test::expect![[r#"
        MoonPkg {
            kind: MoonPkg,
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
                            heap_start_address: Some(
                                100,
                            ),
                            import_memory: Some(
                                ImportMemory {
                                    module: "string",
                                    name: "string",
                                },
                            ),
                            memory_limits: Some(
                                MemoryLimits {
                                    min: 1,
                                    max: 10,
                                },
                            ),
                            shared_memory: Some(
                                true,
                            ),
                            export_memory_name: Some(
                                "name",
                            ),
                            flags: Some(
                                [
                                    "--flag1",
                                    "--flag2",
                                ],
                            ),
                        },
                    ),
                    wasm_gc: Some(
                        WasmGcLinkConfig {
                            exports: Some(
                                [
                                    "f1",
                                    "f2",
                                ],
                            ),
                            import_memory: Some(
                                ImportMemory {
                                    module: "string",
                                    name: "string",
                                },
                            ),
                            memory_limits: Some(
                                MemoryLimits {
                                    min: 1,
                                    max: 10,
                                },
                            ),
                            shared_memory: Some(
                                true,
                            ),
                            export_memory_name: Some(
                                "name",
                            ),
                            flags: Some(
                                [
                                    "--flag1",
                                    "--flag2",
                                ],
                            ),
                            use_js_builtin_string: Some(
                                true,
                            ),
                            imported_string_constants: Some(
                                "string",
                            ),
                        },
                    ),
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
                            cc_link_flags: Some(
                                "-flag1",
                            ),
                            stub_cc: Some(
                                "gcc",
                            ),
                            stub_cc_flags: None,
                            stub_cc_link_flags: Some(
                                "-flag1",
                            ),
                            stub_lib_deps: None,
                        },
                    ),
                },
            ),
            warn_list: None,
            alert_list: None,
            targets: None,
            pre_build: None,
            bin_name: Some(
                "name",
            ),
            bin_target: Some(
                Wasm,
            ),
            supported_targets: {},
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
fn expect_empty_array() {
    let actual = run(r#"
      f(
        label1=[],
        label2={},
      ) 
    "#);
    expect_test::expect![[r#"
        MoonPkg {
            kind: MoonPkg,
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
            supported_targets: {},
            native_stub: None,
            virtual_pkg: None,
            implement: None,
            overrides: None,
            max_concurrent_tests: None,
        }"#]]
    .assert_eq(&actual);
}

#[test]
fn expect_max_concurrent_tests() {
    let actual = run(r#"
      options({
        "max_concurrent_tests": 1,
      }) 
    "#);
    expect_test::expect![[r#"
        MoonPkg {
            kind: MoonPkg,
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
            supported_targets: {},
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
