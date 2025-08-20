use super::*;
use std::path::Path;

// Helpers: single-walk snapshot of layout and files
fn is_excluded(entry_name: &str) -> bool {
    matches!(entry_name, ".git" | "target" | ".DS_Store")
}

fn snapshot_layout_and_files(root: &Path) -> String {
    let mut layout_items: Vec<String> = Vec::new();
    let mut file_items: Vec<(String, String)> = Vec::new();

    for entry in walkdir::WalkDir::new(root)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
    {
        let path = entry.path();
        // top-level exclusion
        let first = path
            .strip_prefix(root)
            .ok()
            .and_then(|p| p.components().next())
            .and_then(|c| c.as_os_str().to_str())
            .unwrap_or("");
        if is_excluded(first) {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");

        if entry.file_type().is_symlink() {
            let rel_file = format!("./{}", rel);
            layout_items.push(rel_file.clone());
            let link_target = std::fs::read_link(path)
                .map(|target| format!("<symbolic link to {}>", target.display()))
                .unwrap_or_else(|_| "<symbolic link>".to_string());
            file_items.push((rel_file, format!("{}\n", link_target)));
        } else if entry.file_type().is_dir() {
            layout_items.push(format!("./{}/", rel.trim_end_matches('/')));
        } else {
            let rel_file = format!("./{}", rel);
            layout_items.push(rel_file.clone());
            if rel == "LICENSE" {
                // Skip LICENSE file content
                file_items.push((rel_file, "<LICENSE file content>\n".to_string()));
                continue;
            }
            let mut content = read(path);
            if !content.ends_with('\n') {
                content.push('\n');
            }
            file_items.push((rel_file, content));
        }
    }

    layout_items.sort();
    file_items.sort_by(|a, b| a.0.cmp(&b.0));

    let mut out = String::new();
    out.push_str("-- layout --\n");
    out.push_str(".\n");
    for item in layout_items {
        out.push_str(&item);
        out.push('\n');
    }
    out.push_str("\n-- files --\n");
    for (rel, content) in file_items {
        out.push_str(&format!("=== {} ===\n", rel));
        out.push_str(&content);
        out.push('\n');
    }

    out
}

#[test]
fn test_moon_run_main() {
    let dir = TestDir::new("moon_new/plain");
    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
            Hello, world!
        "#]],
    );
}

#[test]
#[cfg(unix)]
fn test_moon_new() {
    let dir = TestDir::new_empty();
    get_stdout(
        &dir,
        [
            "new",
            "--path",
            "hello",
            "--user",
            "moonbitlang",
            "--name",
            "hello",
        ],
    );
    check(
        get_stdout(
            &dir,
            [
                "run",
                "--source-dir",
                "./hello",
                "--target-dir",
                "./hello/target",
                "cmd/main",
            ],
        ),
        expect![[r#"
            89
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "run",
                "--directory",
                "./hello",
                "--target-dir",
                "./hello/target",
                "cmd/main",
            ],
        ),
        expect![[r#"
            89
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "run",
                "-C",
                "./hello",
                "--target-dir",
                "./hello/target",
                "cmd/main",
            ],
        ),
        expect![[r#"
            89
        "#]],
    );
}

#[test]
fn test_moon_new_exist() {
    let dir = TestDir::new("moon_new/exist");
    dir.join("hello").rm_rf();
    let res = &get_stdout(
        &dir,
        [
            "new",
            "--path",
            "hello",
            "--user",
            "moonbitlang",
            "--name",
            "hello",
        ],
    );

    assert!(res.contains("Created hello"));
    assert!(res.contains("Initialized empty Git repository"));

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(dir.join("hello"))
        .args([
            "new",
            "--path",
            ".",
            "--user",
            "moonbitlang",
            "--name",
            "hello",
        ])
        .assert()
        .failure();

    dir.join("hello").rm_rf();
}

#[test]
fn test_moon_new_new() {
    let dir = TestDir::new("moon_new/new");

    let hello1 = dir.join("hello");
    if hello1.exists() {
        hello1.rm_rf()
    }
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args([
            "new",
            "--path",
            "hello",
            "--user",
            "moonbitlang",
            "--name",
            "hello",
        ])
        .assert()
        .success();
    check(
        get_stdout(&hello1, ["run", "cmd/main"]),
        expect![[r#"
            89
        "#]],
    );
    hello1.rm_rf();

    let hello2 = dir.join("hello2");
    std::fs::create_dir_all(&hello2).unwrap();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&hello2)
        .args([
            "new",
            "--path",
            ".",
            "--user",
            "moonbitlang",
            "--name",
            "hello",
        ])
        .assert()
        .success();
    check(
        get_stdout(&hello2, ["run", "cmd/main"]),
        expect![[r#"
            89
        "#]],
    );
    hello2.rm_rf();

    let hello3 = dir.join("hello3");
    if hello3.exists() {
        hello3.rm_rf();
    }
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args([
            "new",
            "--lib",
            "--path",
            "hello3",
            "--user",
            "moonbitlang",
            "--name",
            "hello",
        ])
        .assert()
        .success();
    check(
        get_stdout(&hello3, ["test", "-v"]),
        expect![[r#"
            test moonbitlang/hello/hello_test.mbt::fib ok
            test moonbitlang/hello/README.mbt.md::0 ok
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );
    check(
        get_stdout(&hello3, ["test"]),
        expect![[r#"
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );
    hello3.rm_rf();

    let hello4 = dir.join("hello4");
    std::fs::create_dir_all(&hello4).unwrap();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&hello4)
        .args([
            "new",
            "--path",
            ".",
            "--user",
            "moonbitlang",
            "--name",
            "hello",
        ])
        .assert()
        .success();
    check(
        std::fs::read_to_string(hello4.join("moon.pkg.json")).unwrap(),
        expect![["{}"]],
    );
    check(
        get_stdout(&hello4, ["test", "-v"]),
        expect![[r#"
            test moonbitlang/hello/hello_test.mbt::fib ok
            test moonbitlang/hello/README.mbt.md::0 ok
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );
    hello4.rm_rf();
}

#[test]
#[ignore = "todo"]
fn test_moon_new_interactive() {
    let dir = TestDir::new("moon_new/new");

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["new"])
        .stdin("hello5\nexec\nmoonbitlang\nhello5\n\n")
        .assert()
        .success();
    check(
        std::fs::read_to_string(dir.join("hello5").join("moon.mod.json")).unwrap(),
        expect![[r#"
            {
              "name": "moonbitlang/hello5",
              "version": "0.1.0",
              "readme": "README.md",
              "repository": "",
              "license": "",
              "keywords": [],
              "description": ""
            }"#]],
    );
    dir.join("hello5").rm_rf();

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["new"])
        .stdin("hello6\nlib\nmoonbitlang\nhello6\n")
        .assert()
        .success();
    check(
        std::fs::read_to_string(dir.join("hello6").join("moon.pkg.json")).unwrap(),
        expect![[r#"
            {
              "import": [
                "moonbitlang/hello6/lib"
              ]
            }
        "#]],
    );
    dir.join("hello6").rm_rf();
}

#[test]
fn test_moon_new_snapshot() {
    let dir = TestDir::new("moon_new/snapshot");

    let hello = dir.join("hello");
    if hello.exists() {
        hello.rm_rf();
    }

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["new", "hello", "--no-license"])
        .env("MOON_HOME", dir.path.path())
        .assert()
        .success();

    // New snapshot: layout first, then file contents
    let snap = snapshot_layout_and_files(&hello);
    check(
        &snap,
        expect![[r#"
            -- layout --
            .
            ./.gitignore
            ./README.mbt.md
            ./README.md
            ./cmd/
            ./cmd/main/
            ./cmd/main/main.mbt
            ./cmd/main/moon.pkg.json
            ./hello.mbt
            ./hello_test.mbt
            ./moon.mod.json
            ./moon.pkg.json

            -- files --
            === ./.gitignore ===
            .DS_Store
            target/
            .mooncakes/
            .moonagent/

            === ./README.mbt.md ===
            # testuser/hello

            ## Development
            - To test the project, run `moon test`. To update the snapshot, run `moon test --update`.
            - To build the project, run `moon build`.
            - To run the project, run `moon run cmd/main`.

            You may also write tests in this file to demonstrate the functionality of your project:
            ```moonbit
            test {
              inspect(@hello.fib(10), content="89")
            }
            ```

            === ./README.md ===
            <symbolic link to README.mbt.md>

            === ./cmd/main/main.mbt ===
            ///|
            fn main {
              println(@lib.fib(10))
            }

            === ./cmd/main/moon.pkg.json ===
            {
              "is-main": true,
              "import": [
                {
                  "path": "testuser/hello",
                  "alias": "lib"
                }
              ]
            }

            === ./hello.mbt ===
            ///|
            pub fn fib(n : Int) -> Int64 {
              for i = 0, a = 0L, b = 1L; i < n; i = i + 1, a = b, b = a + b {

              } else {
                b
              }
            }

            === ./hello_test.mbt ===
            ///|
            test "fib" {
              let array = [1, 2, 3, 4, 5].map(fib(_))

              // `inspect` is used to check the output of the function
              // Just write `inspect(value)` and execute `moon test --update`
              // to update the expected output, and verify them afterwards
              inspect(array, content="[1, 2, 3, 5, 8]")
            }

            === ./moon.mod.json ===
            {
              "name": "testuser/hello",
              "version": "0.1.0",
              "readme": "README.md",
              "repository": "",
              "license": "",
              "keywords": [],
              "description": ""
            }

            === ./moon.pkg.json ===
            {}

        "#]],
    );
    assert!(!hello.join("LICENSE").exists());

    if hello.exists() {
        hello.rm_rf();
    }

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args([
            "new",
            "--path",
            "hello",
            "--user",
            "moonbitlang",
            "--name",
            "hello",
        ])
        .assert()
        .success();

    let snap2 = snapshot_layout_and_files(&hello);
    check(
        &snap2,
        expect![[r#"
            -- layout --
            .
            ./.gitignore
            ./LICENSE
            ./README.mbt.md
            ./README.md
            ./cmd/
            ./cmd/main/
            ./cmd/main/main.mbt
            ./cmd/main/moon.pkg.json
            ./hello.mbt
            ./hello_test.mbt
            ./moon.mod.json
            ./moon.pkg.json

            -- files --
            === ./.gitignore ===
            .DS_Store
            target/
            .mooncakes/
            .moonagent/

            === ./LICENSE ===
            <LICENSE file content>

            === ./README.mbt.md ===
            # moonbitlang/hello

            ## Development
            - To test the project, run `moon test`. To update the snapshot, run `moon test --update`.
            - To build the project, run `moon build`.
            - To run the project, run `moon run cmd/main`.

            You may also write tests in this file to demonstrate the functionality of your project:
            ```moonbit
            test {
              inspect(@hello.fib(10), content="89")
            }
            ```

            === ./README.md ===
            <symbolic link to README.mbt.md>

            === ./cmd/main/main.mbt ===
            ///|
            fn main {
              println(@lib.fib(10))
            }

            === ./cmd/main/moon.pkg.json ===
            {
              "is-main": true,
              "import": [
                {
                  "path": "moonbitlang/hello",
                  "alias": "lib"
                }
              ]
            }

            === ./hello.mbt ===
            ///|
            pub fn fib(n : Int) -> Int64 {
              for i = 0, a = 0L, b = 1L; i < n; i = i + 1, a = b, b = a + b {

              } else {
                b
              }
            }

            === ./hello_test.mbt ===
            ///|
            test "fib" {
              let array = [1, 2, 3, 4, 5].map(fib(_))

              // `inspect` is used to check the output of the function
              // Just write `inspect(value)` and execute `moon test --update`
              // to update the expected output, and verify them afterwards
              inspect(array, content="[1, 2, 3, 5, 8]")
            }

            === ./moon.mod.json ===
            {
              "name": "moonbitlang/hello",
              "version": "0.1.0",
              "readme": "README.md",
              "repository": "",
              "license": "Apache-2.0",
              "keywords": [],
              "description": ""
            }

            === ./moon.pkg.json ===
            {}

        "#]],
    );
    hello.rm_rf();
}

#[test]
fn test_moon_new_snapshot_lib() {
    let dir = TestDir::new("moon_new/snapshot");

    let hello = dir.join("hello_lib");

    if hello.exists() {
        hello.rm_rf()
    }

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["new", "--lib", "hello_lib"])
        .env("MOON_HOME", dir.path.path())
        .assert()
        .success();

    // Snapshot layout + files (includes LICENSE)
    let snap = snapshot_layout_and_files(&hello);
    check(
        &snap,
        expect![[r#"
            -- layout --
            .
            ./.gitignore
            ./LICENSE
            ./README.mbt.md
            ./README.md
            ./cmd/
            ./cmd/main/
            ./cmd/main/main.mbt
            ./cmd/main/moon.pkg.json
            ./hello.mbt
            ./hello_test.mbt
            ./moon.mod.json
            ./moon.pkg.json

            -- files --
            === ./.gitignore ===
            .DS_Store
            target/
            .mooncakes/
            .moonagent/

            === ./LICENSE ===
            <LICENSE file content>

            === ./README.mbt.md ===
            # testuser/hello

            ## Development
            - To test the project, run `moon test`. To update the snapshot, run `moon test --update`.
            - To build the project, run `moon build`.
            - To run the project, run `moon run cmd/main`.

            You may also write tests in this file to demonstrate the functionality of your project:
            ```moonbit
            test {
              inspect(@hello.fib(10), content="89")
            }
            ```

            === ./README.md ===
            <symbolic link to README.mbt.md>

            === ./cmd/main/main.mbt ===
            ///|
            fn main {
              println(@lib.fib(10))
            }

            === ./cmd/main/moon.pkg.json ===
            {
              "is-main": true,
              "import": [
                {
                  "path": "testuser/hello",
                  "alias": "lib"
                }
              ]
            }

            === ./hello.mbt ===
            ///|
            pub fn fib(n : Int) -> Int64 {
              for i = 0, a = 0L, b = 1L; i < n; i = i + 1, a = b, b = a + b {

              } else {
                b
              }
            }

            === ./hello_test.mbt ===
            ///|
            test "fib" {
              let array = [1, 2, 3, 4, 5].map(fib(_))

              // `inspect` is used to check the output of the function
              // Just write `inspect(value)` and execute `moon test --update`
              // to update the expected output, and verify them afterwards
              inspect(array, content="[1, 2, 3, 5, 8]")
            }

            === ./moon.mod.json ===
            {
              "name": "testuser/hello",
              "version": "0.1.0",
              "readme": "README.md",
              "repository": "",
              "license": "Apache-2.0",
              "keywords": [],
              "description": ""
            }

            === ./moon.pkg.json ===
            {}

        "#]],
    );
    hello.rm_rf();

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args([
            "new",
            "--lib",
            "--path",
            "hello_lib",
            "--user",
            "username",
            "--name",
            "parser",
        ])
        .assert()
        .success();

    // Snapshot layout + files (includes LICENSE)
    let snap2 = snapshot_layout_and_files(&hello);
    check(
        &snap2,
        expect![[r#"
            -- layout --
            .
            ./.gitignore
            ./LICENSE
            ./README.mbt.md
            ./README.md
            ./cmd/
            ./cmd/main/
            ./cmd/main/main.mbt
            ./cmd/main/moon.pkg.json
            ./moon.mod.json
            ./moon.pkg.json
            ./parser.mbt
            ./parser_test.mbt

            -- files --
            === ./.gitignore ===
            .DS_Store
            target/
            .mooncakes/
            .moonagent/

            === ./LICENSE ===
            <LICENSE file content>

            === ./README.mbt.md ===
            # username/parser

            ## Development
            - To test the project, run `moon test`. To update the snapshot, run `moon test --update`.
            - To build the project, run `moon build`.
            - To run the project, run `moon run cmd/main`.

            You may also write tests in this file to demonstrate the functionality of your project:
            ```moonbit
            test {
              inspect(@parser.fib(10), content="89")
            }
            ```

            === ./README.md ===
            <symbolic link to README.mbt.md>

            === ./cmd/main/main.mbt ===
            ///|
            fn main {
              println(@lib.fib(10))
            }

            === ./cmd/main/moon.pkg.json ===
            {
              "is-main": true,
              "import": [
                {
                  "path": "username/parser",
                  "alias": "lib"
                }
              ]
            }

            === ./moon.mod.json ===
            {
              "name": "username/parser",
              "version": "0.1.0",
              "readme": "README.md",
              "repository": "",
              "license": "Apache-2.0",
              "keywords": [],
              "description": ""
            }

            === ./moon.pkg.json ===
            {}

            === ./parser.mbt ===
            ///|
            pub fn fib(n : Int) -> Int64 {
              for i = 0, a = 0L, b = 1L; i < n; i = i + 1, a = b, b = a + b {

              } else {
                b
              }
            }

            === ./parser_test.mbt ===
            ///|
            test "fib" {
              let array = [1, 2, 3, 4, 5].map(fib(_))

              // `inspect` is used to check the output of the function
              // Just write `inspect(value)` and execute `moon test --update`
              // to update the expected output, and verify them afterwards
              inspect(array, content="[1, 2, 3, 5, 8]")
            }

        "#]],
    );
    hello.rm_rf();
}

#[test]
fn test_moon_new_snapshot_lib_no_license() {
    let dir = TestDir::new("moon_new/snapshot");

    let hello = dir.join("hello_lib");

    if hello.exists() {
        hello.rm_rf()
    }

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["new", "--lib", "hello_lib", "--no-license"])
        .env("MOON_HOME", dir.path.path())
        .assert()
        .success();

    let snap1 = snapshot_layout_and_files(&hello);
    check(
        snap1,
        expect![[r#"
            -- layout --
            .
            ./.gitignore
            ./README.mbt.md
            ./README.md
            ./cmd/
            ./cmd/main/
            ./cmd/main/main.mbt
            ./cmd/main/moon.pkg.json
            ./hello.mbt
            ./hello_test.mbt
            ./moon.mod.json
            ./moon.pkg.json

            -- files --
            === ./.gitignore ===
            .DS_Store
            target/
            .mooncakes/
            .moonagent/

            === ./README.mbt.md ===
            # testuser/hello

            ## Development
            - To test the project, run `moon test`. To update the snapshot, run `moon test --update`.
            - To build the project, run `moon build`.
            - To run the project, run `moon run cmd/main`.

            You may also write tests in this file to demonstrate the functionality of your project:
            ```moonbit
            test {
              inspect(@hello.fib(10), content="89")
            }
            ```

            === ./README.md ===
            <symbolic link to README.mbt.md>

            === ./cmd/main/main.mbt ===
            ///|
            fn main {
              println(@lib.fib(10))
            }

            === ./cmd/main/moon.pkg.json ===
            {
              "is-main": true,
              "import": [
                {
                  "path": "testuser/hello",
                  "alias": "lib"
                }
              ]
            }

            === ./hello.mbt ===
            ///|
            pub fn fib(n : Int) -> Int64 {
              for i = 0, a = 0L, b = 1L; i < n; i = i + 1, a = b, b = a + b {

              } else {
                b
              }
            }

            === ./hello_test.mbt ===
            ///|
            test "fib" {
              let array = [1, 2, 3, 4, 5].map(fib(_))

              // `inspect` is used to check the output of the function
              // Just write `inspect(value)` and execute `moon test --update`
              // to update the expected output, and verify them afterwards
              inspect(array, content="[1, 2, 3, 5, 8]")
            }

            === ./moon.mod.json ===
            {
              "name": "testuser/hello",
              "version": "0.1.0",
              "readme": "README.md",
              "repository": "",
              "license": "",
              "keywords": [],
              "description": ""
            }

            === ./moon.pkg.json ===
            {}

        "#]],
    );

    if hello.exists() {
        hello.rm_rf()
    }

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args([
            "new",
            "--lib",
            "--path",
            "hello_lib",
            "--user",
            "moonbitlang",
            "--name",
            "hello",
            "--no-license",
        ])
        .assert()
        .success();

    let snap2 = snapshot_layout_and_files(&hello);
    check(
        snap2,
        expect![[r#"
            -- layout --
            .
            ./.gitignore
            ./README.mbt.md
            ./README.md
            ./cmd/
            ./cmd/main/
            ./cmd/main/main.mbt
            ./cmd/main/moon.pkg.json
            ./hello.mbt
            ./hello_test.mbt
            ./moon.mod.json
            ./moon.pkg.json

            -- files --
            === ./.gitignore ===
            .DS_Store
            target/
            .mooncakes/
            .moonagent/

            === ./README.mbt.md ===
            # moonbitlang/hello

            ## Development
            - To test the project, run `moon test`. To update the snapshot, run `moon test --update`.
            - To build the project, run `moon build`.
            - To run the project, run `moon run cmd/main`.

            You may also write tests in this file to demonstrate the functionality of your project:
            ```moonbit
            test {
              inspect(@hello.fib(10), content="89")
            }
            ```

            === ./README.md ===
            <symbolic link to README.mbt.md>

            === ./cmd/main/main.mbt ===
            ///|
            fn main {
              println(@lib.fib(10))
            }

            === ./cmd/main/moon.pkg.json ===
            {
              "is-main": true,
              "import": [
                {
                  "path": "moonbitlang/hello",
                  "alias": "lib"
                }
              ]
            }

            === ./hello.mbt ===
            ///|
            pub fn fib(n : Int) -> Int64 {
              for i = 0, a = 0L, b = 1L; i < n; i = i + 1, a = b, b = a + b {

              } else {
                b
              }
            }

            === ./hello_test.mbt ===
            ///|
            test "fib" {
              let array = [1, 2, 3, 4, 5].map(fib(_))

              // `inspect` is used to check the output of the function
              // Just write `inspect(value)` and execute `moon test --update`
              // to update the expected output, and verify them afterwards
              inspect(array, content="[1, 2, 3, 5, 8]")
            }

            === ./moon.mod.json ===
            {
              "name": "moonbitlang/hello",
              "version": "0.1.0",
              "readme": "README.md",
              "repository": "",
              "license": "",
              "keywords": [],
              "description": ""
            }

            === ./moon.pkg.json ===
            {}

        "#]],
    );
    hello.rm_rf();
}
