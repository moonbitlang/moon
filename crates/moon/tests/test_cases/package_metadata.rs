use super::*;

#[test]
fn test_bad_version() {
    let dir = TestDir::new("general.in");
    let content = std::fs::read_to_string(dir.join("moon.mod.json")).unwrap();
    let mut moon_mod: MoonModJSON = serde_json::from_str(&content).unwrap();
    moon_mod.version = Some("0.0".to_string());
    std::fs::write(
        dir.join("moon.mod.json"),
        serde_json::to_string(&moon_mod).unwrap(),
    )
    .unwrap();

    let check_stderr = get_err_stderr(&dir, ["check"]);
    println!("{}", check_stderr);
    assert!(check_stderr.contains("`version` bad format"));
}

#[test]
#[ignore = "Supported backend check is extremely slow, temporarily disable it"]
fn test_supported_backends_in_pkg_json() {
    let dir = TestDir::new("supported_backends_in_pkg_json");
    let pkg1 = dir.join("pkg1.in");
    // let pkg2 = dir.join("pkg2.in");
    let pkg3 = dir.join("pkg3.in");

    check(
        get_err_stderr(&pkg1, ["build"]),
        expect![[r#"
            Error: cannot find a common supported backend for the deps chain: "username/hello1/main: [js, wasm-gc] -> username/hello1/lib: [native]"
        "#]],
    );

    // check(
    //     get_err_stderr(&pkg2, ["build"]),
    //     expect![[r#"
    //         error: deps chain: "username/hello2/main: [js, wasm-gc] -> username/hello2/lib: [js]" supports backends `[js]`, while the current target backend is wasm-gc
    //     "#]],
    // );

    check(
        get_err_stderr(&pkg3, ["check"]),
        expect![[r#"
            Error: cannot find a common supported backend for the deps chain: "username/hello/main: [wasm-gc] -> username/hello/lib: [js, llvm, native, wasm, wasm-gc] -> username/hello/lib1: [wasm-gc] -> username/hello/lib3: [wasm-gc] -> username/hello/lib7: [js, wasm]"
        "#]],
    );
}

#[test]
fn test_native_stub_in_pkg_json() {
    let dir = TestDir::new("native_stub.in");

    let native_1 = dir.join("native_1.in");
    let native_2 = dir.join("native_2.in");
    let native_3 = dir.join("native_3.in");

    check(
        get_stdout(&native_1, ["test", "--target", "native", "--sort-input"]),
        expect![[r#"
            Hello world from native_1/lib/stub.c!!!
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    check(
        get_stdout(&native_1, ["run", "main", "--target", "native"]),
        expect![[r#"
            Hello world from native_1/lib/stub.c!!!
        "#]],
    );

    check(
        get_stdout(&native_2, ["test", "--target", "native", "--sort-input"]),
        expect![[r#"
            Hello world from native_1/lib/stub.c!!!
            Hello world from native_2/libb/stub.c!!!
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    check(
        get_stdout(&native_2, ["run", "main", "--target", "native"]),
        expect![[r#"
            Hello world from native_2/libb/stub.c!!!
        "#]],
    );

    check(
        get_stdout(&native_3, ["test", "--target", "native", "--sort-input"]),
        expect![[r#"
            Hello world from native_1/lib/stub.c!!!
            Hello world from native_2/libb/stub.c!!!
            Hello world from native_3/libbb/stub.c!!!
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    check(
        get_stdout(&native_3, ["run", "main", "--target", "native"]),
        expect![[r#"
            Hello world from native_3/libbb/stub.c!!!
        "#]],
    );
}
