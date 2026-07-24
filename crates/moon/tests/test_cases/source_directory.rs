// moon: The build system and package manager for MoonBit.
// Copyright (C) 2024 International Digital Economy Academy
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

use super::*;

#[test]
fn test_specify_source_dir_003() {
    let dir = TestDir::new("specify_source_dir_003_empty_string.in");
    check(
        get_stdout(&dir, ["check"]),
        expect![[r#"
            Finished. moon: ran 2 tasks, now up to date
        "#]],
    );
}

#[test]
fn test_specify_source_dir_004() {
    let dir = TestDir::new("specify_source_dir_004.in");
    check(
        get_stdout(&dir, ["check"]),
        expect![[r#"
            Finished. moon: ran 4 tasks, now up to date
        "#]],
    );

    get_stdout(&dir, ["clean"]);
    check(
        get_stdout(
            &dir,
            ["run", "nes/t/ed/src/main", "--target", "js", "--build-only"],
        ),
        expect![[r#"
            {"artifacts_path":["$ROOT/_build/js/debug/build/main/main.js"]}
        "#]],
    );
    assert!(dir.join("_build/js/debug/build/main/main.js").exists());

    check(
        get_stdout(&dir, ["run", "nes/t/ed/src/main"]),
        expect![[r#"
            Hello, world!
        "#]],
    );
}

#[test]
fn test_specify_source_dir_005() {
    let dir = TestDir::new("specify_source_dir_005_bad.in");
    let check_stderr = get_err_stderr(&dir, ["check"]);
    assert!(check_stderr.contains("`source` not a subdirectory of the parent directory"));
}

#[test]
fn test_specify_source_dir_with_deps() {
    let dir = TestDir::new("specify_source_dir_with_deps_001.in");
    assert_dry_run_graph(
        &dir,
        ["check", "--target", "wasm-gc", "--dry-run", "--sort-input"],
        expect_file!["./specify_source_dir_with_deps_001.in/check_graph.jsonl.snap"],
    );
    assert_dry_run_graph(
        &dir,
        ["test", "--target", "wasm-gc", "--dry-run", "--sort-input"],
        expect_file!["./specify_source_dir_with_deps_001.in/test_graph.jsonl.snap"],
    );

    check(
        get_stdout(&dir, ["check"]),
        expect![[r#"
            Finished. moon: ran 6 tasks, now up to date
        "#]],
    );
    check(
        get_stdout(&dir, ["build"]),
        expect![[r#"
            Finished. moon: ran 5 tasks, now up to date
        "#]],
    );
    check(
        get_stdout(&dir, ["test"]),
        expect![[r#"
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    check(
        get_stdout(&dir, ["run", "./anyhow/main"]),
        expect![[r#"
            Hello, world!
            hello
            world
        "#]],
    );
}

#[test]
fn test_specify_source_dir_with_deps_002() {
    let dir = TestDir::new("specify_source_dir_with_deps_002.in");
    check(
        get_stderr(&dir, ["check"]),
        expect![[r#"
            Warning: Duplicate alias `lib` at "$ROOT/deps/hello004/lib/moon.pkg.json". "test-import" will automatically add "import" and current package as dependency so you don't need to add it manually. If you're test-importing a dependency with the same default alias as your current package, considering give it a different alias than the current package. Violating import: `just/hello003/lib`
            Warning: Duplicate alias `lib` at "$ROOT/deps/hello003/source003/lib/moon.pkg.json". "test-import" will automatically add "import" and current package as dependency so you don't need to add it manually. If you're test-importing a dependency with the same default alias as your current package, considering give it a different alias than the current package. Violating import: `just/hello002/lib`
            Warning: Duplicate alias `lib` at "$ROOT/deps/hello002/lib/moon.pkg.json". "test-import" will automatically add "import" and current package as dependency so you don't need to add it manually. If you're test-importing a dependency with the same default alias as your current package, considering give it a different alias than the current package. Violating import: `just/hello001/lib`
        "#]],
    );
    check(
        get_stderr(&dir, ["build"]),
        expect![[r#"
            Warning: Duplicate alias `lib` at "$ROOT/deps/hello004/lib/moon.pkg.json". "test-import" will automatically add "import" and current package as dependency so you don't need to add it manually. If you're test-importing a dependency with the same default alias as your current package, considering give it a different alias than the current package. Violating import: `just/hello003/lib`
            Warning: Duplicate alias `lib` at "$ROOT/deps/hello003/source003/lib/moon.pkg.json". "test-import" will automatically add "import" and current package as dependency so you don't need to add it manually. If you're test-importing a dependency with the same default alias as your current package, considering give it a different alias than the current package. Violating import: `just/hello002/lib`
            Warning: Duplicate alias `lib` at "$ROOT/deps/hello002/lib/moon.pkg.json". "test-import" will automatically add "import" and current package as dependency so you don't need to add it manually. If you're test-importing a dependency with the same default alias as your current package, considering give it a different alias than the current package. Violating import: `just/hello001/lib`
        "#]],
    );
    check(
        get_stdout(&dir, ["test"]),
        expect![[r#"
        Total tests: 0, passed: 0, failed: 0.
    "#]],
    );
    check(
        get_stdout(&dir, ["run", "./anyhow"]),
        expect![[r#"
            a!b!c!d!
            one!two!three!four!
        "#]],
    );
}
