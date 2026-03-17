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
fn test_work_init_creates_empty_workspace() {
    let dir = TestDir::new("hello");

    check(
        get_stdout(&dir, ["work", "init"]),
        expect![[r#"
            Created moon.work.json
        "#]],
    );

    check(
        std::fs::read_to_string(dir.join("moon.work.json")).unwrap(),
        expect![[r#"
            {
              "use": []
            }"#]],
    );
}

#[test]
fn test_work_use_updates_workspace_members() {
    let dir = TestDir::new("workspace_basic.in");

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
            Updated moon.work.json
        "#]],
    );

    check(
        std::fs::read_to_string(dir.join("moon.work.json")).unwrap(),
        expect![[r#"
            {
              "use": [
                "./liba",
                "./app"
              ],
              "preferred-target": "wasm-gc"
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

    assert!(stderr.contains("`moon work sync` requires `moon.work.json`"));
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
            Created moon.work.json
        "#]],
    );

    check(
        std::fs::read_to_string(dir.join("extra/moon.work.json")).unwrap(),
        expect![[r#"
            {
              "use": [
                "."
              ]
            }"#]],
    );

    check(
        std::fs::read_to_string(dir.join("moon.work.json")).unwrap(),
        expect![[r#"
            {
              "preferred-target": "wasm-gc",
              "use": [
                "./app",
                "./liba"
              ]
            }
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

    check(
        get_stdout(&dir, ["-C", "tools", "work", "use", "../app"]),
        expect![[r#"
            moon.work.json is already up to date
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

    assert!(stderr.contains("`moon work sync` requires `moon.work.json`"));
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
