use super::*;

const MODULE_NAME_FLAG: &str = "-wasm-module-name";
const VERSIONED_PACKAGE_NAME: &str = "example/wasm-module-name/app@0.0.0";
const UNVERSIONED_PACKAGE_NAME: &str = "example/wasm-module-name/app";

fn linked_wasm_module_names(output: &str) -> Vec<&str> {
    output
        .lines()
        .filter(|line| line.contains("moonc link-core"))
        .filter_map(|line| {
            line.split_once(MODULE_NAME_FLAG)
                .and_then(|(_, value)| value.split_whitespace().next())
        })
        .collect()
}

#[test]
fn release_binary_wasm_builds_name_the_linked_package() {
    let dir = TestDir::new("wasm_module_name/versioned");

    for target in ["wasm", "wasm-gc"] {
        let output = get_stdout(
            &dir,
            [
                "build",
                "--release",
                "--target",
                target,
                "--dry-run",
                "--nostd",
                "--sort-input",
            ],
        );
        assert_eq!(
            linked_wasm_module_names(&output),
            [VERSIONED_PACKAGE_NAME],
            "unexpected module names for {target}:\n{output}"
        );
    }
}

#[test]
fn release_binary_wasm_build_omits_an_undeclared_version() {
    let dir = TestDir::new("wasm_module_name/unversioned");
    let output = get_stdout(
        &dir,
        [
            "build",
            "--release",
            "--target",
            "wasm",
            "--dry-run",
            "--nostd",
            "--sort-input",
        ],
    );

    assert_eq!(
        linked_wasm_module_names(&output),
        [UNVERSIONED_PACKAGE_NAME]
    );
}

#[test]
fn non_release_or_non_binary_builds_do_not_set_a_wasm_module_name() {
    let dir = TestDir::new("wasm_module_name/versioned");
    let cases: &[(&[&str], &str)] = &[
        (
            &["build", "--target", "wasm", "--dry-run", "--nostd"],
            "debug Wasm",
        ),
        (
            &["build", "--target", "wasm-gc", "--dry-run", "--nostd"],
            "debug WasmGC",
        ),
        (
            &[
                "build",
                "--release",
                "--target",
                "wasm",
                "--output-wat",
                "--dry-run",
                "--nostd",
            ],
            "Wasm WAT",
        ),
        (
            &[
                "build",
                "--release",
                "--target",
                "wasm-gc",
                "--output-wat",
                "--dry-run",
                "--nostd",
            ],
            "WasmGC WAT",
        ),
        (
            &[
                "build",
                "--release",
                "--target",
                "js",
                "--dry-run",
                "--nostd",
            ],
            "JavaScript",
        ),
        (
            &[
                "build",
                "--release",
                "--target",
                "native",
                "--dry-run",
                "--nostd",
            ],
            "Native",
        ),
        (
            &[
                "build",
                "--release",
                "--target",
                "llvm",
                "--dry-run",
                "--nostd",
            ],
            "LLVM",
        ),
    ];

    for (args, description) in cases {
        let output = get_stdout(&dir, args.iter().copied());
        assert!(
            !output.contains(MODULE_NAME_FLAG),
            "{description} unexpectedly set a Wasm module name:\n{output}"
        );
    }
}

#[test]
fn release_wasm_tests_use_the_source_package_name() {
    let dir = TestDir::new("wasm_module_name/versioned");
    let output = get_stdout(
        &dir,
        [
            "test",
            "--release",
            "--target",
            "wasm-gc",
            "--dry-run",
            "--nostd",
            "--sort-input",
        ],
    );
    let mut module_names = linked_wasm_module_names(&output);
    module_names.sort_unstable();

    assert_eq!(
        module_names,
        [VERSIONED_PACKAGE_NAME, VERSIONED_PACKAGE_NAME]
    );
}
