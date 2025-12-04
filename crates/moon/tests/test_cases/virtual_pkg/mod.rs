use super::*;

#[test]
fn test_virtual_pkg() {
    let dir = TestDir::new("virtual_pkg.in");

    let virtual_pkg = dir.join("virtual");

    check(
        get_stdout(&virtual_pkg, ["run", "main", "--dry-run"]),
        expect![[r#"
            moonc build-interface ./lib3/pkg.mbti -o ./target/wasm-gc/release/build/lib3/lib3.mi -pkg username/hello/lib3 -pkg-sources username/hello/lib3:./lib3 -virtual -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -error-format json
            moonc build-interface ./lib1/pkg.mbti -o ./target/wasm-gc/release/build/lib1/lib1.mi -pkg username/hello/lib1 -pkg-sources username/hello/lib1:./lib1 -virtual -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -error-format json
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/release/build/lib1/lib1.mi:lib1 -i ./target/wasm-gc/release/build/lib3/lib3.mi:lib3 -pkg-sources username/hello/main:./main -target wasm-gc -workspace-path . -all-pkgs ./target/wasm-gc/release/build/all_pkgs.json
            moonc build-package ./lib4/hello.mbt -o ./target/wasm-gc/release/build/lib4/lib4.core -pkg username/hello/lib4 -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -pkg-sources username/hello/lib4:./lib4 -target wasm-gc -check-mi ./target/wasm-gc/release/build/lib3/lib3.mi -impl-virtual -pkg-sources username/hello/lib3:./lib3 -no-mi -workspace-path . -all-pkgs ./target/wasm-gc/release/build/all_pkgs.json
            moonc build-package ./dummy_lib/hello.mbt -o ./target/wasm-gc/release/build/dummy_lib/dummy_lib.core -pkg username/hello/dummy_lib -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -pkg-sources username/hello/dummy_lib:./dummy_lib -target wasm-gc -workspace-path . -all-pkgs ./target/wasm-gc/release/build/all_pkgs.json
            moonc build-package ./lib2/hello.mbt -o ./target/wasm-gc/release/build/lib2/lib2.core -pkg username/hello/lib2 -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/release/build/dummy_lib/dummy_lib.mi:dummy_lib -pkg-sources username/hello/lib2:./lib2 -target wasm-gc -check-mi ./target/wasm-gc/release/build/lib1/lib1.mi -impl-virtual -pkg-sources username/hello/lib1:./lib1 -no-mi -workspace-path . -all-pkgs ./target/wasm-gc/release/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core' ./target/wasm-gc/release/build/dummy_lib/dummy_lib.core ./target/wasm-gc/release/build/lib2/lib2.core ./target/wasm-gc/release/build/lib4/lib4.core ./target/wasm-gc/release/build/main/main.core -main username/hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/dummy_lib:./dummy_lib -pkg-sources username/hello/lib2:./lib2 -pkg-sources username/hello/lib4:./lib4 -pkg-sources username/hello/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm-gc
            moonrun ./target/wasm-gc/release/build/main/main.wasm --
        "#]],
    );
    check(
        get_stdout(&virtual_pkg, ["run", "main"]),
        expect![[r#"
            another impl for f1 in lib2: 1
            another impl for f2 in lib2: 2
            another impl for f3 in lib4
        "#]],
    );
    check(
        get_stdout(&virtual_pkg, ["test", "--no-parallelize"]),
        expect![[r#"
            bb test
            default impl for f1 in lib1: 1
            another impl for f3 in lib4
            internal test
            default impl for f1 in lib1: 1
            another impl for f3 in lib4
            wb test
            default impl for f1 in lib1: 1
            another impl for f3 in lib4
            Total tests: 3, passed: 3, failed: 0.
        "#]],
    );

    let user = dir.join("user");
    check(
        get_stdout(&user, ["run", "main"]),
        expect![[r#"
            user impl for f1 in lib: 1
            user impl for f2 in lib: 2
            another impl for f3 in lib4
        "#]],
    );
    check(
        get_err_stdout(&user, ["test", "--no-parallelize"])
            .lines()
            .take(10)
            .collect::<Vec<_>>()
            .join("\n")
            + "\n",
        expect![[r#"
            bb test
            default impl for f1 in lib1: 1
            another impl for f3 in lib4
            internal test
            default impl for f1 in lib1: 1
            another impl for f3 in lib4
            wb test
            default impl for f1 in lib1: 1
            another impl for f3 in lib4
            [username/xxx] test lib2/hello_test.mbt:2 (#0) failed: Error
        "#]],
    );
}

#[test]
fn test_virtual_pkg_err() {
    let dir = TestDir::new("virtual_pkg.in");

    let err = dir.join("err");
    let content = get_err_stderr(&err, ["check"]);
    println!("Error output:\n{}", content);
    assert!(content.contains("$ROOT/lib1/pkg.mbti:5:1"));
    assert!(content.contains("$ROOT/lib1/pkg.mbti:3:1"));

    // moon build will not build default impl for lib1 if no pkg depend on this default impl
    // so here just report error for missing impl for f2(diy impl in lib2), no report error for missing impl for f1(default impl in lib1)
    check(
        get_err_stderr(&err, ["build"]),
        expect![[r#"
            Error: [4159]
               ╭─[ $ROOT/lib1/pkg.mbti:5:1 ]
               │
             5 │ fn f2(String) -> Unit
               │ ──────────┬──────────  
               │           ╰──────────── Missing implementation for function f2.
            ───╯
            Failed with 0 warnings, 1 errors.
            error: failed when building project
        "#]],
    );
}
