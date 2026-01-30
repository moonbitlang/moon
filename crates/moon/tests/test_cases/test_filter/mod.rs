mod include_skip_test;
mod single_test_skip;
mod skip_test;

use super::*;

#[test]
fn test_moon_test_filter_by_name() {
    let dir = TestDir::new("test_filter/test_filter");

    // Filter tests matching "A" - should only run test named "A"
    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/A",
                "--filter",
                "A",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            test A
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    // Filter tests matching "hello_*" - should run hello_0, hello_1, hello_2
    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/A",
                "--filter",
                "hello_*",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            test hello_0
            test hello_1
            test hello_2
            Total tests: 3, passed: 3, failed: 0.
        "#]],
    );

    // Filter tests matching "*_1" - should only run hello_1
    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/A",
                "--filter",
                "*_1",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            test hello_1
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_test_filter_by_name_with_question_mark() {
    let dir = TestDir::new("test_filter/test_filter");

    // Filter tests matching "hello_?" - should run hello_0, hello_1, hello_2
    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/A",
                "--filter",
                "hello_?",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            test hello_0
            test hello_1
            test hello_2
            Total tests: 3, passed: 3, failed: 0.
        "#]],
    );

    // Filter tests matching "?" - should match single character names like "A" and "B"
    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/A",
                "-f",
                "hello.mbt",
                "--filter",
                "?",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            test A
            test B
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_test_filter_by_name_no_match() {
    let dir = TestDir::new("test_filter/test_filter");

    // Filter with pattern that matches nothing
    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/A",
                "--filter",
                "nonexistent*",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            Total tests: 0, passed: 0, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_test_filter_by_name_combined_with_file() {
    let dir = TestDir::new("test_filter/test_filter");

    // Filter by file and name pattern
    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/A",
                "-f",
                "hello.mbt",
                "--filter",
                "A",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            test A
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_test_filter_package() {
    let dir = TestDir::new("test_filter/test_filter");

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/A",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            test C
            test D
            test A
            test B
            test hello_0
            test hello_1
            test hello_2
            Total tests: 7, passed: 7, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/lib",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            test A
            test hello_0
            test hello_1
            Total tests: 3, passed: 3, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_test_filter_multi_package() {
    let dir = TestDir::new("test_filter/pkg_with_test_imports");

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/lib",
                "username/hello/lib1",
                "username/hello/lib2",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            Hello from lib1

            Hello from lib2

            Hello from lib7
            Hello from lib3

            Hello from lib4
            Total tests: 4, passed: 4, failed: 0.
        "#]],
    );

    // Note: Previously there were tests that looked like this:
    // `moon test -p a b c --file file.mbt --index i`
    // which means to select, separately, the `i`th test in `a/file.mbt`,
    // `b/file.mbt`, `c/file.mbt`.
    //
    // Since this usage is too cursed, RR banned it so one can only specify
    // files when a single package is selected. Thus, these tests are removed.
}

#[test]
fn test_moon_test_filter_package_with_singlefile() {
    let dir = TestDir::new("test_filter/test_filter");

    check(
        get_stdout(&dir, ["test", "A/hello.mbt"]),
        expect![[r#"
            test A
            test B
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );

    check(
        get_stdout(&dir, ["test", "A/hello_wbtest.mbt"]),
        expect![[r#"
            test hello_0
            test hello_1
            test hello_2
            Total tests: 3, passed: 3, failed: 0.
        "#]],
    );

    check(
        get_stdout(&dir.join("A"), ["test", "hello_wbtest.mbt"]),
        expect![[r#"
            test hello_0
            test hello_1
            test hello_2
            Total tests: 3, passed: 3, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_test_filter_package_with_folder() {
    let dir = TestDir::new("test_filter/test_filter");

    check(
        get_stdout(&dir, ["test", "A", "--sort-input", "--no-parallelize"]),
        expect![[r#"
            test C
            test D
            test A
            test B
            test hello_0
            test hello_1
            test hello_2
            Total tests: 7, passed: 7, failed: 0.
        "#]],
    );

    check(
        get_stdout(&dir, ["test", "A/", "--sort-input", "--no-parallelize"]),
        expect![[r#"
            test C
            test D
            test A
            test B
            test hello_0
            test hello_1
            test hello_2
            Total tests: 7, passed: 7, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir.join("A"),
            ["test", ".", "--sort-input", "--no-parallelize"],
        ),
        expect![[r#"
            test C
            test D
            test A
            test B
            test hello_0
            test hello_1
            test hello_2
            Total tests: 7, passed: 7, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_test_filter_package_with_deps() {
    let dir = TestDir::new("test_filter/pkg_with_deps");

    check(
        get_stdout(
            &dir,
            ["test", "-p", "username/hello/lib", "--no-parallelize"],
        ),
        expect![[r#"
            Hello from lib1
            Hello from lib2
            Hello from lib4

            Hello from lib3
            Hello from lib4


            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            ["test", "-p", "username/hello/lib2", "--no-parallelize"],
        ),
        expect![[r#"
            Hello from lib2
            Hello from lib4

            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            ["test", "-p", "username/hello/lib4", "--no-parallelize"],
        ),
        expect![[r#"
            Hello from lib4
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_test_filter_package_with_test_imports() {
    let dir = TestDir::new("test_filter/pkg_with_test_imports");

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/lib",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            Hello from lib1

            Hello from lib2

            Hello from lib7
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/lib1",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            Hello from lib3

            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/lib2",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            Hello from lib4
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/lib3",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            Hello from lib3

            Hello from lib7
            Hello from lib6
            Total tests: 3, passed: 3, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/lib4",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            Hello from lib5
            Hello from lib5
            Hello from lib7
            Total tests: 3, passed: 3, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/lib5",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            Hello from lib5
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/lib6",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            Hello from lib6
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/lib7",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            Hello from lib7
            Hello from lib6
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_parallelism() {
    let dir = TestDir::new("test_filter/pkg_with_test_imports");

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-j1",
                "-p",
                "username/hello/lib",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            Hello from lib1

            Hello from lib2

            Hello from lib7
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-j1",
                "-p",
                "username/hello/lib1",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            Hello from lib3

            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-j1",
                "-p",
                "username/hello/lib2",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            Hello from lib4
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-j1",
                "-p",
                "username/hello/lib3",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            Hello from lib3

            Hello from lib7
            Hello from lib6
            Total tests: 3, passed: 3, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-j1",
                "-p",
                "username/hello/lib4",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            Hello from lib5
            Hello from lib5
            Hello from lib7
            Total tests: 3, passed: 3, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-j1",
                "-p",
                "username/hello/lib5",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            Hello from lib5
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-j1",
                "-p",
                "username/hello/lib6",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            Hello from lib6
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-j1",
                "-p",
                "username/hello/lib7",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"
            Hello from lib7
            Hello from lib6
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_test_filter_package_dry_run() {
    let dir = TestDir::new("test_filter/test_filter");

    let graph_file = dir.join("test_graph_filter_a.jsonl");
    snap_dry_run_graph(
        &dir,
        [
            "test",
            "-p",
            "username/hello/A",
            "--dry-run",
            "--sort-input",
        ],
        &graph_file,
    );
    compare_graphs(
        &graph_file,
        expect_file!["test_filter_dry_run_filter_a.jsonl.snap"],
    );

    let graph_file = dir.join("test_graph_no_filter.jsonl");
    snap_dry_run_graph(&dir, ["test", "--dry-run", "--sort-input"], &graph_file);
    compare_graphs(
        &graph_file,
        expect_file!["test_filter_dry_run_no_filter.jsonl.snap"],
    );
}

#[test]
fn test_moon_test_filter_file() {
    let dir = TestDir::new("test_filter/test_filter");

    check(
        get_stdout(&dir, ["test", "-p", "username/hello/A", "-f", "hello.mbt"]),
        expect![[r#"
            test A
            test B
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            ["test", "-p", "username/hello/lib", "-f", "hello_wbtest.mbt"],
        ),
        expect![[r#"
            test hello_0
            test hello_1
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_test_filter_file_index_with_path_arg() {
    let dir = TestDir::new("test_filter/test_filter");

    // Path argument form from module root
    check(
        get_stdout(&dir, ["test", "A/hello.mbt", "-i", "1"]),
        expect![[r#"
            test B
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    // Path argument form from inside package directory
    check(
        get_stdout(&dir.join("A"), ["test", "hello.mbt", "-i", "1"]),
        expect![[r#"
            test B
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_test_filter_index() {
    let dir = TestDir::new("test_filter/test_filter");

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/A",
                "-f",
                "hello.mbt",
                "-i",
                "1",
            ],
        ),
        expect![[r#"
            test B
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/lib",
                "-f",
                "hello_wbtest.mbt",
                "-i",
                "0",
            ],
        ),
        expect![[r#"
            test hello_0
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_test_filter_index_range() {
    let dir = TestDir::new("test_filter/test_filter");

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/A",
                "-f",
                "hello.mbt",
                "-i",
                "0-2",
            ],
        ),
        expect![[r#"
            test A
            test B
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_test_filter_index_with_auto_update() {
    let dir = TestDir::new("test_filter/test_filter");

    let _ = get_stdout(
        &dir,
        [
            "test",
            "-p",
            "username/hello/lib2",
            "-f",
            "lib.mbt",
            "-i",
            "1",
            "-u",
            "--no-parallelize",
        ],
    );
    check(
        read(dir.join("lib2").join("lib.mbt")),
        expect![[r#"
            test {
              println(2)
            }

            test {
              inspect(1, content=(#|1
              ))
              inspect(1 + 2, content=(#|3
              ))
              inspect("hello", content=(#|hello
              ))
              inspect([1, 2, 3], content=(#|[1, 2, 3]
              ))
            }

            test {
              inspect(2)
            }
        "#]],
    );

    let dir = TestDir::new("test_filter/test_filter");
    let _ = get_err_stderr(
        &dir,
        [
            "test",
            "-p",
            "username/hello/lib2",
            "-f",
            "lib.mbt",
            "-i",
            "1",
            "-u",
            "-l",
            "2",
            "--no-parallelize",
        ],
    );
    check(
        read(dir.join("lib2").join("lib.mbt")),
        expect![[r#"
            test {
              println(2)
            }

            test {
              inspect(1, content=(#|1
              ))
              inspect(1 + 2, content=(#|3
              ))
              inspect("hello")
              inspect([1, 2, 3])
            }

            test {
              inspect(2)
            }
        "#]],
    );

    let dir = TestDir::new("test_filter/test_filter");
    let _ = get_err_stderr(
        &dir,
        [
            "test",
            "-p",
            "username/hello/lib2",
            "-f",
            "lib.mbt",
            "-u",
            "-l",
            "1",
            "--no-parallelize",
        ],
    );
    check(
        read(dir.join("lib2").join("lib.mbt")),
        expect![[r#"
            test {
              println(2)
            }

            test {
              inspect(1, content=(#|1
              ))
              inspect(1 + 2)
              inspect("hello")
              inspect([1, 2, 3])
            }

            test {
              inspect(2, content=(#|2
              ))
            }
        "#]],
    );
}

#[test]
fn moon_test_parallelize_should_success() {
    let dir = TestDir::new("test_filter/pkg_with_test_imports");

    let output = get_stdout(&dir, ["test"]);
    assert!(output.contains("Total tests: 14, passed: 14, failed: 0."));

    let output = get_stdout(&dir, ["test", "--target", "native"]);
    assert!(output.contains("Total tests: 14, passed: 14, failed: 0."));
}

#[test]
fn moon_test_parallelize_test_filter_should_success() {
    let dir = TestDir::new("test_filter/test_filter");

    let output = get_err_stdout(&dir, ["test"]);
    assert!(output.contains("Total tests: 13, passed: 11, failed: 2."));

    let output = get_err_stdout(&dir, ["test", "--target", "native"]);
    assert!(output.contains("Total tests: 13, passed: 11, failed: 2."));

    let output = get_stdout(&dir, ["test", "-u", "--no-parallelize"]);
    assert!(output.contains("Total tests: 13, passed: 13, failed: 0."));

    let output = get_stdout(
        &dir,
        ["test", "-u", "--no-parallelize", "--target", "native"],
    );
    assert!(output.contains("Total tests: 13, passed: 13, failed: 0."));
}
