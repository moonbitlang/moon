use super::*;
use moonutil::common::MBTI_GENERATED;

fn assert_requires_target_module(stderr: &str, command: &str) {
    assert!(
        stderr.contains(&format!(
            "`moon {command}` cannot infer a target module in workspace `$ROOT`"
        )),
        "expected missing target module error, got:\n{stderr}"
    );
}

fn assert_registry_resolution_failure(stderr: &str) {
    assert!(
        stderr
            .contains("Failed to resolve registry dependency `alice/liba` for module `alice/app`"),
        "expected registry dependency resolution failure, got:\n{stderr}"
    );
}

#[test]
fn test_workspace_commands() {
    let dir = TestDir::new("workspace_basic.in");

    check(
        get_stdout(&dir, ["build", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./liba/src/lib/lib.mbt -o ./_build/wasm-gc/debug/build/alice/liba/lib/lib.core -pkg alice/liba/lib -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources alice/liba/lib:./liba/src/lib -target wasm-gc -g -O0 -source-map -workspace-path ./liba -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./app/src/main/main.mbt -o ./_build/wasm-gc/debug/build/alice/app/main/main.core -pkg alice/app/main -is-main -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/build/alice/liba/lib/lib.mi:lib -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources alice/app/main:./app/src/main -target wasm-gc -g -O0 -source-map -workspace-path ./app -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/build/alice/liba/lib/lib.core ./_build/wasm-gc/debug/build/alice/app/main/main.core -main alice/app/main -o ./_build/wasm-gc/debug/build/alice/app/main/main.wasm -pkg-config-path ./app/src/main/moon.pkg.json -pkg-sources alice/liba/lib:./liba/src/lib -pkg-sources alice/app/main:./app/src/main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm-gc -g -O0 -source-map
        "#]],
    );

    check(
        get_stdout(&dir, ["test", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./liba/src/lib/lib.mbt -o ./_build/wasm-gc/debug/test/alice/liba/lib/lib.core -pkg alice/liba/lib -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources alice/liba/lib:./liba/src/lib -target wasm-gc -g -O0 -source-map -workspace-path ./liba -all-pkgs ./_build/wasm-gc/debug/test/all_pkgs.json
            moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/alice/app/main/__generated_driver_for_internal_test.mbt --output-metadata ./_build/wasm-gc/debug/test/alice/app/main/__internal_test_info.json ./app/src/main/main.mbt --target wasm-gc --pkg-name alice/app/main --driver-kind internal
            moonc build-package ./app/src/main/main.mbt ./_build/wasm-gc/debug/test/alice/app/main/__generated_driver_for_internal_test.mbt -o ./_build/wasm-gc/debug/test/alice/app/main/main.internal_test.core -pkg alice/app/main -is-main -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/test/alice/liba/lib/lib.mi:lib -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources alice/app/main:./app/src/main -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path ./app -all-pkgs ./_build/wasm-gc/debug/test/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/test/alice/liba/lib/lib.core ./_build/wasm-gc/debug/test/alice/app/main/main.internal_test.core -main alice/app/main -o ./_build/wasm-gc/debug/test/alice/app/main/main.internal_test.wasm -test-mode -pkg-config-path ./app/src/main/moon.pkg.json -pkg-sources alice/liba/lib:./liba/src/lib -pkg-sources alice/app/main:./app/src/main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map
            moonc build-package ./app/src/main/main.mbt -o ./_build/wasm-gc/debug/test/alice/app/main/main.core -pkg alice/app/main -is-main -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/test/alice/liba/lib/lib.mi:lib -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources alice/app/main:./app/src/main -target wasm-gc -g -O0 -source-map -workspace-path ./app -all-pkgs ./_build/wasm-gc/debug/test/all_pkgs.json
            moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/alice/app/main/__generated_driver_for_blackbox_test.mbt --output-metadata ./_build/wasm-gc/debug/test/alice/app/main/__blackbox_test_info.json ./app/src/main/main_test.mbt --doctest-only ./app/src/main/main.mbt --target wasm-gc --pkg-name alice/app/main --driver-kind blackbox
            moonc build-package ./app/src/main/main_test.mbt ./_build/wasm-gc/debug/test/alice/app/main/__generated_driver_for_blackbox_test.mbt -doctest-only ./app/src/main/main.mbt -o ./_build/wasm-gc/debug/test/alice/app/main/main.blackbox_test.core -pkg alice/app/main_blackbox_test -is-main -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/test/alice/liba/lib/lib.mi:lib -i ./_build/wasm-gc/debug/test/alice/app/main/main.mi:main -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources alice/app/main_blackbox_test:./app/src/main -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path ./app -all-pkgs ./_build/wasm-gc/debug/test/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/test/alice/liba/lib/lib.core ./_build/wasm-gc/debug/test/alice/app/main/main.core ./_build/wasm-gc/debug/test/alice/app/main/main.blackbox_test.core -main alice/app/main_blackbox_test -o ./_build/wasm-gc/debug/test/alice/app/main/main.blackbox_test.wasm -test-mode -pkg-config-path ./app/src/main/moon.pkg.json -pkg-sources alice/liba/lib:./liba/src/lib -pkg-sources alice/app/main:./app/src/main -pkg-sources alice/app/main_blackbox_test:./app/src/main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/alice/liba/lib/__generated_driver_for_internal_test.mbt --output-metadata ./_build/wasm-gc/debug/test/alice/liba/lib/__internal_test_info.json ./liba/src/lib/lib.mbt --target wasm-gc --pkg-name alice/liba/lib --driver-kind internal
            moonc build-package ./liba/src/lib/lib.mbt ./_build/wasm-gc/debug/test/alice/liba/lib/__generated_driver_for_internal_test.mbt -o ./_build/wasm-gc/debug/test/alice/liba/lib/lib.internal_test.core -pkg alice/liba/lib -is-main -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources alice/liba/lib:./liba/src/lib -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path ./liba -all-pkgs ./_build/wasm-gc/debug/test/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/test/alice/liba/lib/lib.internal_test.core -main alice/liba/lib -o ./_build/wasm-gc/debug/test/alice/liba/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./liba/src/lib/moon.pkg.json -pkg-sources alice/liba/lib:./liba/src/lib -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/alice/liba/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./_build/wasm-gc/debug/test/alice/liba/lib/__blackbox_test_info.json ./liba/src/lib/lib_test.mbt --doctest-only ./liba/src/lib/lib.mbt --target wasm-gc --pkg-name alice/liba/lib --driver-kind blackbox
            moonc build-package ./liba/src/lib/lib_test.mbt ./_build/wasm-gc/debug/test/alice/liba/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./liba/src/lib/lib.mbt -o ./_build/wasm-gc/debug/test/alice/liba/lib/lib.blackbox_test.core -pkg alice/liba/lib_blackbox_test -is-main -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/test/alice/liba/lib/lib.mi:lib -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources alice/liba/lib_blackbox_test:./liba/src/lib -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path ./liba -all-pkgs ./_build/wasm-gc/debug/test/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/test/alice/liba/lib/lib.core ./_build/wasm-gc/debug/test/alice/liba/lib/lib.blackbox_test.core -main alice/liba/lib_blackbox_test -o ./_build/wasm-gc/debug/test/alice/liba/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./liba/src/lib/moon.pkg.json -pkg-sources alice/liba/lib:./liba/src/lib -pkg-sources alice/liba/lib_blackbox_test:./liba/src/lib -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map
        "#]],
    );

    if cfg!(windows) {
        check(
            get_stdout(&dir, ["fmt", "--dry-run", "--sort-input"]),
            expect![[r#"
                moonfmt ./liba/src/lib/moon.pkg.json -o ./_build/wasm-gc/release/format/alice/liba/lib/moon.pkg
                cmd /c copy ./_build/wasm-gc/release/format/alice/liba/lib/moon.pkg ./liba/src/lib/moon.pkg
                cmd /c del ./liba/src/lib/moon.pkg.json
                moonfmt ./app/src/main/moon.pkg.json -o ./_build/wasm-gc/release/format/alice/app/main/moon.pkg
                cmd /c copy ./_build/wasm-gc/release/format/alice/app/main/moon.pkg ./app/src/main/moon.pkg
                cmd /c del ./app/src/main/moon.pkg.json
                moonfmt ./app/src/main/main_test.mbt -w -o ./_build/wasm-gc/release/format/alice/app/main/main_test.mbt
                moonfmt ./app/src/main/main.mbt -w -o ./_build/wasm-gc/release/format/alice/app/main/main.mbt
                moonfmt ./liba/src/lib/lib_test.mbt -w -o ./_build/wasm-gc/release/format/alice/liba/lib/lib_test.mbt
                moonfmt ./liba/src/lib/lib.mbt -w -o ./_build/wasm-gc/release/format/alice/liba/lib/lib.mbt
            "#]],
        );
    } else {
        check(
            get_stdout(&dir, ["fmt", "--dry-run", "--sort-input"]),
            expect![[r#"
                moon tool format-workspace --old ./moon.work --write --new ./_build/wasm-gc/release/format/moon.work
                moonfmt ./liba/src/lib/moon.pkg.json -o ./_build/wasm-gc/release/format/alice/liba/lib/moon.pkg
                cp ./_build/wasm-gc/release/format/alice/liba/lib/moon.pkg ./liba/src/lib/moon.pkg
                rm ./liba/src/lib/moon.pkg.json
                moonfmt ./app/src/main/moon.pkg.json -o ./_build/wasm-gc/release/format/alice/app/main/moon.pkg
                cp ./_build/wasm-gc/release/format/alice/app/main/moon.pkg ./app/src/main/moon.pkg
                rm ./app/src/main/moon.pkg.json
                moonfmt ./app/src/main/main_test.mbt -w -o ./_build/wasm-gc/release/format/alice/app/main/main_test.mbt
                moonfmt ./app/src/main/main.mbt -w -o ./_build/wasm-gc/release/format/alice/app/main/main.mbt
                moonfmt ./liba/src/lib/lib_test.mbt -w -o ./_build/wasm-gc/release/format/alice/liba/lib/lib_test.mbt
                moonfmt ./liba/src/lib/lib.mbt -w -o ./_build/wasm-gc/release/format/alice/liba/lib/lib.mbt
            "#]],
        );
    }

    let stderr = get_stderr(&dir, ["check", "--sort-input"]);
    assert!(stderr.contains("Finished. moon: ran "));

    check(get_stdout(&dir, ["info"]), expect![[r#""#]]);

    let lib_mi_out =
        std::fs::read_to_string(dir.join("liba/src/lib").join(MBTI_GENERATED)).unwrap();
    expect![[r#"
        // Generated using `moon info`, DON'T EDIT IT
        package "alice/liba/lib"

        // Values
        pub fn hello() -> String

        // Errors

        // Types and methods

        // Type aliases

        // Traits

    "#]]
    .assert_eq(&lib_mi_out);

    let main_mi_out =
        std::fs::read_to_string(dir.join("app/src/main").join(MBTI_GENERATED)).unwrap();
    expect![[r#"
        // Generated using `moon info`, DON'T EDIT IT
        package "alice/app/main"

        // Values

        // Errors

        // Types and methods

        // Type aliases

        // Traits

    "#]]
    .assert_eq(&main_mi_out);

    let metadata = std::fs::read_to_string(dir.join("_build/packages.json")).unwrap();
    let metadata = replace_dir(&metadata, &dir);
    let metadata: serde_json::Value = serde_json::from_str(&metadata).unwrap();

    assert_eq!(metadata["source_dir"], "$ROOT");
    assert_eq!(metadata["name"], "workspace");
    assert_eq!(metadata["deps"], serde_json::json!(["alice/liba"]));
    assert_eq!(metadata["backend"], "wasm-gc");
    assert_eq!(metadata["opt_level"], "debug");
    assert_eq!(metadata["source"], serde_json::Value::Null);
    assert_eq!(metadata.get("workspace"), None);

    let packages = metadata["packages"].as_array().unwrap();
    let artifact_for = |root: &str, rel: &str| {
        packages
            .iter()
            .find(|pkg| pkg["root"] == root && pkg["rel"] == rel)
            .unwrap()["artifact"]
            .as_str()
            .unwrap()
            .to_owned()
    };

    assert_eq!(
        artifact_for("alice/app", "main"),
        "$ROOT/_build/wasm-gc/debug/check/alice/app/main/main.mi"
    );
    assert_eq!(
        artifact_for("alice/liba", "lib"),
        "$ROOT/_build/wasm-gc/debug/check/alice/liba/lib/lib.mi"
    );
}

#[test]
fn test_workspace_root_path_selector_is_skipped() {
    let dir = TestDir::new("workspace_basic.in");

    check(
        get_stdout(&dir, ["build", ".", "--dry-run"]),
        expect![[r#""#]],
    );
    check(
        get_stdout(&dir, ["check", ".", "--dry-run"]),
        expect![[r#""#]],
    );

    let build_stderr = get_stderr(&dir, ["build", ".", "--dry-run"]);
    assert!(
        !build_stderr.contains("skipping path"),
        "stderr: {build_stderr}"
    );

    let check_stderr = get_stderr(&dir, ["check", ".", "--dry-run"]);
    assert!(
        !check_stderr.contains("skipping path"),
        "stderr: {check_stderr}"
    );

    let build_stderr = get_stderr(&dir, ["build", ".", "--dry-run", "--verbose"]);
    assert!(
        build_stderr.contains("skipping path `.`"),
        "stderr: {build_stderr}"
    );

    let check_stderr = get_stderr(&dir, ["check", ".", "--dry-run", "--verbose"]);
    assert!(
        check_stderr.contains("skipping path `.`"),
        "stderr: {check_stderr}"
    );
}

#[test]
fn test_workspace_module_root_path_selector_is_skipped() {
    let dir = TestDir::new("workspace_basic.in");

    check(
        get_stdout(&dir, ["build", "app", "--dry-run"]),
        expect![[r#""#]],
    );
    check(
        get_stdout(&dir, ["check", "app", "--dry-run"]),
        expect![[r#""#]],
    );
    check(
        get_stdout(&dir, ["build", "liba", "--dry-run"]),
        expect![[r#""#]],
    );
    check(
        get_stdout(&dir, ["check", "liba", "--dry-run"]),
        expect![[r#""#]],
    );

    let build_app = get_stderr(&dir, ["build", "app", "--dry-run", "--verbose"]);
    assert!(
        build_app.contains("skipping path `app`"),
        "stderr: {build_app}"
    );

    let check_app = get_stderr(&dir, ["check", "app", "--dry-run", "--verbose"]);
    assert!(
        check_app.contains("skipping path `app`"),
        "stderr: {check_app}"
    );

    let build_liba = get_stderr(&dir, ["build", "liba", "--dry-run", "--verbose"]);
    assert!(
        build_liba.contains("skipping path `liba`"),
        "stderr: {build_liba}"
    );

    let check_liba = get_stderr(&dir, ["check", "liba", "--dry-run", "--verbose"]);
    assert!(
        check_liba.contains("skipping path `liba`"),
        "stderr: {check_liba}"
    );
}

#[test]
fn test_workspace_member_path_selector_uses_workspace_context() {
    let dir = TestDir::new("workspace_basic.in");

    let build_stderr = get_stderr(
        &dir,
        [
            "-C",
            "app",
            "build",
            "src/main",
            "../liba/src/lib",
            "--dry-run",
            "--verbose",
        ],
    );
    assert!(
        !build_stderr.contains("skipping path `../liba/src/lib`"),
        "stderr: {build_stderr}"
    );

    let check_stderr = get_stderr(
        &dir,
        [
            "-C",
            "app",
            "check",
            "src/main",
            "../liba/src/lib",
            "--dry-run",
            "--verbose",
        ],
    );
    assert!(
        !check_stderr.contains("skipping path `../liba/src/lib`"),
        "stderr: {check_stderr}"
    );

    let fmt_stderr = get_stderr(
        &dir,
        [
            "-C",
            "app",
            "fmt",
            "src/main",
            "../liba/src/lib",
            "--dry-run",
            "--verbose",
        ],
    );
    assert!(
        !fmt_stderr.contains("skipping path `../liba/src/lib`"),
        "stderr: {fmt_stderr}"
    );

    let check_no_mi_stderr = get_err_stderr(
        &dir,
        [
            "-C",
            "app",
            "check",
            "src/main",
            "../liba/src/lib",
            "--no-mi",
            "--dry-run",
        ],
    );
    assert!(
        check_no_mi_stderr
            .contains("`--no-mi` requires the selector to resolve to a single package"),
        "stderr: {check_no_mi_stderr}"
    );
}

#[test]
fn test_work_init_creates_empty_workspace() {
    let dir = TestDir::new("hello");

    check(
        get_stdout(&dir, ["work", "init"]),
        expect![[r#"
            Created moon.work
        "#]],
    );

    check(
        std::fs::read_to_string(dir.join("moon.work")).unwrap(),
        expect![[r#"
            members = []
        "#]],
    );
}

#[test]
fn test_work_use_reads_legacy_workspace_and_writes_moon_work() {
    let dir = TestDir::new("workspace_basic.in");

    std::fs::remove_file(dir.join("moon.work")).unwrap();
    std::fs::write(
        dir.join("moon.work.json"),
        r#"{
  "preferred-target": "wasm-gc",
  "use": [
    "./liba"
  ]
}"#,
    )
    .unwrap();

    check(
        get_stdout(&dir, ["work", "use", "app"]),
        expect![[r#"
            Updated moon.work
        "#]],
    );

    check(
        std::fs::read_to_string(dir.join("moon.work")).unwrap(),
        expect![[r#"
            members = [
              "./liba",
              "./app",
            ]
            preferred_target = "wasm-gc"
        "#]],
    );

    check(
        std::fs::read_to_string(dir.join("moon.work.json")).unwrap(),
        expect![[r#"
            {
              "preferred-target": "wasm-gc",
              "use": [
                "./liba"
              ]
            }"#]],
    );
}

#[test]
fn test_work_sync_ignores_unrelated_ancestor_workspace() {
    let dir = TestDir::new("workspace_basic.in");

    std::fs::create_dir_all(dir.join("extra")).unwrap();
    std::fs::write(
        dir.join("extra/moon.mod.json"),
        r#"{
  "name": "alice/extra"
}"#,
    )
    .unwrap();

    let stderr = get_err_stderr(&dir, ["-C", "extra", "work", "sync"]);

    assert!(stderr.contains("`moon work sync` requires `moon.work` or `moon.work.json`"));
}

#[test]
fn test_work_use_ignores_unrelated_ancestor_workspace() {
    let dir = TestDir::new("workspace_basic.in");

    std::fs::create_dir_all(dir.join("extra")).unwrap();
    std::fs::write(
        dir.join("extra/moon.mod.json"),
        r#"{
  "name": "alice/extra"
}"#,
    )
    .unwrap();

    check(
        get_stdout(&dir, ["-C", "extra", "work", "use", "."]),
        expect![[r#"
            Created moon.work
        "#]],
    );

    check(
        std::fs::read_to_string(dir.join("extra/moon.work")).unwrap(),
        expect![[r#"
            members = [
              ".",
            ]
        "#]],
    );

    check(
        std::fs::read_to_string(dir.join("moon.work")).unwrap(),
        expect![[r#"
            members = [
              "./app",
              "./liba",
            ]
            preferred_target = "wasm-gc"
        "#]],
    );
}

#[test]
fn test_workspace_commands_find_ancestor_workspace_from_nested_non_module_dir() {
    let dir = TestDir::new("workspace_basic.in");
    std::fs::create_dir_all(dir.join("tools")).unwrap();

    check(get_stdout(&dir, ["-C", "tools", "info"]), expect![[r#""#]]);
}

#[test]
fn test_work_use_reuses_ancestor_workspace_from_nested_non_module_dir() {
    let dir = TestDir::new("workspace_basic.in");
    std::fs::create_dir_all(dir.join("tools")).unwrap();
    std::fs::write(dir.join("moon.work.json"), r#"{ "use": ["./liba"] }"#).unwrap();

    check(
        get_stdout(&dir, ["-C", "tools", "work", "use", "../app"]),
        expect![[r#"
            moon.work is already up to date
        "#]],
    );
}

#[test]
fn test_workspace_sync_updates_member_manifests() {
    let dir = TestDir::new("workspace_basic.in");

    check(
        get_stdout(&dir, ["work", "sync"]),
        expect![[r#"
            Synced workspace manifests:
            app/moon.mod.json
        "#]],
    );

    check(
        std::fs::read_to_string(dir.join("app/moon.mod.json")).unwrap(),
        expect![[r#"
            {
              "name": "alice/app",
              "version": "0.1.0",
              "deps": {
                "alice/liba": "0.1.1"
              },
              "source": "src"
            }"#]],
    );

    check(
        std::fs::read_to_string(dir.join("liba/moon.mod.json")).unwrap(),
        expect![[r#"
            {
              "name": "alice/liba",
              "version": "0.1.1",
              "source": "src"
            }
        "#]],
    );
}

#[test]
fn test_work_sync_requires_workspace() {
    let dir = TestDir::new("hello");
    let stderr = get_err_stderr(&dir, ["work", "sync"]);

    assert!(stderr.contains("`moon work sync` requires `moon.work` or `moon.work.json`"));
}

#[test]
fn test_single_module_commands_fail_at_workspace_root() {
    let dir = TestDir::new("workspace_basic.in");

    let stderr = get_err_stderr(&dir, ["tree"]);
    assert_requires_target_module(&stderr, "tree");

    let stderr = get_err_stderr(&dir, ["remove", "alice/liba"]);
    assert_requires_target_module(&stderr, "remove");

    let stderr = get_err_stderr(&dir, ["add", "alice/liba@0.1.0", "--no-update"]);
    assert_requires_target_module(&stderr, "add");

    let stderr = get_err_stderr(&dir, ["package", "--list"]);
    assert_requires_target_module(&stderr, "package");

    let stderr = get_err_stderr(&dir, ["publish", "--dry-run"]);
    assert_requires_target_module(&stderr, "publish");
}

#[test]
fn test_single_module_commands_from_member_dir_target_member_manifest() {
    let dir = TestDir::new("workspace_basic.in");

    check(get_stdout(&dir, ["-C", "app", "tree"]), expect![[r#""#]]);

    let stderr = get_stderr(
        &dir,
        ["-C", "app", "add", "moonbitlang/core", "--no-update"],
    );
    assert!(
        stderr.contains("no need to add `moonbitlang/core` as dependency"),
        "expected add command to target app module, got:\n{stderr}"
    );

    check(
        get_stdout(&dir, ["-C", "app", "remove", "alice/liba"]),
        expect![[r#""#]],
    );

    let app_manifest = std::fs::read_to_string(dir.join("app/moon.mod.json")).unwrap();
    check(
        app_manifest.trim_end_matches('\n'),
        expect![[r#"
            {
              "name": "alice/app",
              "version": "0.1.0",
              "deps": {},
              "source": "src"
            }"#]],
    );
}

#[test]
fn test_manifest_path_targets_workspace_member_for_single_module_commands() {
    let dir = TestDir::new("workspace_basic.in");

    let stderr = get_stderr(
        &dir,
        [
            "--manifest-path",
            "app/moon.mod.json",
            "add",
            "moonbitlang/core",
            "--no-update",
        ],
    );
    assert!(
        stderr.contains("no need to add `moonbitlang/core` as dependency"),
        "expected add command to target app module, got:\n{stderr}"
    );

    let stderr = get_err_stderr(
        &dir,
        ["--manifest-path", "app/moon.mod.json", "package", "--list"],
    );
    assert_registry_resolution_failure(&stderr);

    check(
        std::fs::read_to_string(dir.join("app/moon.mod.json")).unwrap(),
        expect![[r#"
            {
              "name": "alice/app",
              "version": "0.1.0",
              "source": "src",
              "deps": {
                "alice/liba": "0.1.0"
              }
            }
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "--manifest-path",
                "app/moon.mod.json",
                "remove",
                "alice/liba",
            ],
        ),
        expect![[r#""#]],
    );

    let app_manifest = std::fs::read_to_string(dir.join("app/moon.mod.json")).unwrap();
    check(
        app_manifest.trim_end_matches('\n'),
        expect![[r#"
            {
              "name": "alice/app",
              "version": "0.1.0",
              "deps": {},
              "source": "src"
            }"#]],
    );
}

#[test]
fn test_package_targets_workspace_member_from_member_dir() {
    let dir = TestDir::new("workspace_basic.in");

    let stderr = get_err_stderr(&dir, ["-C", "app", "package", "--list"]);
    assert_registry_resolution_failure(&stderr);
}

#[test]
fn test_prove_targets_member_module_with_workspace_resolution() {
    let dir = TestDir::new("workspace_basic.in");

    let stderr = get_err_stderr(&dir, ["prove", "--dry-run"]);
    assert_requires_target_module(&stderr, "prove");

    let stdout = get_stdout(&dir, ["-C", "app", "prove", "--dry-run"]);
    assert!(
        stdout.contains("moonc prove ./app/src/main/main.mbt"),
        "expected app prove dry-run to target the app member, got:\n{stdout}"
    );
    assert!(
        stdout.contains("-workspace-path ./app"),
        "expected app prove dry-run to keep the app workspace path, got:\n{stdout}"
    );
    assert!(
        stdout.contains("alice/liba/lib"),
        "expected app prove dry-run to keep workspace-local dependency resolution, got:\n{stdout}"
    );

    let stdout = get_stdout(
        &dir,
        ["--manifest-path", "app/moon.mod.json", "prove", "--dry-run"],
    );
    assert!(
        stdout.contains("moonc prove ./app/src/main/main.mbt"),
        "expected manifest-path prove dry-run to target the app member, got:\n{stdout}"
    );
    assert!(
        stdout.contains("-workspace-path ./app"),
        "expected manifest-path prove dry-run to keep workspace-local context, got:\n{stdout}"
    );
    assert!(
        stdout.contains("alice/liba/lib"),
        "expected manifest-path prove dry-run to keep workspace-local dependency resolution, got:\n{stdout}"
    );

    let stdout = get_stdout(
        &dir,
        [
            "--manifest-path",
            "liba/moon.mod.json",
            "prove",
            "--dry-run",
        ],
    );
    assert!(
        stdout.contains("alice/liba/lib"),
        "expected prove dry-run to target the liba module, got:\n{stdout}"
    );
}

#[test]
fn test_doc_targets_member_module_with_workspace_resolution() {
    let dir = TestDir::new("workspace_basic.in");

    let stderr = get_err_stderr(&dir, ["doc", "--dry-run"]);
    assert_requires_target_module(&stderr, "doc");

    let stdout = get_stdout(&dir, ["-C", "app", "doc", "--dry-run"]);
    assert!(
        stdout.contains("moondoc ./app -o ./_build/doc"),
        "expected doc dry-run to use the app module as moondoc root, got:\n{stdout}"
    );
    assert!(
        stdout.contains("-packages-json ./_build/packages.json"),
        "expected doc dry-run to pass workspace metadata to moondoc, got:\n{stdout}"
    );
    assert!(
        stdout.contains("./_build/wasm-gc/debug/check/alice/app/main/main.mi"),
        "expected doc dry-run to keep the workspace build layout for the app module, got:\n{stdout}"
    );
    assert!(
        stdout.contains("./_build/wasm-gc/debug/check/alice/liba/lib/lib.mi"),
        "expected doc dry-run to keep the workspace build layout for dependencies, got:\n{stdout}"
    );

    let stdout = get_stdout(&dir, ["-C", "app", "doc", "--serve", "--dry-run"]);
    assert!(
        stdout.contains("moondoc ./app -o ./_build/doc"),
        "expected doc --serve dry-run to use the app module as moondoc root, got:\n{stdout}"
    );
    assert!(
        stdout.contains("-serve-mode"),
        "expected doc --serve dry-run to enable serve mode for moondoc, got:\n{stdout}"
    );

    let stdout = get_stdout(
        &dir,
        ["--manifest-path", "app/moon.mod.json", "doc", "--dry-run"],
    );
    assert!(
        stdout.contains("moondoc ./app -o ./_build/doc"),
        "expected manifest-path doc dry-run to target the app member, got:\n{stdout}"
    );

    let stdout = get_stdout(&dir, ["-C", "app/src/main", "doc", "--dry-run"]);
    assert!(
        stdout.contains("moondoc ./app -o ./_build/doc"),
        "expected nested doc dry-run to resolve the app module like `moon publish`, got:\n{stdout}"
    );

    let _ = get_stderr(&dir, ["-C", "app", "doc"]);
    assert!(
        dir.join("_build/doc/alice/app/main/members.md").exists(),
        "expected member docs to be generated under the app module"
    );

    let metadata = std::fs::read_to_string(dir.join("_build/packages.json")).unwrap();
    let metadata = replace_dir(&metadata, &dir);
    let metadata: serde_json::Value = serde_json::from_str(&metadata).unwrap();

    assert_eq!(metadata["source_dir"], "$ROOT/app");
    assert_eq!(metadata["name"], "workspace");
    assert_eq!(metadata["deps"], serde_json::json!(["alice/liba"]));

    let packages = metadata["packages"].as_array().unwrap();
    let app_pkg = packages
        .iter()
        .find(|pkg| pkg["root"] == "alice/app" && pkg["rel"] == "main")
        .unwrap();
    assert_eq!(app_pkg["is-third-party"], serde_json::json!(false));
    assert_eq!(
        app_pkg["artifact"],
        serde_json::json!("$ROOT/_build/wasm-gc/debug/check/alice/app/main/main.mi")
    );

    let lib_pkg = packages
        .iter()
        .find(|pkg| pkg["root"] == "alice/liba" && pkg["rel"] == "lib")
        .unwrap();
    assert_eq!(lib_pkg["is-third-party"], serde_json::json!(false));
    assert_eq!(
        lib_pkg["artifact"],
        serde_json::json!("$ROOT/_build/wasm-gc/debug/check/alice/liba/lib/lib.mi")
    );
}
