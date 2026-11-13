use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use super::*;

const SEPARATE_COMPILATION: [(&str, &str); 4] = [
    ("MOON_SEPARATE_COMPILATION", "1"),
    ("MOON_WASI_LINK", "1"),
    ("MOON_WASM_NEW_ALLOCATOR", "0"),
    ("MOON_COLLECT_REF_CYCLE", "0"),
];

#[derive(serde::Deserialize)]
struct ObjectBuildNode {
    command: Option<String>,
    inputs: Vec<PathBuf>,
    outputs: Vec<PathBuf>,
}

fn object_build_graph(path: &Path) -> Vec<ObjectBuildNode> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn isolated_core() -> (tempfile::TempDir, PathBuf) {
    let toolchain = tempfile::tempdir().unwrap();
    let core = toolchain.path().join("core");
    let installed_core = std::env::var_os("MOON_CORE_OVERRIDE")
        .map(PathBuf::from)
        .unwrap_or_else(|| toolchain_root_for_tests().join("lib/core"));
    copy(&installed_core, &core).unwrap();
    (toolchain, core)
}

fn has_local_library_object(nodes: &[ObjectBuildNode], package: &str) -> bool {
    let suffix = Path::new(package).join(format!("{package}.o"));
    nodes
        .iter()
        .any(|node| node.outputs.iter().any(|output| output.ends_with(&suffix)))
}

#[test]
fn prebuilt_core_objects_are_inputs_and_missing_or_incompatible_objects_are_built() {
    let dir = TestDir::new("wasm_separate_compilation");
    let (toolchain, core) = isolated_core();
    for entry in walkdir::WalkDir::new(&core)
        .into_iter()
        .map(Result::unwrap)
        .collect::<Vec<_>>()
    {
        if entry.file_type().is_dir() && entry.file_name() == "wasm-objects" {
            std::fs::remove_dir_all(entry.path()).unwrap();
        }
    }

    // A partially populated toolchain can reuse builtin while compiling every
    // missing library object into the project's own target directory.
    for profile in ["debug", "release"] {
        let object = core.join(format!(
            "_build/wasm/release/bundle/builtin/wasm-objects/{profile}/builtin.o"
        ));
        std::fs::create_dir_all(object.parent().unwrap()).unwrap();
        std::fs::write(object, b"prebuilt object (planning only)").unwrap();
    }

    for profile in ["debug", "release"] {
        let object = core.join(format!(
            "_build/wasm/release/bundle/builtin/wasm-objects/{profile}/builtin.o"
        ));
        let graph = toolchain.path().join("graph.jsonl");
        moon_cmd(&dir)
            .args([
                "build",
                "--target",
                "wasm",
                "--dry-run",
                &format!("--{profile}"),
            ])
            .envs(SEPARATE_COMPILATION)
            .env("MOON_CORE_OVERRIDE", &core)
            .env("MOON_TEST_DUMP_BUILD_GRAPH", &graph)
            .assert()
            .success();
        let nodes = object_build_graph(&graph);
        assert!(
            nodes.iter().any(|node| {
                node.command
                    .as_ref()
                    .is_some_and(|cmd| cmd.contains("wasm-ld"))
                    && node.inputs.contains(&object)
            }),
            "the shipped object must be a tracked linker input"
        );
        assert!(
            !has_local_library_object(&nodes, "builtin"),
            "reusing builtin must remove its local compilation action"
        );
        assert!(
            has_local_library_object(&nodes, "json"),
            "missing objects must still compile locally"
        );
    }

    // Objects with different runtime settings or symbol policies cannot be
    // substituted, even when the toolchain has that package's default object.
    for (extra_args, env_name, env_value) in [
        (&["--debug"][..], "MOON_WASI_LINK", "0"),
        (&["--debug"][..], "MOON_WASM_NEW_ALLOCATOR", "1"),
        (&["--debug"][..], "MOON_COLLECT_REF_CYCLE", "1"),
        (&["--debug", "--strip"][..], "MOON_WASI_LINK", "1"),
        (&["--release", "--no-strip"][..], "MOON_WASI_LINK", "1"),
    ] {
        let graph = toolchain.path().join("fallback.jsonl");
        moon_cmd(&dir)
            .args(["build", "--target", "wasm", "--dry-run"])
            .args(extra_args)
            .envs(SEPARATE_COMPILATION)
            .env("MOON_CORE_OVERRIDE", &core)
            .env(env_name, env_value)
            .env("MOON_TEST_DUMP_BUILD_GRAPH", &graph)
            .assert()
            .success();
        let nodes = object_build_graph(&graph);
        assert!(
            has_local_library_object(&nodes, "builtin"),
            "incompatible object reused: {extra_args:?} {env_name}={env_value}"
        );
        assert!(!nodes.iter().flat_map(|node| &node.inputs).any(|path| {
            path.components()
                .any(|component| component.as_os_str() == "wasm-objects")
        }));
    }
}

#[test]
fn wasm_bundle_includes_both_object_profiles_from_release_core_ir() {
    let dir = TestDir::new("dummy_core");
    let graph_dir = tempfile::tempdir().unwrap();
    let graph = graph_dir.path().join("graph.jsonl");
    moon_cmd(&dir)
        .args(["bundle", "--target", "wasm", "--dry-run"])
        .envs(SEPARATE_COMPILATION)
        .env("MOON_TEST_DUMP_BUILD_GRAPH", &graph)
        .assert()
        .success();
    let nodes = object_build_graph(&graph);
    let mut core_packages = HashSet::new();
    let mut debug_packages = HashSet::new();
    let mut release_packages = HashSet::new();
    for node in nodes {
        let argv = shlex::split(node.command.as_deref().unwrap()).unwrap();
        if argv.get(1).map(String::as_str) == Some("build-package") {
            core_packages.insert(argv.windows(2).find(|w| w[0] == "-pkg").unwrap()[1].clone());
        } else if argv.get(1).map(String::as_str) == Some("link-core") {
            assert!(argv.windows(2).any(|w| w == ["-pkg-type", "library"]));
            let package = argv.windows(2).find(|w| w[0] == "-main").unwrap()[1].clone();
            let output = argv.windows(2).find(|w| w[0] == "-o").unwrap()[1].replace('\\', "/");
            assert!(output.contains("/wasm/release/bundle/"));
            for input in argv.iter().filter(|arg| arg.ends_with(".core")) {
                assert!(
                    input.replace('\\', "/").contains("/wasm/release/bundle/"),
                    "both object profiles must load the distributed release Core IR"
                );
            }
            if output.contains("/wasm-objects/debug/") {
                assert!(argv.iter().any(|arg| arg == "-g"));
                assert!(argv.iter().any(|arg| arg == "-O0"));
                debug_packages.insert(package);
            } else {
                assert!(output.contains("/wasm-objects/release/"));
                assert!(!argv.iter().any(|arg| arg == "-g" || arg == "-O0"));
                release_packages.insert(package);
            }
        }
    }
    assert!(!core_packages.is_empty());
    assert_eq!(
        debug_packages, core_packages,
        "every bundled package needs a debug object"
    );
    assert_eq!(
        release_packages, core_packages,
        "every bundled package needs a release object"
    );
}

#[test]
fn nondefault_bundles_do_not_publish_default_objects() {
    let dir = TestDir::new("dummy_core");
    let graph_dir = tempfile::tempdir().unwrap();
    let graph = graph_dir.path().join("graph.jsonl");
    for (args, env_name, env_value) in [
        (&["--debug"][..], "MOON_WASI_LINK", "1"),
        (&["--enable-coverage"][..], "MOON_WASI_LINK", "1"),
        (&["--output-wat"][..], "MOON_WASI_LINK", "1"),
        (&[][..], "MOON_WASI_LINK", "0"),
        (&[][..], "MOON_WASM_NEW_ALLOCATOR", "1"),
        (&[][..], "MOON_COLLECT_REF_CYCLE", "1"),
    ] {
        moon_cmd(&dir)
            .args(["bundle", "--target", "wasm", "--dry-run"])
            .args(args)
            .envs(SEPARATE_COMPILATION)
            .env(env_name, env_value)
            .env("MOON_TEST_DUMP_BUILD_GRAPH", &graph)
            .assert()
            .success();
        assert!(
            !object_build_graph(&graph).iter().any(|node| {
                node.command
                    .as_ref()
                    .is_some_and(|command| command.contains("link-core"))
            }),
            "nondefault bundle must not publish objects as the default configuration"
        );
    }
}

#[test]
fn bundled_core_objects_run_and_test_from_cold_builds() {
    // Stable and older nightly toolchains do not yet have the experimental
    // linker runtime flags. Keep their CLI/planning coverage above runnable.
    let help = std::process::Command::new(&*moonutil::toolchain::BINARIES.moonc)
        .args(["wasm-ld", "-help"])
        .output()
        .unwrap();
    if !help.status.success() || !String::from_utf8_lossy(&help.stdout).contains("-wasi") {
        eprintln!("skipping: requires moonc wasm-ld with runtime configuration flags");
        return;
    }
    // Given the real compiler and an isolated copy of its matching core source,
    // package both object profiles using the same command as toolchain packaging.
    let dir = TestDir::new("wasm_separate_compilation");
    let (toolchain, core) = isolated_core();
    let build_dir = core.join("_build");
    if build_dir.exists() {
        std::fs::remove_dir_all(build_dir).unwrap();
    }
    moon_cmd(&core)
        .args(["bundle", "--target", "wasm", "--frozen"])
        .envs(SEPARATE_COMPILATION)
        .assert()
        .success();

    // Distribution relocates the bundle away from its build-time source root.
    let installed_core = toolchain.path().join("installed-core");
    std::fs::rename(core, &installed_core).unwrap();

    // When cold debug and release programs use the newly packaged toolchain,
    // both must run correctly and leave core object compilation out of _build.
    for profile in ["--debug", "--release"] {
        moon_cmd(&dir)
            .args(["run", "main", "--target", "wasm", profile])
            .envs(SEPARATE_COMPILATION)
            .env("MOON_CORE_OVERRIDE", &installed_core)
            .assert()
            .success()
            .stdout_eq("Hello, world!\n");
    }
    // Standalone scripts use the same injected core object selection.
    std::fs::write(
        dir.join("script.mbtx"),
        "fn main { println(\"Hello, script!\") }\n",
    )
    .unwrap();
    moon_cmd(&dir)
        .args(["run", "script.mbtx", "--target", "wasm"])
        .envs(SEPARATE_COMPILATION)
        .env("MOON_CORE_OVERRIDE", &installed_core)
        .assert()
        .success()
        .stdout_eq("Hello, script!\n");
    moon_cmd(&dir)
        .args(["test", "--target", "wasm"])
        .envs(SEPARATE_COMPILATION)
        .env("MOON_CORE_OVERRIDE", &installed_core)
        .assert()
        .success()
        .stdout_eq("Total tests: 1, passed: 1, failed: 0.\n");
    assert!(
        !walkdir::WalkDir::new(dir.join("_build"))
            .into_iter()
            .map(Result::unwrap)
            .any(|entry| entry.path().ends_with(".mooncakes/moonbitlang/core")),
        "run and test must link shipped core objects without rebuilding them"
    );
}

fn is_stdlib_object_build(argv: &[String]) -> bool {
    argv.get(1).is_some_and(|arg| arg == "link-core")
        && argv
            .windows(2)
            .any(|w| w[0] == "-main" && w[1].starts_with("moonbitlang/core/"))
}

fn is_stdlib_object_path(arg: &str) -> bool {
    arg.ends_with(".o")
        && (arg.contains("/moonbitlang/core/") || arg.starts_with("$MOON_HOME/lib/core/"))
}

/// Replace each run of arguments naming installed stdlib `.core` files or
/// stdlib package objects with one placeholder.
fn collapse_stdlib(argv: &[String]) -> String {
    let mut collapsed: Vec<&str> = Vec::new();
    for arg in argv {
        let placeholder = if arg.ends_with(".core") && arg.starts_with("$MOON_HOME/lib/core/") {
            Some("<stdlib cores>")
        } else if is_stdlib_object_path(arg) {
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
        (arg.ends_with(&suffix)
            || (extension == ".o"
                && arg.contains(&format!("/{package}/wasm-objects/"))
                && arg.ends_with(&format!("/{package}.o"))))
            && (arg.starts_with("$MOON_HOME/lib/core/") || arg.contains("/moonbitlang/core/"))
    })
}

/// The `link-core` and `wasm-ld` commands of a dry run with separate
/// compilation. Which standard library packages a target reaches depends on
/// the installed core, so their objects are checked to be built or shipped and linked,
/// then collapsed with the stdlib `.core` inputs.
fn link_commands(dir: &TestDir, args: &[&str]) -> String {
    let out = get_stdout_with_envs(dir, args, SEPARATE_COMPILATION)
        .replace("$MOON_TOOLCHAIN_ROOT", "$MOON_HOME");
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
                    .filter(|arg| is_stdlib_object_path(arg))
                    .collect::<Vec<_>>();
                assert!(
                    names_stdlib_file(argv, "builtin", ".o"),
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
                        stdlib_objects.contains(object)
                            || (object.starts_with("$MOON_HOME/lib/core/")
                                && object.contains("/wasm-objects/")),
                        "linked stdlib object {object} should be built or shipped"
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
