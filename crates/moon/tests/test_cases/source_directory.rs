// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

use super::*;

#[test]
fn test_specify_source_dir_003() {
    let dir = TestDir::new("specify_source_dir_003_empty_string.in");
    check(
        get_stderr(&dir, ["check"]),
        expect![[r#"
            Warning: `moon.mod.json` at '$ROOT' is deprecated. Run `moon fmt` to migrate to `moon.mod`.
            Finished. moon: ran 2 tasks, now up to date
        "#]],
    );
}

#[test]
fn test_specify_source_dir_004() {
    let dir = TestDir::new("specify_source_dir_004.in");
    check(
        get_stderr(&dir, ["check"]),
        expect![[r#"
            Warning: `moon.mod.json` at '$ROOT' is deprecated. Run `moon fmt` to migrate to `moon.mod`.
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
    build_graph::assert(
        moon_cmd(&dir).args(["check", "--target", "wasm-gc", "--dry-run", "--sort-input"]),
        expect_file!["./specify_source_dir_with_deps_001.in/check_graph.jsonl.snap"],
    );
    build_graph::assert(
        moon_cmd(&dir).args(["test", "--target", "wasm-gc", "--dry-run", "--sort-input"]),
        expect_file!["./specify_source_dir_with_deps_001.in/test_graph.jsonl.snap"],
    );

    check(
        get_stderr(&dir, ["check"]),
        expect![[r#"
            Warning: `moon.mod.json` at '$ROOT' is deprecated. Run `moon fmt` to migrate to `moon.mod`.
            Warning: `moon.mod.json` at '$ROOT/deps/hello19' is deprecated. Run `moon fmt` to migrate to `moon.mod`.
            Finished. moon: ran 6 tasks, now up to date
        "#]],
    );
    check(
        get_stderr(&dir, ["build"]),
        expect![[r#"
            Warning: `moon.mod.json` at '$ROOT' is deprecated. Run `moon fmt` to migrate to `moon.mod`.
            Warning: `moon.mod.json` at '$ROOT/deps/hello19' is deprecated. Run `moon fmt` to migrate to `moon.mod`.
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
            Warning: `moon.mod.json` at '$ROOT' is deprecated. Run `moon fmt` to migrate to `moon.mod`.
            Warning: `moon.mod.json` at '$ROOT/deps/hello004' is deprecated. Run `moon fmt` to migrate to `moon.mod`.
            Warning: `moon.mod.json` at '$ROOT/deps/hello003' is deprecated. Run `moon fmt` to migrate to `moon.mod`.
            Warning: `moon.mod.json` at '$ROOT/deps/hello002' is deprecated. Run `moon fmt` to migrate to `moon.mod`.
            Warning: `moon.mod.json` at '$ROOT/deps/hello001' is deprecated. Run `moon fmt` to migrate to `moon.mod`.
            Warning: Duplicate alias `lib` at "$ROOT/deps/hello004/lib/moon.pkg.json". "test-import" will automatically add "import" and current package as dependency so you don't need to add it manually. If you're test-importing a dependency with the same default alias as your current package, considering give it a different alias than the current package. Violating import: `just/hello003/lib`
            Warning: Duplicate alias `lib` at "$ROOT/deps/hello003/source003/lib/moon.pkg.json". "test-import" will automatically add "import" and current package as dependency so you don't need to add it manually. If you're test-importing a dependency with the same default alias as your current package, considering give it a different alias than the current package. Violating import: `just/hello002/lib`
            Warning: Duplicate alias `lib` at "$ROOT/deps/hello002/lib/moon.pkg.json". "test-import" will automatically add "import" and current package as dependency so you don't need to add it manually. If you're test-importing a dependency with the same default alias as your current package, considering give it a different alias than the current package. Violating import: `just/hello001/lib`
            Finished. moon: ran 10 tasks, now up to date
        "#]],
    );
    check(
        get_stderr(&dir, ["build"]),
        expect![[r#"
            Warning: `moon.mod.json` at '$ROOT' is deprecated. Run `moon fmt` to migrate to `moon.mod`.
            Warning: `moon.mod.json` at '$ROOT/deps/hello004' is deprecated. Run `moon fmt` to migrate to `moon.mod`.
            Warning: `moon.mod.json` at '$ROOT/deps/hello003' is deprecated. Run `moon fmt` to migrate to `moon.mod`.
            Warning: `moon.mod.json` at '$ROOT/deps/hello002' is deprecated. Run `moon fmt` to migrate to `moon.mod`.
            Warning: `moon.mod.json` at '$ROOT/deps/hello001' is deprecated. Run `moon fmt` to migrate to `moon.mod`.
            Warning: Duplicate alias `lib` at "$ROOT/deps/hello004/lib/moon.pkg.json". "test-import" will automatically add "import" and current package as dependency so you don't need to add it manually. If you're test-importing a dependency with the same default alias as your current package, considering give it a different alias than the current package. Violating import: `just/hello003/lib`
            Warning: Duplicate alias `lib` at "$ROOT/deps/hello003/source003/lib/moon.pkg.json". "test-import" will automatically add "import" and current package as dependency so you don't need to add it manually. If you're test-importing a dependency with the same default alias as your current package, considering give it a different alias than the current package. Violating import: `just/hello002/lib`
            Warning: Duplicate alias `lib` at "$ROOT/deps/hello002/lib/moon.pkg.json". "test-import" will automatically add "import" and current package as dependency so you don't need to add it manually. If you're test-importing a dependency with the same default alias as your current package, considering give it a different alias than the current package. Violating import: `just/hello001/lib`
            Finished. moon: ran 10 tasks, now up to date
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
