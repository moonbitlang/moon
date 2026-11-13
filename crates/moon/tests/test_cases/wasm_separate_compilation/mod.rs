use std::collections::HashSet;

use super::*;

const SEPARATE_COMPILATION: [(&str, &str); 1] = [("MOON_SEPARATE_COMPILATION", "1")];

fn is_stdlib_object_build(argv: &[String]) -> bool {
    argv.get(1).is_some_and(|arg| arg == "link-core")
        && argv
            .windows(2)
            .any(|w| w[0] == "-main" && w[1].starts_with("moonbitlang/core/"))
}

/// Replace each run of arguments naming installed stdlib `.core` files or
/// stdlib package objects with one placeholder.
fn collapse_stdlib(argv: &[String]) -> String {
    let mut collapsed: Vec<&str> = Vec::new();
    for arg in argv {
        let placeholder = if arg.ends_with(".core") && arg.starts_with("$MOON_HOME/lib/core/") {
            Some("<stdlib cores>")
        } else if arg.ends_with(".o") && arg.contains("/moonbitlang/core/") {
            Some("<stdlib objects>")
        } else {
            None
        };
        match placeholder {
            Some(placeholder) if collapsed.last() == Some(&placeholder) => {}
            Some(placeholder) => collapsed.push(placeholder),
            None => collapsed.push(arg),
        }
    }
    shlex::try_join(collapsed).unwrap()
}

/// The packages every object reads and every executable links, so that
/// `moonc` can compile the error table.
const ERROR_TABLE_PACKAGES: [&str; 2] = ["json", "debug"];

/// Whether `argv` names the installed `.core` file or the object of the
/// standard library package `package`.
fn names_stdlib_file(argv: &[String], package: &str, extension: &str) -> bool {
    let suffix = format!("/{package}/{package}{extension}");
    argv.iter().any(|arg| {
        arg.ends_with(&suffix)
            && (arg.starts_with("$MOON_HOME/lib/core/") || arg.contains("/moonbitlang/core/"))
    })
}

/// The `link-core` and `wasm-ld` commands of a dry run with separate
/// compilation. Which standard library packages a target reaches depends on
/// the installed core, so their objects are checked to be built and linked,
/// then collapsed with the stdlib `.core` inputs.
fn link_commands(dir: &TestDir, args: &[&str]) -> String {
    let out = get_stdout_with_envs(dir, args, SEPARATE_COMPILATION);
    let commands = out
        .lines()
        .map(|line| shlex::split(line).expect("valid command arguments"))
        .collect::<Vec<_>>();

    let stdlib_objects = commands
        .iter()
        .filter(|argv| is_stdlib_object_build(argv))
        .map(|argv| {
            let output = argv.iter().skip_while(|arg| *arg != "-o").nth(1);
            output.expect("object output").clone()
        })
        .collect::<HashSet<_>>();

    let mut rendered = Vec::new();
    for argv in &commands {
        match argv.get(1).map(String::as_str) {
            Some("link-core") if is_stdlib_object_build(argv) => {
                for package in ERROR_TABLE_PACKAGES {
                    assert!(
                        names_stdlib_file(argv, package, ".core"),
                        "every stdlib object reads {package}: {argv:?}"
                    );
                }
            }
            Some("link-core") => rendered.push(collapse_stdlib(argv)),
            Some("wasm-ld") => {
                let linked_stdlib = argv
                    .iter()
                    .filter(|arg| arg.ends_with(".o") && arg.contains("/moonbitlang/core/"))
                    .collect::<Vec<_>>();
                assert!(
                    linked_stdlib
                        .iter()
                        .any(|arg| arg.ends_with("/moonbitlang/core/builtin/builtin.o")),
                    "every program links builtin: {argv:?}"
                );
                for package in ERROR_TABLE_PACKAGES {
                    assert!(
                        names_stdlib_file(argv, package, ".o"),
                        "every program links {package}: {argv:?}"
                    );
                }
                for object in linked_stdlib {
                    assert!(
                        stdlib_objects.contains(object),
                        "linked stdlib object {object} should be built"
                    );
                }
                rendered.push(collapse_stdlib(argv));
            }
            _ => {}
        }
    }
    rendered.join("\n") + "\n"
}

#[test]
fn test_wasm_separate_compilation_build() {
    let dir = TestDir::new("wasm_separate_compilation");

    // `main` links its object with the library objects; `exported` has link
    // settings the objects cannot carry, so it keeps the whole-program link.
    check(
        link_commands(
            &dir,
            &["build", "--target", "wasm", "--dry-run", "--sort-input"],
        ),
        expect![[r#"
            moonc link-core '<stdlib cores>' ./_build/wasm/debug/build/lib/lib.core ./_build/wasm/debug/build/main/main.core -main username/sep/main -o ./_build/wasm/debug/build/main/__moonbit_link_core__/main.o -pkg-config-path ./main/moon.pkg -pkg-sources username/sep/lib:./lib -pkg-sources username/sep/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm -g -O0 -wasi
            moonc link-core '<stdlib cores>' ./_build/wasm/debug/build/lib/lib.core -main username/sep/lib -pkg-type library -o ./_build/wasm/debug/build/lib/lib.o -pkg-sources username/sep/lib:./lib -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm -g -O0 -wasi
            moonc wasm-ld '<stdlib objects>' ./_build/wasm/debug/build/lib/lib.o ./_build/wasm/debug/build/main/__moonbit_link_core__/main.o -o ./_build/wasm/debug/build/main/main.wasm -g -wasi
            moonc link-core '<stdlib cores>' ./_build/wasm/debug/build/lib/lib.core ./_build/wasm/debug/build/exported/exported.core -main username/sep/exported -o ./_build/wasm/debug/build/exported/exported.wasm -pkg-config-path ./exported/moon.pkg -pkg-sources username/sep/lib:./lib -pkg-sources username/sep/exported:./exported -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm -g -O0 -wasi -export-memory-name memory
        "#]],
    );

    // Text output and other backends keep the whole-program link.
    for args in [
        &["build", "--target", "wasm", "--output-wat", "--dry-run"][..],
        &["build", "--target", "wasm-gc", "--dry-run"],
    ] {
        let out = get_stdout_with_envs(&dir, args, SEPARATE_COMPILATION);
        assert!(!out.contains("wasm-ld"), "{args:?}: {out}");
        assert!(!out.contains("-pkg-type library -o"), "{args:?}: {out}");
    }
}

#[test]
fn test_wasm_separate_compilation_test() {
    let dir = TestDir::new("wasm_separate_compilation");

    // Test drivers link their objects with the library objects, except the
    // blackbox tests of main packages, which would need a main package's
    // library object, and the tests of `exported`.
    check(
        link_commands(
            &dir,
            &["test", "--target", "wasm", "--dry-run", "--sort-input"],
        ),
        expect![[r#"
            moonc link-core '<stdlib cores>' ./_build/wasm/debug/test/lib/lib.core ./_build/wasm/debug/test/main/main.internal_test.core -main username/sep/main -o ./_build/wasm/debug/test/main/__moonbit_link_core__/main.internal_test.o -test-mode -pkg-config-path ./main/moon.pkg -pkg-sources username/sep/lib:./lib -pkg-sources username/sep/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm -g -O0 -wasi
            moonc link-core '<stdlib cores>' ./_build/wasm/debug/test/lib/lib.core -main username/sep/lib -pkg-type library -o ./_build/wasm/debug/test/lib/lib.o -pkg-sources username/sep/lib:./lib -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm -g -O0 -wasi
            moonc wasm-ld '<stdlib objects>' ./_build/wasm/debug/test/lib/lib.o ./_build/wasm/debug/test/main/__moonbit_link_core__/main.internal_test.o -o ./_build/wasm/debug/test/main/main.internal_test.wasm -g -wasi
            moonc link-core '<stdlib cores>' ./_build/wasm/debug/test/lib/lib.core ./_build/wasm/debug/test/main/main.core ./_build/wasm/debug/test/main/main.blackbox_test.core -main username/sep/main_blackbox_test -o ./_build/wasm/debug/test/main/main.blackbox_test.wasm -test-mode -pkg-config-path ./main/moon.pkg -pkg-sources username/sep/lib:./lib -pkg-sources username/sep/main:./main -pkg-sources username/sep/main_blackbox_test:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm -g -O0 -wasi
            moonc link-core '<stdlib cores>' ./_build/wasm/debug/test/lib/lib.whitebox_test.core -main username/sep/lib -o ./_build/wasm/debug/test/lib/__moonbit_link_core__/lib.whitebox_test.o -test-mode -pkg-config-path ./lib/moon.pkg -pkg-sources username/sep/lib:./lib -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm -g -O0 -wasi
            moonc wasm-ld '<stdlib objects>' ./_build/wasm/debug/test/lib/__moonbit_link_core__/lib.whitebox_test.o -o ./_build/wasm/debug/test/lib/lib.whitebox_test.wasm -g -wasi
            moonc link-core '<stdlib cores>' ./_build/wasm/debug/test/lib/lib.internal_test.core -main username/sep/lib -o ./_build/wasm/debug/test/lib/__moonbit_link_core__/lib.internal_test.o -test-mode -pkg-config-path ./lib/moon.pkg -pkg-sources username/sep/lib:./lib -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm -g -O0 -wasi
            moonc wasm-ld '<stdlib objects>' ./_build/wasm/debug/test/lib/__moonbit_link_core__/lib.internal_test.o -o ./_build/wasm/debug/test/lib/lib.internal_test.wasm -g -wasi
            moonc link-core '<stdlib cores>' ./_build/wasm/debug/test/lib/lib.core ./_build/wasm/debug/test/lib/lib.blackbox_test.core -main username/sep/lib_blackbox_test -o ./_build/wasm/debug/test/lib/__moonbit_link_core__/lib.blackbox_test.o -test-mode -pkg-config-path ./lib/moon.pkg -pkg-sources username/sep/lib:./lib -pkg-sources username/sep/lib_blackbox_test:./lib -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm -g -O0 -wasi
            moonc wasm-ld '<stdlib objects>' ./_build/wasm/debug/test/lib/lib.o ./_build/wasm/debug/test/lib/__moonbit_link_core__/lib.blackbox_test.o -o ./_build/wasm/debug/test/lib/lib.blackbox_test.wasm -g -wasi
            moonc link-core '<stdlib cores>' ./_build/wasm/debug/test/lib/lib.core ./_build/wasm/debug/test/exported/exported.internal_test.core -main username/sep/exported -o ./_build/wasm/debug/test/exported/exported.internal_test.wasm -test-mode -pkg-config-path ./exported/moon.pkg -pkg-sources username/sep/lib:./lib -pkg-sources username/sep/exported:./exported -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm -g -O0 -wasi -export-memory-name memory
            moonc link-core '<stdlib cores>' ./_build/wasm/debug/test/lib/lib.core ./_build/wasm/debug/test/exported/exported.core ./_build/wasm/debug/test/exported/exported.blackbox_test.core -main username/sep/exported_blackbox_test -o ./_build/wasm/debug/test/exported/exported.blackbox_test.wasm -test-mode -pkg-config-path ./exported/moon.pkg -pkg-sources username/sep/lib:./lib -pkg-sources username/sep/exported:./exported -pkg-sources username/sep/exported_blackbox_test:./exported -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm -g -O0 -wasi -export-memory-name memory
        "#]],
    );
}

#[test]
fn test_wasm_separate_compilation_main_dependent() {
    let dir = TestDir::new("wasm_separate_compilation_main_dependent.in");

    // `alpha` depends on `zbase`, and `zbase`'s whitebox test imports `alpha`.
    // That test's main object compiles `zbase`, which `alpha.o` could not
    // refer to, so it keeps the whole-program link. `zbase`'s internal test
    // does not import `alpha` and links from objects.
    check(
        link_commands(
            &dir,
            &["test", "--target", "wasm", "--dry-run", "--sort-input"],
        ),
        expect![[r#"
            moonc link-core '<stdlib cores>' ./_build/wasm/debug/test/zbase/zbase.whitebox_test.core ./_build/wasm/debug/test/alpha/alpha.core -main username/dep/zbase -o ./_build/wasm/debug/test/zbase/zbase.whitebox_test.wasm -test-mode -pkg-config-path ./zbase/moon.pkg -pkg-sources username/dep/zbase:./zbase -pkg-sources username/dep/alpha:./alpha -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm -g -O0 -wasi
            moonc link-core '<stdlib cores>' ./_build/wasm/debug/test/zbase/zbase.internal_test.core -main username/dep/zbase -o ./_build/wasm/debug/test/zbase/__moonbit_link_core__/zbase.internal_test.o -test-mode -pkg-config-path ./zbase/moon.pkg -pkg-sources username/dep/zbase:./zbase -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm -g -O0 -wasi
            moonc wasm-ld '<stdlib objects>' ./_build/wasm/debug/test/zbase/__moonbit_link_core__/zbase.internal_test.o -o ./_build/wasm/debug/test/zbase/zbase.internal_test.wasm -g -wasi
            moonc link-core '<stdlib cores>' ./_build/wasm/debug/test/zbase/zbase.core -main username/dep/zbase -pkg-type library -o ./_build/wasm/debug/test/zbase/zbase.o -pkg-sources username/dep/zbase:./zbase -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm -g -O0 -wasi
            moonc link-core '<stdlib cores>' ./_build/wasm/debug/test/zbase/zbase.core ./_build/wasm/debug/test/zbase/zbase.blackbox_test.core -main username/dep/zbase_blackbox_test -o ./_build/wasm/debug/test/zbase/__moonbit_link_core__/zbase.blackbox_test.o -test-mode -pkg-config-path ./zbase/moon.pkg -pkg-sources username/dep/zbase:./zbase -pkg-sources username/dep/zbase_blackbox_test:./zbase -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm -g -O0 -wasi
            moonc wasm-ld '<stdlib objects>' ./_build/wasm/debug/test/zbase/zbase.o ./_build/wasm/debug/test/zbase/__moonbit_link_core__/zbase.blackbox_test.o -o ./_build/wasm/debug/test/zbase/zbase.blackbox_test.wasm -g -wasi
            moonc link-core '<stdlib cores>' ./_build/wasm/debug/test/zbase/zbase.core ./_build/wasm/debug/test/alpha/alpha.internal_test.core -main username/dep/alpha -o ./_build/wasm/debug/test/alpha/__moonbit_link_core__/alpha.internal_test.o -test-mode -pkg-config-path ./alpha/moon.pkg -pkg-sources username/dep/zbase:./zbase -pkg-sources username/dep/alpha:./alpha -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm -g -O0 -wasi
            moonc wasm-ld '<stdlib objects>' ./_build/wasm/debug/test/zbase/zbase.o ./_build/wasm/debug/test/alpha/__moonbit_link_core__/alpha.internal_test.o -o ./_build/wasm/debug/test/alpha/alpha.internal_test.wasm -g -wasi
            moonc link-core '<stdlib cores>' ./_build/wasm/debug/test/zbase/zbase.core ./_build/wasm/debug/test/alpha/alpha.core -main username/dep/alpha -pkg-type library -o ./_build/wasm/debug/test/alpha/alpha.o -pkg-sources username/dep/zbase:./zbase -pkg-sources username/dep/alpha:./alpha -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm -g -O0 -wasi
            moonc link-core '<stdlib cores>' ./_build/wasm/debug/test/zbase/zbase.core ./_build/wasm/debug/test/alpha/alpha.core ./_build/wasm/debug/test/alpha/alpha.blackbox_test.core -main username/dep/alpha_blackbox_test -o ./_build/wasm/debug/test/alpha/__moonbit_link_core__/alpha.blackbox_test.o -test-mode -pkg-config-path ./alpha/moon.pkg -pkg-sources username/dep/zbase:./zbase -pkg-sources username/dep/alpha:./alpha -pkg-sources username/dep/alpha_blackbox_test:./alpha -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm -g -O0 -wasi
            moonc wasm-ld '<stdlib objects>' ./_build/wasm/debug/test/zbase/zbase.o ./_build/wasm/debug/test/alpha/alpha.o ./_build/wasm/debug/test/alpha/__moonbit_link_core__/alpha.blackbox_test.o -o ./_build/wasm/debug/test/alpha/alpha.blackbox_test.wasm -g -wasi
        "#]],
    );
}

#[test]
fn test_wasm_separate_compilation_library_override() {
    let dir = TestDir::new("wasm_separate_compilation_library_override.in");

    // `lib` overrides the virtual `vlib` with `impl`, but `main` declares no
    // overrides, so its whole-program link uses `vlib`'s default. A shared
    // `lib.o` cannot follow each executable's resolution of `vlib`, so `main`,
    // which reaches a virtual package, keeps the whole-program link.
    check(
        link_commands(
            &dir,
            &["build", "--target", "wasm", "--dry-run", "--sort-input"],
        ),
        expect![[r#"
            moonc link-core '<stdlib cores>' ./_build/wasm/debug/build/vlib/vlib.core ./_build/wasm/debug/build/lib/lib.core ./_build/wasm/debug/build/main/main.core -main username/vo/main -o ./_build/wasm/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg -pkg-sources username/vo/vlib:./vlib -pkg-sources username/vo/lib:./lib -pkg-sources username/vo/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm -g -O0 -wasi
        "#]],
    );
}
