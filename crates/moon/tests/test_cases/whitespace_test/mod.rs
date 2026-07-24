use super::*;
use crate::build_graph::compare_graphs;
use expect_test::expect_file;
#[test]
fn whitespace_test() {
    let dir = TestDir::new("whitespace_test.in");
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["clean"])
        .assert()
        .success();

    // unstable test
    // check(
    //     &get_stdout_with_args(&dir, ["check", "--dry-run", "--nostd"]),
    //     expect![[r#"
    //         moonc check './main lib/hello.mbt' './main lib/hello_test.mbt' -o './_build/check/main lib/main lib.underscore_test.mi' -pkg 'username/hello/main lib' -pkg-sources 'username/hello/main lib:./main lib'
    //         moonc check './main lib/hello.mbt' -o './_build/check/main lib/main lib.mi' -pkg 'username/hello/main lib' -pkg-sources 'username/hello/main lib:./main lib'
    //         moonc check './main exe/main.mbt' -o './_build/check/main exe/main exe.mi' -pkg 'username/hello/main exe' -is-main -i './_build/check/main lib/main lib.mi:lib' -pkg-sources 'username/hello/main exe:./main exe'
    //     "#]],
    // );
    let build_graph = dir.join("build_graph.jsonl");
    snap_dry_run_graph(
        &dir,
        ["build", "--target", "wasm-gc", "--dry-run", "--nostd"],
        &build_graph,
    );
    compare_graphs(
        &build_graph,
        expect_file!["../whitespace_test.in/build_graph.jsonl.snap"],
    );

    check(
        get_stdout(&dir, ["run", "--target", "wasm-gc", "main exe"]),
        expect![[r#"
            Hello, world!
        "#]],
    );

    let out = get_stdout(&dir, ["check", "--target", "wasm-gc"]);
    expect![[r#"
        Finished. moon: ran 5 tasks, now up to date
    "#]]
    .assert_eq(&out);
}

#[test]
fn test_whitespace_parent_space() -> anyhow::Result<()> {
    let tmp_dir = tempfile::TempDir::new()?;
    let path_with_space = tmp_dir.path().join("with space");
    std::fs::create_dir_all(&path_with_space)?;
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/test_cases")
        .join("whitespace_test.in");
    copy(&dir, &path_with_space)?;

    let canon = dunce::canonicalize(tmp_dir.path())?;
    let prefix = canon.as_path().display().to_string().replace('\\', "/");

    let build_graph = path_with_space.join("build_graph.jsonl");
    snap_dry_run_graph(
        &path_with_space,
        [
            "build",
            "--target",
            "wasm-gc",
            "--no-render",
            "--sort-input",
            "--dry-run",
        ],
        &build_graph,
    );
    compare_graphs(
        &build_graph,
        expect_file!["../whitespace_test.in/parent_space_build_graph.jsonl.snap"],
    );

    let out = get_stdout(
        &path_with_space,
        ["build", "--target", "wasm-gc", "--no-render"],
    );
    let out = out.replace(&prefix, ".");
    let out = out.replace(
        &moonutil::toolchain::home()
            .to_str()
            .unwrap()
            .replace('\\', "/"),
        "$MOON_HOME",
    );

    copy(&dir, &path_with_space)?;
    check(
        &out,
        expect![[r#"
            Finished. moon: ran 3 tasks, now up to date
        "#]],
    );
    Ok(())
}
