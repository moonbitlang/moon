use super::*;

#[test]
fn test_target_backend() {
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
            ["build", "--dry-run", "--target", "wasm-gc", "--nostd"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/debug/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/debug/build/main/main.core -pkg hello/main -is-main -i ./_build/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc link-core ./_build/wasm-gc/debug/build/lib/lib.core ./_build/wasm-gc/debug/build/main/main.core -main hello/main -o ./_build/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map
        "#]],
    );
    check(
        get_stdout(&dir, ["build", "--dry-run", "--target", "js", "--nostd"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/js/debug/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target js -g -O0 -source-map -workspace-path . -all-pkgs ./_build/js/debug/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/js/debug/build/main/main.core -pkg hello/main -is-main -i ./_build/js/debug/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target js -g -O0 -source-map -workspace-path . -all-pkgs ./_build/js/debug/build/all_pkgs.json
            moonc link-core ./_build/js/debug/build/lib/lib.core ./_build/js/debug/build/main/main.core -main hello/main -o ./_build/js/debug/build/main/main.js -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target js -g -O0 -source-map
        "#]],
    );
    check(
        get_stdout(&dir, ["build", "--dry-run", "--nostd"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/debug/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/debug/build/main/main.core -pkg hello/main -is-main -i ./_build/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc link-core ./_build/wasm-gc/debug/build/lib/lib.core ./_build/wasm-gc/debug/build/main/main.core -main hello/main -o ./_build/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map
        "#]],
    );
    check(
        get_stdout(&dir, ["run", "main", "--dry-run", "--nostd"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/debug/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/debug/build/main/main.core -pkg hello/main -is-main -i ./_build/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc link-core ./_build/wasm-gc/debug/build/lib/lib.core ./_build/wasm-gc/debug/build/main/main.core -main hello/main -o ./_build/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map
            moonrun ./_build/wasm-gc/debug/build/main/main.wasm --
        "#]],
    );
    check(
        get_stdout(
            &dir,
            ["run", "main", "--dry-run", "--target", "wasm-gc", "--nostd"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/debug/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/debug/build/main/main.core -pkg hello/main -is-main -i ./_build/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc link-core ./_build/wasm-gc/debug/build/lib/lib.core ./_build/wasm-gc/debug/build/main/main.core -main hello/main -o ./_build/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map
            moonrun ./_build/wasm-gc/debug/build/main/main.wasm --
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

#[test]
fn test_mixed_backend_default_selection_is_target_aware() {
    let dir = TestDir::new("mixed_backend_local_dep.in");

    let check_js = get_stdout(
        &dir,
        ["check", "--target", "js", "--dry-run", "--sort-input"],
    );
    assert_contains_and_absent(
        &check_js,
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

    let build_js = get_stdout(
        &dir,
        ["build", "--target", "js", "--dry-run", "--sort-input"],
    );
    assert_contains_and_absent(
        &build_js,
        &[
            "./shared/shared.mbt",
            "./web/main.mbt",
            "./deps/jsdep/lib/lib.mbt",
        ],
        &["./server/main.mbt", "./deps/nativedep/lib/lib.mbt"],
    );

    let test_js = get_stdout(
        &dir,
        ["test", "--target", "js", "--dry-run", "--sort-input"],
    );
    assert_contains_and_absent(
        &test_js,
        &["./web/web_wbtest.mbt", "./deps/jsdep/lib/lib.mbt"],
        &["./server/server_wbtest.mbt", "./deps/nativedep/lib/lib.mbt"],
    );

    let check_native = get_stdout(
        &dir,
        ["check", "--target", "native", "--dry-run", "--sort-input"],
    );
    assert_contains_and_absent(
        &check_native,
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

    let build_native = get_stdout(
        &dir,
        ["build", "--target", "native", "--dry-run", "--sort-input"],
    );
    assert_contains_and_absent(
        &build_native,
        &[
            "./shared/shared.mbt",
            "./server/main.mbt",
            "./deps/nativedep/lib/lib.mbt",
        ],
        &["./web/main.mbt", "./deps/jsdep/lib/lib.mbt"],
    );

    let test_native = get_stdout(
        &dir,
        ["test", "--target", "native", "--dry-run", "--sort-input"],
    );
    assert_contains_and_absent(
        &test_native,
        &["./server/server_wbtest.mbt", "./deps/nativedep/lib/lib.mbt"],
        &["./web/web_wbtest.mbt", "./deps/jsdep/lib/lib.mbt"],
    );
}

#[test]
fn test_mixed_backend_bench_is_target_aware() {
    let dir = TestDir::new("mixed_backend_local_dep.in");

    let bench_js = get_stdout(
        &dir,
        ["bench", "--target", "js", "--dry-run", "--sort-input"],
    );
    assert_contains_and_absent(
        &bench_js,
        &["./web/web_wbtest.mbt", "./deps/jsdep/lib/lib.mbt"],
        &["./server/server_wbtest.mbt", "./deps/nativedep/lib/lib.mbt"],
    );

    let bench_native = get_stdout(
        &dir,
        ["bench", "--target", "native", "--dry-run", "--sort-input"],
    );
    assert_contains_and_absent(
        &bench_native,
        &["./server/server_wbtest.mbt", "./deps/nativedep/lib/lib.mbt"],
        &["./web/web_wbtest.mbt", "./deps/jsdep/lib/lib.mbt"],
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

    let build_err = get_err_stderr(&dir, ["build", "server", "--target", "js", "--dry-run"]);
    assert!(
        build_err.contains("Package 'mixed/localdep/server' does not support target backend 'js'")
    );
    assert!(build_err.contains("Supported backends: [native]"));

    let test_err = get_err_stderr(
        &dir,
        [
            "test",
            "--package",
            "mixed/localdep/server",
            "--target",
            "js",
            "--dry-run",
        ],
    );
    assert!(test_err.contains("Selected package(s) do not support target backend 'js'"));
    assert!(test_err.contains("mixed/localdep/server ([native])"));

    let run_err = get_err_stderr(&dir, ["run", "server", "--target", "js", "--dry-run"]);
    assert!(
        run_err.contains("Package 'mixed/localdep/server' does not support target backend 'js'")
    );
    assert!(run_err.contains("Supported backends: [native]"));
}

#[test]
fn test_mixed_backend_run_info_bundle_are_target_aware() {
    let dir = TestDir::new("mixed_backend_local_dep.in");

    let run_js = get_stdout(
        &dir,
        ["run", "web", "--target", "js", "--dry-run", "--sort-input"],
    );
    assert_contains_and_absent(
        &run_js,
        &[
            "./shared/shared.mbt",
            "./web/main.mbt",
            "./deps/jsdep/lib/lib.mbt",
        ],
        &["./server/main.mbt", "./deps/nativedep/lib/lib.mbt"],
    );

    let run_native = get_stdout(
        &dir,
        [
            "run",
            "server",
            "--target",
            "native",
            "--dry-run",
            "--sort-input",
        ],
    );
    assert_contains_and_absent(
        &run_native,
        &[
            "./shared/shared.mbt",
            "./server/main.mbt",
            "./deps/nativedep/lib/lib.mbt",
        ],
        &["./web/main.mbt", "./deps/jsdep/lib/lib.mbt"],
    );

    get_stdout(&dir, ["info", "--target", "js"]);
    assert!(dir.join("shared").join(MBTI_GENERATED).exists());
    assert!(dir.join("web").join(MBTI_GENERATED).exists());
    assert!(!dir.join("server").join(MBTI_GENERATED).exists());

    for pkg in ["shared", "web", "server"] {
        let path = dir.join(pkg).join(MBTI_GENERATED);
        if path.exists() {
            std::fs::remove_file(path).unwrap();
        }
    }

    get_stdout(&dir, ["info", "--target", "native"]);
    assert!(dir.join("shared").join(MBTI_GENERATED).exists());
    assert!(dir.join("server").join(MBTI_GENERATED).exists());
    assert!(!dir.join("web").join(MBTI_GENERATED).exists());

    for pkg in ["shared", "web", "server"] {
        let path = dir.join(pkg).join(MBTI_GENERATED);
        if path.exists() {
            std::fs::remove_file(path).unwrap();
        }
    }

    get_stdout(&dir, ["info", "--target", "js,native"]);
    assert!(dir.join("shared").join(MBTI_GENERATED).exists());
    assert!(dir.join("web").join(MBTI_GENERATED).exists());
    assert!(dir.join("server").join(MBTI_GENERATED).exists());

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

    let check_js = get_stdout(
        &dir,
        ["check", "--target", "js", "--dry-run", "--sort-input"],
    );
    assert_contains_and_absent(
        &check_js,
        &["./main/main.mbt", "./lib/lib.mbt"],
        &["./never/never.mbt"],
    );

    let check_native = get_stdout(
        &dir,
        ["check", "--target", "native", "--dry-run", "--sort-input"],
    );
    assert_contains_and_absent(
        &check_native,
        &["./main/main.mbt", "./lib/lib.mbt"],
        &["./never/never.mbt"],
    );

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

    let check_wasm_gc = get_stdout(
        &dir,
        [
            "check",
            "lib",
            "--target",
            "wasm-gc",
            "--dry-run",
            "--sort-input",
        ],
    );
    assert_contains_and_absent(&check_wasm_gc, &["./lib/lib.mbt"], &["./main/main.mbt"]);

    let check_native = get_stdout(
        &dir,
        [
            "check",
            "lib",
            "--target",
            "native",
            "--dry-run",
            "--sort-input",
        ],
    );
    assert_contains_and_absent(&check_native, &["./lib/lib.mbt"], &["./main/main.mbt"]);

    let check_llvm = get_stdout(
        &dir,
        [
            "check",
            "lib",
            "--target",
            "llvm",
            "--dry-run",
            "--sort-input",
        ],
    );
    assert_contains_and_absent(&check_llvm, &["./lib/lib.mbt"], &["./main/main.mbt"]);

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
fn test_explicit_target_suppresses_mixed_preferred_target_warning() {
    let dir = TestDir::new("workspace_mixed_preferred_targets.in");
    let warning = "Multiple local modules specify different preferred targets; pass `--target` to choose one explicitly";

    let default_stderr = get_stderr(&dir, ["check", "--dry-run", "--sort-input"]);
    assert!(default_stderr.contains(warning), "stderr: {default_stderr}");

    let explicit_stderr = get_stderr(
        &dir,
        ["check", "--target", "js", "--dry-run", "--sort-input"],
    );
    assert!(
        !explicit_stderr.contains(warning),
        "stderr: {explicit_stderr}"
    );
}
