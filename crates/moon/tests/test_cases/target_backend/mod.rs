use super::*;

const PREFERRED_TARGET_CONFLICT_WARNING: &str = "Multiple local modules specify different preferred targets; pass `--target` to choose one explicitly";

#[test]
fn test_target_backend_cli_wiring_smoke() {
    let dir = TestDir::new("target_backend");
    check(
        get_stdout(&dir, ["build", "--dry-run", "--nostd"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/debug/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/debug/build/main/main.core -pkg hello/main -is-main -i ./_build/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc link-core ./_build/wasm-gc/debug/build/lib/lib.core ./_build/wasm-gc/debug/build/main/main.core -main hello/main -o ./_build/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map
        "#]],
    );
    check(
        get_stdout(
            &dir,
            ["run", "main", "--dry-run", "--target", "js", "--nostd"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/js/debug/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target js -g -O0 -source-map -workspace-path . -all-pkgs ./_build/js/debug/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/js/debug/build/main/main.core -pkg hello/main -is-main -i ./_build/js/debug/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target js -g -O0 -source-map -workspace-path . -all-pkgs ./_build/js/debug/build/all_pkgs.json
            moonc link-core ./_build/js/debug/build/lib/lib.core ./_build/js/debug/build/main/main.core -main hello/main -o ./_build/js/debug/build/main/main.js -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target js -g -O0 -source-map
            node --enable-source-maps ./_build/js/debug/build/main/main.js
        "#]],
    );
}

fn assert_contains_and_absent(output: &str, present: &[&str], absent: &[&str]) {
    for needle in present {
        assert!(
            output.contains(needle),
            "expected output to contain `{needle}`, got:\n{output}"
        );
    }
    for needle in absent {
        assert!(
            !output.contains(needle),
            "expected output to not contain `{needle}`, got:\n{output}"
        );
    }
}

fn assert_preferred_target_conflict_warning(stderr: &str) {
    assert!(
        stderr.contains(PREFERRED_TARGET_CONFLICT_WARNING),
        "stderr: {stderr}"
    );
}

fn assert_conflicting_workspace_preferred_targets_default_to_wasm_gc(
    command: &str,
    stdout: &str,
    stderr: &str,
) {
    assert_preferred_target_conflict_warning(stderr);
    assert_contains_and_absent(
        stdout,
        &[
            "./_build/wasm-gc/",
            "-target wasm-gc",
            "./js_preferred/src/lib/extra.wasm-gc.mbt",
            "./native_preferred/src/lib/extra.wasm-gc.mbt",
        ],
        &[
            "./js_preferred/src/lib/extra.js.mbt",
            "./native_preferred/src/lib/extra.native.mbt",
        ],
    );
    assert!(
        stdout.contains("./js_preferred/src/lib/lib.mbt"),
        "{command} output did not include js_preferred sources:\n{stdout}"
    );
    assert!(
        stdout.contains("./native_preferred/src/lib/lib.mbt"),
        "{command} output did not include native_preferred sources:\n{stdout}"
    );
}

#[test]
fn test_mixed_backend_explicit_selection_rejects_unsupported_backend() {
    let dir = TestDir::new("mixed_backend_local_dep.in");

    let check_err = get_err_stderr(&dir, ["check", "server", "--target", "js", "--dry-run"]);
    assert!(
        check_err.contains("Package 'mixed/localdep/server' does not support target backend 'js'")
    );
    assert!(check_err.contains("Supported backends: [native]"));
}

#[test]
fn test_mixed_backend_explicit_multi_path_selection_filters_unsupported_packages() {
    let dir = TestDir::new("mixed_backend_local_dep.in");

    let check_out = get_stdout(
        &dir,
        [
            "check",
            "web",
            "server",
            "--target",
            "js",
            "--dry-run",
            "--sort-input",
        ],
    );
    assert_contains_and_absent(
        &check_out,
        &[
            "./shared/shared.mbt",
            "./web/main.mbt",
            "./deps/jsdep/lib/lib.mbt",
        ],
        &["./server/main.mbt", "./deps/nativedep/lib/lib.mbt"],
    );

    let build_out = get_stdout(
        &dir,
        [
            "build",
            "web",
            "server",
            "--target",
            "js",
            "--dry-run",
            "--sort-input",
        ],
    );
    assert_contains_and_absent(
        &build_out,
        &[
            "./shared/shared.mbt",
            "./web/main.mbt",
            "./deps/jsdep/lib/lib.mbt",
        ],
        &["./server/main.mbt", "./deps/nativedep/lib/lib.mbt"],
    );
}

#[test]
fn test_mixed_backend_explicit_multi_path_selection_warns_only_in_verbose_mode() {
    let dir = TestDir::new("mixed_backend_local_dep.in");

    let stderr = get_stderr(
        &dir,
        [
            "check",
            "web",
            "server",
            "--target",
            "js",
            "--dry-run",
            "--verbose",
        ],
    );
    assert!(
        stderr.contains("skipping path `server`"),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("mixed/localdep/server"), "stderr: {stderr}");

    let stderr = get_stderr(
        &dir,
        ["check", "web", "server", "--target", "js", "--dry-run"],
    );
    assert!(
        !stderr.contains("skipping path `server`"),
        "stderr: {stderr}"
    );
}

#[test]
fn test_mixed_backend_run_info_bundle_are_target_aware() {
    let dir = TestDir::new("mixed_backend_local_dep.in");

    let info_js = get_stdout(&dir, ["info", "--target", "js"]);
    assert!(!dir.join("shared").join(MBTI_GENERATED).exists());
    assert!(!dir.join("web").join(MBTI_GENERATED).exists());
    assert!(!dir.join("server").join(MBTI_GENERATED).exists());
    assert!(info_js.contains("Package mixed/localdep/shared has no canonical interface"));
    assert!(info_js.contains("pub fn shared_banner() -> String"));
    assert!(info_js.contains("pub fn web_banner() -> String"));

    let info_native = get_stdout(&dir, ["info", "--target", "native"]);
    assert!(!dir.join("shared").join(MBTI_GENERATED).exists());
    assert!(!dir.join("web").join(MBTI_GENERATED).exists());
    assert!(!dir.join("server").join(MBTI_GENERATED).exists());
    assert!(info_native.contains("Package mixed/localdep/shared has no canonical interface"));
    assert!(info_native.contains("pub fn shared_banner() -> String"));
    assert!(info_native.contains("pub fn server_banner() -> String"));

    let info_both = get_stdout(&dir, ["info", "--target", "js,native"]);
    assert!(!dir.join("shared").join(MBTI_GENERATED).exists());
    assert!(!dir.join("web").join(MBTI_GENERATED).exists());
    assert!(!dir.join("server").join(MBTI_GENERATED).exists());
    assert!(info_both.contains("Package mixed/localdep/shared has no canonical interface"));
    assert!(info_both.contains("pub fn shared_banner() -> String"));
    assert!(info_both.contains("pub fn web_banner() -> String"));
    assert!(info_both.contains("pub fn server_banner() -> String"));

    let bundle_js = get_stdout(
        &dir,
        ["bundle", "--target", "js", "--dry-run", "--sort-input"],
    );
    assert_contains_and_absent(
        &bundle_js,
        &[
            "./shared/shared.mbt",
            "./web/main.mbt",
            "./deps/jsdep/lib/lib.mbt",
        ],
        &[
            "./server/main.mbt",
            "./deps/nativedep/lib/lib.mbt",
            "./deps/unuseddep/lib/lib.mbt",
        ],
    );

    let bundle_native = get_stdout(
        &dir,
        ["bundle", "--target", "native", "--dry-run", "--sort-input"],
    );
    assert_contains_and_absent(
        &bundle_native,
        &[
            "./shared/shared.mbt",
            "./server/main.mbt",
            "./deps/nativedep/lib/lib.mbt",
        ],
        &[
            "./web/main.mbt",
            "./deps/jsdep/lib/lib.mbt",
            "./deps/unuseddep/lib/lib.mbt",
        ],
    );
}

#[test]
fn test_supported_targets_empty_list_is_never_selected() {
    let dir = TestDir::new("supported_targets_empty.in");

    let explicit_err = get_err_stderr(&dir, ["check", "never", "--target", "js", "--dry-run"]);
    assert!(
        explicit_err
            .contains("Package 'supported/empty/never' does not support target backend 'js'")
    );
    assert!(explicit_err.contains("Supported backends: []"));
}

#[test]
fn test_module_supported_targets_intersects_package_supported_targets() {
    let dir = TestDir::new("supported_targets_module_intersection.in");

    let js_err = get_err_stderr(&dir, ["check", "lib", "--target", "js", "--dry-run"]);
    assert!(
        js_err.contains(
            "Package 'supported/mod-intersection/lib' does not support target backend 'js'"
        )
    );

    let wasm_err = get_err_stderr(&dir, ["check", "lib", "--target", "wasm", "--dry-run"]);
    assert!(wasm_err.contains(
        "Package 'supported/mod-intersection/lib' does not support target backend 'wasm'"
    ));
}

#[test]
fn test_packages_json_contains_computed_supported_targets() {
    let dir = TestDir::new("supported_targets_module_intersection.in");

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["check", "--sort-input"])
        .assert()
        .success();

    let packages_json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("_build/packages.json")).unwrap())
            .unwrap();
    let packages = packages_json["packages"].as_array().unwrap();

    let lib = packages.iter().find(|pkg| pkg["rel"] == "lib").unwrap();
    assert_eq!(
        lib["supported-targets"],
        serde_json::json!(["WasmGC", "Native", "LLVM"])
    );

    let main = packages.iter().find(|pkg| pkg["rel"] == "main").unwrap();
    assert_eq!(
        main["supported-targets"],
        serde_json::json!(["Wasm", "WasmGC", "Native", "LLVM"])
    );
}

#[test]
fn test_supported_targets_transitive_mismatch_fails_fast() {
    let dir = TestDir::new("supported_targets_transitive_mismatch.in");

    let check_err = get_err_stderr(&dir, ["check", "--target", "js", "--dry-run"]);
    assert!(check_err.contains("Failed to calculate build plan"));
    assert!(check_err.contains("failed to run check for target"));
    assert!(check_err.contains("incompatible with the dependency graph"));
    assert!(check_err.contains("'supported/mismatch/main' requires 'supported/mismatch/lib'"));
    assert!(check_err.contains("requires 'supported/mismatch/lib'"));
    assert!(check_err.contains("supports [native]"));

    let build_err = get_err_stderr(&dir, ["build", "--target", "js", "--dry-run"]);
    assert!(build_err.contains("incompatible with the dependency graph"));
    assert!(build_err.contains("'supported/mismatch/main' requires 'supported/mismatch/lib'"));
    assert!(build_err.contains("requires 'supported/mismatch/lib'"));
    assert!(build_err.contains("supports [native]"));

    let run_err = get_err_stderr(&dir, ["run", "main", "--target", "js", "--dry-run"]);
    assert!(run_err.contains("incompatible with the dependency graph"));
    assert!(run_err.contains("'supported/mismatch/main' requires 'supported/mismatch/lib'"));
    assert!(run_err.contains("requires 'supported/mismatch/lib'"));
    assert!(run_err.contains("supports [native]"));
}

#[test]
fn test_supported_targets_virtual_root_dep_mismatch_fails_fast() {
    let dir = TestDir::new("supported_targets_virtual_root_mismatch.in");

    let check_err = get_err_stderr(&dir, ["check", "virtual", "--target", "js", "--dry-run"]);
    assert!(check_err.contains("incompatible with the dependency graph"));
    assert!(
        check_err
            .contains("'supported/virtual-root/virtual' requires 'supported/virtual-root/lib'")
    );
    assert!(check_err.contains("supports [native]"));

    let build_err = get_err_stderr(&dir, ["build", "virtual", "--target", "js", "--dry-run"]);
    assert!(build_err.contains("incompatible with the dependency graph"));
    assert!(
        build_err
            .contains("'supported/virtual-root/virtual' requires 'supported/virtual-root/lib'")
    );
    assert!(build_err.contains("supports [native]"));
}

#[test]
fn test_check_skips_unrealizable_test_targets_when_source_supports_backend() {
    let dir = TestDir::new("supported_targets_test_target_mismatch.in");

    let check_out = get_stdout(
        &dir,
        ["check", "--target", "js", "--dry-run", "--sort-input"],
    );
    assert_contains_and_absent(
        &check_out,
        &["./lib/lib.mbt"],
        &["./lib/lib_test.mbt", "./lib/lib_wbtest.mbt"],
    );
}

#[test]
fn test_test_skips_unrealizable_test_targets_when_source_supports_backend() {
    let dir = TestDir::new("supported_targets_test_target_mismatch.in");

    let test_out = get_stdout(&dir, ["test", "--target", "js", "-v", "--no-parallelize"]);
    assert_contains_and_absent(
        &test_out,
        &["inline js test", "Total tests: 1, passed: 1, failed: 0."],
        &["blackbox should be skipped", "whitebox should be skipped"],
    );
}

#[test]
fn test_check_warns_when_test_target_is_never_realizable() {
    let dir = TestDir::new("supported_targets_test_target_unrealizable.in");

    let stderr = get_stderr(&dir, ["check", "--target", "js", "--dry-run"]);
    assert!(stderr.contains("Skipping whitebox tests for package"));
    assert!(stderr.contains("Skipping blackbox tests for package"));
    assert!(stderr.contains("supported/test-target-unrealizable/lib"));
    assert!(stderr.contains("unrealizable on every backend"));
}

#[test]
fn test_test_warns_when_test_target_is_never_realizable() {
    let dir = TestDir::new("supported_targets_test_target_unrealizable.in");

    let stderr = get_stderr(&dir, ["test", "--target", "js", "-v", "--no-parallelize"]);
    assert!(stderr.contains("Skipping whitebox tests for package"));
    assert!(stderr.contains("Skipping blackbox tests for package"));
    assert!(stderr.contains("supported/test-target-unrealizable/lib"));
    assert!(stderr.contains("unrealizable on every backend"));
}

#[test]
fn test_check_skips_backend_mismatched_tests_as_info() {
    let dir = TestDir::new("supported_targets_test_target_mismatch.in");

    let stderr = get_stderr(&dir, ["check", "--target", "js", "--dry-run"]);
    assert!(!stderr.contains("target is not realizable for this backend"));

    let verbose_stderr = get_stderr(&dir, ["check", "--target", "js", "--dry-run", "-v"]);
    assert!(verbose_stderr.contains("Skipping whitebox tests for package"));
    assert!(verbose_stderr.contains("Skipping blackbox tests for package"));
    assert!(verbose_stderr.contains("target is not realizable for this backend"));
}

#[test]
fn test_missing_supported_targets_root_warns_when_dep_declares() {
    let dir = TestDir::new("supported_targets_missing_root_warning.in");
    let stderr = get_stderr(&dir, ["check", "--target", "js", "--dry-run"]);
    assert!(stderr.contains("does not declare `supported_targets`"));
    assert!(stderr.contains("supported/missing-root/main"));
    assert!(stderr.contains("supported/missing-root/lib"));
}

#[test]
fn test_legacy_supported_targets_warning_is_local_only() {
    let dir = TestDir::new("mixed_backend_local_dep.in");
    let stderr = get_stderr(&dir, ["check", "--target", "js", "--dry-run"]);
    assert!(stderr.contains("Package `mixed/localdep/web` uses legacy array syntax"));
    assert!(!stderr.contains("Package `mixed/localdep/jsdep` uses legacy array syntax"));
}

#[test]
fn test_explicit_target_suppresses_conflicting_workspace_preferred_target_warning() {
    let dir = TestDir::new("workspace_conflicting_preferred_targets.in");

    let default_stderr = get_stderr(&dir, ["check", "--dry-run", "--sort-input"]);
    assert_preferred_target_conflict_warning(&default_stderr);

    let explicit_stderr = get_stderr(
        &dir,
        ["check", "--target", "js", "--dry-run", "--sort-input"],
    );
    assert!(
        !explicit_stderr.contains(PREFERRED_TARGET_CONFLICT_WARNING),
        "stderr: {explicit_stderr}"
    );
}

#[test]
fn test_conflicting_workspace_preferred_targets_default_to_wasm_gc_across_commands() {
    let dir = TestDir::new("workspace_conflicting_preferred_targets.in");
    let commands = [
        ("build", &["build", "--dry-run", "--sort-input"][..]),
        ("check", &["check", "--dry-run", "--sort-input"][..]),
        ("test", &["test", "--dry-run", "--sort-input"][..]),
        ("bundle", &["bundle", "--dry-run", "--sort-input"][..]),
        ("bench", &["bench", "--dry-run", "--sort-input"][..]),
    ];

    for (command, args) in commands {
        let stdout = get_stdout(&dir, args.iter().copied());
        let stderr = get_stderr(&dir, args.iter().copied());
        assert_conflicting_workspace_preferred_targets_default_to_wasm_gc(
            command, &stdout, &stderr,
        );
    }
}

#[test]
fn test_conflicting_workspace_preferred_targets_info_uses_module_preferred_targets() {
    let dir = TestDir::new("workspace_conflicting_preferred_targets.in");

    let stderr = get_stderr(&dir, ["info"]);
    assert!(
        !stderr.contains(PREFERRED_TARGET_CONFLICT_WARNING),
        "stderr: {stderr}"
    );

    let js_mbti =
        std::fs::read_to_string(dir.join("js_preferred/src/lib").join(MBTI_GENERATED)).unwrap();
    assert_contains_and_absent(
        &js_mbti,
        &["pub fn js_value() -> Int", "pub fn js_extra() -> Int"],
        &["js_wasm_gc_extra", "native_extra", "native_wasm_gc_extra"],
    );

    let native_mbti =
        std::fs::read_to_string(dir.join("native_preferred/src/lib").join(MBTI_GENERATED)).unwrap();
    assert_contains_and_absent(
        &native_mbti,
        &[
            "pub fn native_value() -> Int",
            "pub fn native_extra() -> Int",
        ],
        &["native_wasm_gc_extra", "js_extra", "js_wasm_gc_extra"],
    );
}
