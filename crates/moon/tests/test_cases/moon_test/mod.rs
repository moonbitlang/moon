use super::*;

#[test]
fn test_moon_test_succ() {
    std::env::set_var("NO_COLOR", "1");
    let dir = TestDir::new("moon_test/succ");
    check(
        get_stdout(&dir, ["test", "-v", "--sort-input", "--no-parallelize"]),
        expect![[r#"
            test moontest/lib/hello_wbtest.mbt::0 ok
            test moontest/lib2/hello_wbtest.mbt::0 ok
            test moontest/lib2/nested/lib_wbtest.mbt::0 ok
            test moontest/lib2/nested/lib_wbtest.mbt::1 ok
            test moontest/lib3/hello_wbtest.mbt::0 ok
            test moontest/lib4/hello_wbtest.mbt::0 ok
            Total tests: 6, passed: 6, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_test_hello_exec() {
    let dir = TestDir::new("moon_test/hello_exec");
    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
            Hello, world!
        "#]],
    );
    check(
        get_stdout(&dir, ["test", "-v"]),
        expect![[r#"
            this is lib test
            test moonbitlang/hello/lib/hello_wbtest.mbt::0 ok
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    check(
        get_stdout(&dir, ["test", "--dry-run", "--debug", "--sort-input"]),
        expect![[r#"
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_whitebox_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__whitebox_test_info.json ./lib/hello_wbtest.mbt --target wasm-gc --pkg-name moonbitlang/hello/lib --driver-kind whitebox
            moonc build-package ./lib/hello.mbt ./lib/hello_wbtest.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_whitebox_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.whitebox_test.core -pkg moonbitlang/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moonbitlang/hello/lib:./lib -target wasm-gc -g -O0 -source-map -whitebox-test -no-mi -test-mode
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.whitebox_test.core -main moonbitlang/hello/lib -o ./target/wasm-gc/debug/test/lib/lib.whitebox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
        "#]],
    );
}

#[test]
fn test_moon_test_hello_exec_fntest() {
    let dir = TestDir::new("moon_test/hello_exec_fntest");
    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
            init in main/main.mbt
        "#]],
    );

    check(
        get_stdout(&dir, ["test", "-v", "--dry-run", "--sort-input"]),
        expect![[r#"
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_whitebox_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__whitebox_test_info.json ./lib/hello_wbtest.mbt --target wasm-gc --pkg-name moonbitlang/hello/lib --driver-kind whitebox
            moonc build-package ./lib/hello.mbt ./lib/hello_wbtest.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_whitebox_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.whitebox_test.core -pkg moonbitlang/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moonbitlang/hello/lib:./lib -target wasm-gc -g -O0 -source-map -whitebox-test -no-mi -test-mode
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.whitebox_test.core -main moonbitlang/hello/lib -o ./target/wasm-gc/debug/test/lib/lib.whitebox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__internal_test_info.json ./lib/hello.mbt --target wasm-gc --pkg-name moonbitlang/hello/lib --driver-kind internal
            moonc build-package ./lib/hello.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.internal_test.core -pkg moonbitlang/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moonbitlang/hello/lib:./lib -target wasm-gc -g -O0 -source-map -no-mi -test-mode
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.internal_test.core -main moonbitlang/hello/lib -o ./target/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
        "#]],
    );

    check(
        get_stdout(&dir, ["test", "-v", "--sort-input", "--no-parallelize"]),
        expect![[r#"
            test in lib/hello.mbt
            test moonbitlang/hello/lib/hello.mbt::0 ok
            test in lib/hello_test.mbt
            test moonbitlang/hello/lib/hello_wbtest.mbt::0 ok
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_test_hello_lib() {
    let dir = TestDir::new("moon_test/hello_lib");
    check(
        get_stdout(&dir, ["test", "-v"]),
        expect![[r#"
            test moonbitlang/hello/lib/hello_wbtest.mbt::0 ok
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_test_with_local_dep() {
    let dir = TestDir::new("moon_test/with_local_deps");
    check(
        get_stdout(&dir, ["test", "-v", "--frozen"]),
        expect![[r#"
            test hello31/lib/hello_wbtest.mbt::0 ok
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    check(
        get_stdout(&dir, ["run", "main", "--frozen"]),
        expect![[r#"
            hello from mooncake
            hello from mooncake2
        "#]],
    );
    // Run moon info
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["info", "--frozen"])
        .assert()
        .success();
    // Check directory structure by listing all files
    let root_dir = dir.as_ref().to_owned();
    let dir = WalkDir::new(&dir)
        .sort_by_file_name()
        .into_iter()
        .filter_map(|e| e.ok())
        .map(|e| e.path().strip_prefix(&root_dir).unwrap().to_owned())
        // Filter out target directory
        .filter(|p| !p.starts_with("target"))
        // Convert to string and join with newline
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    let joined = dir.join("\n").replace('\\', "/"); // Normalize path separator
    check(
        &joined,
        expect![[r#"

            .gitignore
            lib
            lib/hello.mbt
            lib/hello_wbtest.mbt
            lib/lib.mbti
            lib/moon.pkg.json
            main
            main/main.mbt
            main/main.mbti
            main/moon.pkg.json
            mods
            mods/lijunchen
            mods/lijunchen/mooncake
            mods/lijunchen/mooncake/lib
            mods/lijunchen/mooncake/lib/hello.mbt
            mods/lijunchen/mooncake/lib/hello_wbtest.mbt
            mods/lijunchen/mooncake/lib/moon.pkg.json
            mods/lijunchen/mooncake/moon.mod.json
            mods/lijunchen/mooncake/moon.pkg.json
            mods/lijunchen/mooncake/top.mbt
            mods/lijunchen/mooncake2
            mods/lijunchen/mooncake2/moon.mod.json
            mods/lijunchen/mooncake2/src
            mods/lijunchen/mooncake2/src/lib
            mods/lijunchen/mooncake2/src/lib/hello.mbt
            mods/lijunchen/mooncake2/src/lib/hello_wbtest.mbt
            mods/lijunchen/mooncake2/src/lib/moon.pkg.json
            mods/lijunchen/mooncake2/src/moon.pkg.json
            mods/lijunchen/mooncake2/src/top.mbt
            moon.mod.json"#]],
    );
}

#[test]
fn test_pkg_source_in() {
    let dir = TestDir::new("moon_test/with_local_deps");
    let out = get_stdout(&dir, ["build", "--dry-run", "--sort-input", "--frozen"]);
    check(
        &out,
        expect![[r#"
            moonc build-package ./mods/lijunchen/mooncake/lib/hello.mbt -w -a -alert -all -o ./target/wasm-gc/release/build/.mooncakes/lijunchen/mooncake/lib/lib.core -pkg lijunchen/mooncake/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources lijunchen/mooncake/lib:./mods/lijunchen/mooncake/lib -target wasm-gc
            moonc build-package ./mods/lijunchen/mooncake2/src/lib/hello.mbt -w -a -alert -all -o ./target/wasm-gc/release/build/.mooncakes/lijunchen/mooncake2/lib/lib.core -pkg lijunchen/mooncake2/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources lijunchen/mooncake2/lib:./mods/lijunchen/mooncake2/src/lib -target wasm-gc
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg hello31/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources hello31/lib:./lib -target wasm-gc
            moonc build-package ./mods/lijunchen/mooncake/top.mbt -w -a -alert -all -o ./target/wasm-gc/release/build/.mooncakes/lijunchen/mooncake/mooncake.core -pkg lijunchen/mooncake -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/.mooncakes/lijunchen/mooncake/lib/lib.mi:lib -pkg-sources lijunchen/mooncake:./mods/lijunchen/mooncake -target wasm-gc
            moonc build-package ./mods/lijunchen/mooncake2/src/top.mbt -w -a -alert -all -o ./target/wasm-gc/release/build/.mooncakes/lijunchen/mooncake2/mooncake2.core -pkg lijunchen/mooncake2 -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/.mooncakes/lijunchen/mooncake2/lib/lib.mi:lib -pkg-sources lijunchen/mooncake2:./mods/lijunchen/mooncake2/src -target wasm-gc
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg hello31/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/lib/lib.mi:lib -i ./target/wasm-gc/release/build/.mooncakes/lijunchen/mooncake/mooncake.mi:mooncake -i ./target/wasm-gc/release/build/.mooncakes/lijunchen/mooncake2/mooncake2.mi:mooncake2 -pkg-sources hello31/main:./main -target wasm-gc
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/.mooncakes/lijunchen/mooncake/lib/lib.core ./target/wasm-gc/release/build/.mooncakes/lijunchen/mooncake/mooncake.core ./target/wasm-gc/release/build/.mooncakes/lijunchen/mooncake2/lib/lib.core ./target/wasm-gc/release/build/.mooncakes/lijunchen/mooncake2/mooncake2.core ./target/wasm-gc/release/build/main/main.core -main hello31/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello31/lib:./lib -pkg-sources lijunchen/mooncake/lib:./mods/lijunchen/mooncake/lib -pkg-sources lijunchen/mooncake:./mods/lijunchen/mooncake -pkg-sources lijunchen/mooncake2/lib:./mods/lijunchen/mooncake2/src/lib -pkg-sources lijunchen/mooncake2:./mods/lijunchen/mooncake2/src -pkg-sources hello31/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc
        "#]],
    );
    assert!(out.contains("lijunchen/mooncake:./mods/lijunchen/mooncake"));
    assert!(out.contains("lijunchen/mooncake2:./mods/lijunchen/mooncake2/src"));
}
#[test]
fn test_moon_test_no_entry_warning() {
    let dir = TestDir::new("moon_test/no_entry_warning");

    let out = snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["test"])
        .assert()
        .success()
        .get_output()
        .stderr
        .to_owned();

    check(
        std::str::from_utf8(&out).unwrap(),
        expect![[r#"
            Warning: no test entry found.
        "#]],
    );
}

#[test]
#[ignore]
fn test_generate_test_driver_incremental() {
    let dir = TestDir::new("moon_test/hello_lib");

    get_stdout(&dir, ["test", "--package", "moonbitlang/hello/lib"]);
    let driver_file =
        dir.join("target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt");
    assert!(driver_file.exists());

    let time_1 = driver_file.metadata().unwrap().modified().unwrap();

    get_stdout(
        &dir,
        [
            "test",
            "--package",
            "moonbitlang/hello/lib",
            "--file",
            "hello_wbtest.mbt",
        ],
    );
    let time_2 = driver_file.metadata().unwrap().modified().unwrap();

    assert!(time_1 == time_2);

    get_stdout(
        &dir,
        [
            "test",
            "--package",
            "moonbitlang/hello/lib",
            "--file",
            "hello_wbtest.mbt",
            "--index",
            "0",
        ],
    );
    let time_3 = driver_file.metadata().unwrap().modified().unwrap();

    assert!(time_2 == time_3);

    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(dir.join("lib/hello.mbt"))
        .unwrap();
    file.write_all(b"\n").unwrap();

    get_stdout(
        &dir,
        [
            "test",
            "--package",
            "moonbitlang/hello/lib",
            "--file",
            "hello_wbtest.mbt",
            "--index",
            "0",
        ],
    );
    let time_4 = driver_file.metadata().unwrap().modified().unwrap();

    assert!(time_3 != time_4);
}
