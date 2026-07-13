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
fn test_moon_doc_dry_run() {
    let dir = TestDir::new("moon_doc.in");
    check(
        get_stdout(&dir, ["doc", "--dry-run"]),
        expect![[r#"
            moonc check ./src/lib/hello.mbt -o ./_build/wasm-gc/debug/check/lib/lib.mi -pkg username/hello/lib -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./src/lib -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check ./src/main/main.mbt -o ./_build/wasm-gc/debug/check/main/main.mi -pkg username/hello/main -pkg-type executable -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/lib/lib.mi:lib -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main:./src/main -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moondoc . -o ./_build/doc -std-path '$MOON_HOME/lib/core' -packages-json ./_build/packages.json
        "#]],
    );
}

#[test]
fn test_moon_doc() {
    let dir = TestDir::new("moon_doc.in");
    let _ = get_stderr(&dir, ["doc"]);
    check(
        read(dir.join("_build/doc/username/hello/lib/members.md")),
        expect![[r#"
            # Documentation
            |Value|description|
            |---|---|
            |[hello](#hello)||

            ## hello

            ```moonbit
            :::source,username/hello/lib/hello.mbt,1:::fn hello() -> String
            ```

        "#]],
    );
    check(
        read(dir.join("_build/doc/username/hello/main/members.md")),
        expect!["# Documentation"],
    );
    check(
        read(dir.join("_build/doc/username/hello/_sidebar.md")),
        expect![[r#"
            - [username/hello](username/hello/)
            - **In this module**
              - [lib](username/hello/lib/members)
              - [main](username/hello/main/members)
            - **Dependencies**
              - [moonbitlang/core](moonbitlang/core/)"#]],
    );
}

#[test]
fn test_moonfmt() {
    let dir = TestDir::new("general.in");
    let oneline = r#"pub fn hello() -> String { "Hello, world!" }"#;

    std::fs::write(dir.join("src/lib/hello.mbt"), oneline).unwrap();

    let out = std::process::Command::new("moonfmt")
        .args(["./src/lib/hello.mbt"])
        .current_dir(&dir)
        .output()
        .unwrap();
    let out = String::from_utf8(out.stdout).unwrap().replace_crlf_to_lf();
    check(
        &out,
        expect![[r#"
            ///|
            pub fn hello() -> String {
              "Hello, world!"
            }
        "#]],
    );

    check(
        read(dir.join("src/lib/hello.mbt")),
        expect![[r#"pub fn hello() -> String { "Hello, world!" }"#]],
    );

    let out = std::process::Command::new("moonfmt")
        .args(["-i", "./src/lib/hello.mbt"])
        .current_dir(&dir)
        .output()
        .unwrap();
    let _ = String::from_utf8(out.stdout).unwrap();
    check(
        read(dir.join("src/lib/hello.mbt")),
        expect![[r#"
            ///|
            pub fn hello() -> String {
              "Hello, world!"
            }
        "#]],
    );

    std::fs::write(dir.join("src/lib/hello.mbt"), oneline).unwrap();
    check(
        read(dir.join("src/lib/hello.mbt")),
        expect![[r#"pub fn hello() -> String { "Hello, world!" }"#]],
    );

    let out = std::process::Command::new("moonfmt")
        .args(["-i", "./src/lib/hello.mbt", "-o", "./src/lib/hello.txt"])
        .current_dir(&dir)
        .output()
        .unwrap();
    let _ = String::from_utf8(out.stdout).unwrap();
    check(
        read(dir.join("src/lib/hello.mbt")),
        expect![[r#"
            ///|
            pub fn hello() -> String {
              "Hello, world!"
            }
        "#]],
    );
    check(
        read(dir.join("src/lib/hello.txt")),
        expect![[r#"
            ///|
            pub fn hello() -> String {
              "Hello, world!"
            }
        "#]],
    );
}

#[test]
fn test_diff_mbti() {
    let dir = TestDir::new("diff_mbti.in");
    let content = get_stdout(&dir, ["info", "--target", "all"]);
    assert!(content.contains("$ROOT/_build/wasm-gc/debug/check/lib/lib.mbti"));
    assert!(content.contains("$ROOT/_build/js/debug/check/lib/lib.mbti"));
    assert!(content.contains("-pub fn aaa() -> String"));
    assert!(content.contains("+pub fn a() -> String"));
    assert!(dir.join("src/lib").join(MBTI_GENERATED).exists());
}

#[test]
fn moon_info_specific_package() {
    let dir = TestDir::new("moon_new/plain");

    // exact match
    get_stdout(&dir, ["info", "--package", "moon_new/main"]);
    assert!(dir.join("main/").join(MBTI_GENERATED).exists());
    assert!(!dir.join("lib/").join(MBTI_GENERATED).exists());

    // fuzzy match
    get_stdout(&dir, ["info", "--package", "lib"]);
    assert!(dir.join("lib/").join(MBTI_GENERATED).exists());

    let content = get_err_stderr(&dir, ["info", "--package", "moon_new/does_not_exist"]);
    assert!(content.contains("package `moon_new/does_not_exist` not found, make sure you have spelled it correctly, e.g. `moonbitlang/core/hashmap`(exact match) or `hashmap`(fuzzy match)"));
}

#[test]
fn merge_doc_test_and_md_test() {
    let dir = TestDir::new("all_kind_test.in");

    let check_output = get_stderr(&dir, ["check"]);
    println!("CHECK OUTPUT:\n{}", check_output);

    assert!(check_output.contains("unused_in_lib_md_test"));
    assert!(check_output.contains("unused_in_lib_doc_test"));

    assert!(get_err_stdout(&dir, ["test"]).contains("Total tests: 9, passed: 6, failed: 3."));

    // should be ok if run with update
    get_stdout(&dir, ["test", "-u"]);

    // Positional file path should run internal test & doc test in that file
    check(
        get_stdout(&dir, ["test", "lib/hello.mbt", "--sort-input"])
            .split("\n")
            .collect::<Vec<&str>>()
            .iter()
            .take(5)
            .next_back()
            .unwrap(),
        expect!["Total tests: 4, passed: 4, failed: 0."],
    );
    // -i should run internal test only
    check(
        get_stdout(&dir, ["test", "lib/hello.mbt", "-i", "0"]),
        expect![[r#"
            internal test 1
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    // --doc-index should run doc test only
    check(
        get_stdout(&dir, ["test", "lib/hello.mbt", "--doc-index", "0"]),
        expect![[r#"
            doc test 1
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    // should run bb test only
    check(
        get_stdout(&dir, ["test", "lib/hello_test.mbt", "-i", "0"]),
        expect![[r#"
            blackbox test 1
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    // doc test is ignored for _test.mbt & .mbt.md
    {
        check(
            get_stdout(
                &dir,
                [
                    "test",
                    "-p",
                    "lib",
                    "--file",
                    "hello_test.mbt",
                    "--doc-index",
                    "0",
                ],
            ),
            expect![[r#"
            Total tests: 0, passed: 0, failed: 0.
        "#]],
        );
        check(
            get_stdout(
                &dir,
                [
                    "test",
                    "-p",
                    "lib",
                    "--file",
                    "README.mbt.md",
                    "--doc-index",
                    "0",
                ],
            ),
            expect![[r#"
            Total tests: 0, passed: 0, failed: 0.
        "#]],
        );
    }
}
