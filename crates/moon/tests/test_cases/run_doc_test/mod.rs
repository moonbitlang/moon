use super::*;

#[test]
fn test_run_doc_test() {
    let dir = TestDir::new("run_doc_test.in");

    // `moon test --doc` run doc test only
    check(
        get_err_stdout(&dir, ["test", "--sort-input", "--doc"]),
        expect![[r#"
            hello from hello_test.mbt
            doc_test 1 from hello.mbt
            doc_test 2 from hello.mbt
            doc_test 3 from hello.mbt
            doc_test
            doc_test 1 from greet.mbt
            test block 1
            test block 2
            test block 3
            doc_test 3 from greet.mbt
            test block 4
            test block 5
            doc_test 5 from greet.mbt
            [username/hello] test lib/hello.mbt:12 (#1) failed
            expect test failed at $ROOT/src/lib/hello.mbt:15:5-15:18
            Diff: (- expected, + actual)
            ----
            +1256
            ----

            [username/hello] test lib/hello.mbt:24 (#2) failed: src/lib/hello.mbt:27:5-27:30@username/hello FAILED: this is a failure
            [username/hello] test lib/greet.mbt:23 (#2) failed
            expect test failed at $ROOT/src/lib/greet.mbt:28:7-28:20
            Diff: (- expected, + actual)
            ----
            +1256
            ----

            [username/hello] test lib/greet.mbt:37 (#3) failed: src/lib/greet.mbt:41:7-41:30@username/hello FAILED: another failure
            [username/hello] test lib/greet.mbt:112 (#8) failed
            expect test failed at $ROOT/src/lib/greet.mbt:116:5-116:40
            Diff: (- expected, + actual)
            ----
            +b"T/x00e/x00s/x00t/x00"
            ----

            Total tests: 16, passed: 11, failed: 5.
        "#]],
    );

    let _ = get_err_stdout(&dir, ["test", "--sort-input", "--update"]);
    let hello_mbt = read(dir.join("src/lib/hello.mbt"));
    let hello_content = hello_mbt.lines().collect::<Vec<_>>();
    let greet_mbt = read(dir.join("src/lib/greet.mbt"));
    let greet_content = greet_mbt.lines().collect::<Vec<_>>();
    check(
        hello_content[14..16].join("\n"),
        expect![[r#"
            /// inspect(1256, content=(
            ///   #|1256"#]],
    );

    check(
        greet_content[27..29].join("\n"),
        expect![[r#"
            ///   inspect(1256, content=(
            ///   #|1256"#]],
    );

    check(
        greet_content[117..119].join("\n"),
        expect![[r#"
            /// inspect(buf.contents(), content=(
            ///   #|b"T\x00e\x00s\x00t\x00""#]],
    );

    check(
        get_err_stdout(&dir, ["test", "--sort-input"]),
        expect![[r#"
            hello from hello_test.mbt
            doc_test 1 from hello.mbt
            doc_test 2 from hello.mbt
            doc_test 3 from hello.mbt
            doc_test
            doc_test 1 from greet.mbt
            test block 1
            test block 2
            test block 3
            doc_test 3 from greet.mbt
            test block 4
            test block 5
            doc_test 5 from greet.mbt
            [username/hello] test lib/hello.mbt:26 (#2) failed: src/lib/hello.mbt:29:5-29:30@username/hello FAILED: this is a failure
            [username/hello] test lib/greet.mbt:39 (#3) failed: src/lib/greet.mbt:43:7-43:30@username/hello FAILED: another failure
            Total tests: 16, passed: 14, failed: 2.
        "#]],
    );
}
