use super::*;

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
                "src/main",
            ],
        ),
        expect![[r#"
            Hello, world!
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
                "src/main",
            ],
        ),
        expect![[r#"
            Hello, world!
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
                "src/main",
            ],
        ),
        expect![[r#"
            Hello, world!
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
        get_stdout(&hello1, ["run", "src/main"]),
        expect![[r#"
            Hello, world!
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
        get_stdout(&hello2, ["run", "src/main"]),
        expect![[r#"
            Hello, world!
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
            test moonbitlang/hello/lib/hello_test.mbt::hello ok
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    check(
        get_stdout(&hello3, ["test"]),
        expect![[r#"
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    hello3.rm_rf();

    let hello4 = dir.join("hello4");
    std::fs::create_dir_all(&hello4).unwrap();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&hello4)
        .args([
            "new",
            "--lib",
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
        std::fs::read_to_string(hello4.join("src").join("moon.pkg.json")).unwrap(),
        expect![[r#"
            {
              "import": [
                "moonbitlang/hello/lib"
              ]
            }
        "#]],
    );
    check(
        get_stdout(&hello4, ["test", "-v"]),
        expect![[r#"
            test moonbitlang/hello/lib/hello_test.mbt::hello ok
            Total tests: 1, passed: 1, failed: 0.
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
        .assert()
        .success();
    check(
        read(hello.join("src").join("lib").join("hello.mbt")),
        expect![[r#"
            pub fn hello() -> String {
              "Hello, world!"
            }
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
    check(
        read(hello.join("src").join("lib").join("hello.mbt")),
        expect![[r#"
            pub fn hello() -> String {
              "Hello, world!"
            }
        "#]],
    );
    check(
        read(hello.join("src").join("lib").join("hello_test.mbt")),
        expect![[r#"
            test "hello" {
              if @lib.hello() != "Hello, world!" {
                fail("@lib.hello() != \"Hello, world!\"")
              }
            }
        "#]],
    );
    check(
        std::fs::read_to_string(hello.join("src").join("lib").join("moon.pkg.json")).unwrap(),
        expect!["{}"],
    );
    check(
        read(hello.join("src").join("main").join("main.mbt")),
        expect![[r#"
            fn main {
              println(@lib.hello())
            }
        "#]],
    );
    check(
        std::fs::read_to_string(hello.join("src").join("main").join("moon.pkg.json")).unwrap(),
        expect![[r#"
            {
              "is-main": true,
              "import": [
                "moonbitlang/hello/lib"
              ]
            }"#]],
    );
    check(
        std::fs::read_to_string(hello.join("moon.mod.json")).unwrap(),
        expect![[r#"
            {
              "name": "moonbitlang/hello",
              "version": "0.1.0",
              "readme": "README.md",
              "repository": "",
              "license": "Apache-2.0",
              "keywords": [],
              "description": "",
              "source": "src"
            }"#]],
    );
    let license_content = std::fs::read_to_string(hello.join("LICENSE")).unwrap();
    assert!(license_content.contains("Apache License"));
    assert!(license_content.contains("Version 2.0, January 2004"));
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
        .assert()
        .success();

    let license_content = std::fs::read_to_string(hello.join("LICENSE")).unwrap();
    assert!(license_content.contains("Apache License"));
    assert!(license_content.contains("Version 2.0, January 2004"));
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
        .assert()
        .success();
    check(
        read(hello.join("src").join("lib").join("hello.mbt")),
        expect![[r#"
            pub fn hello() -> String {
              "Hello, world!"
            }
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
    check(
        read(hello.join("src").join("lib").join("hello.mbt")),
        expect![[r#"
            pub fn hello() -> String {
              "Hello, world!"
            }
        "#]],
    );
    check(
        read(hello.join("src").join("lib").join("hello_test.mbt")),
        expect![[r#"
            test "hello" {
              if @lib.hello() != "Hello, world!" {
                fail("@lib.hello() != \"Hello, world!\"")
              }
            }
        "#]],
    );
    check(
        std::fs::read_to_string(hello.join("src").join("lib").join("moon.pkg.json")).unwrap(),
        expect!["{}"],
    );
    check(
        std::fs::read_to_string(hello.join("src").join("moon.pkg.json")).unwrap(),
        expect![[r#"
            {
              "import": [
                "moonbitlang/hello/lib"
              ]
            }
        "#]],
    );
    check(
        std::fs::read_to_string(hello.join("moon.mod.json")).unwrap(),
        expect![[r#"
            {
              "name": "moonbitlang/hello",
              "version": "0.1.0",
              "readme": "README.md",
              "repository": "",
              "license": "",
              "keywords": [],
              "description": "",
              "source": "src"
            }"#]],
    );
    check(
        read(hello.join("src").join("top.mbt")),
        expect![[r#"
            pub fn greeting() -> Unit {
              println(@lib.hello())
            }
        "#]],
    );
    check(
        std::fs::read_to_string(hello.join("README.md")).unwrap(),
        expect!["# moonbitlang/hello"],
    );
    hello.rm_rf();
}
