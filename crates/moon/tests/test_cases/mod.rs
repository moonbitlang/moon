use std::io::Write;

use super::*;
use expect_test::expect;
use moonutil::common::{
    get_cargo_pkg_version, CargoPathExt, TargetBackend, DEP_PATH, MOON_MOD_JSON,
};
use walkdir::WalkDir;

#[test]
fn test_design() {
    let dir = TestDir::new("design.in");
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["check"])
        .assert()
        .success();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["clean"])
        .assert()
        .success();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["build"])
        .assert()
        .success();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["clean"])
        .assert()
        .success();
    check(
        &get_stdout_with_args(&dir, ["run", "main1"]),
        expect![[r#"
            new_list
            new_queue
            new_list
            new_stack
            new_vector
            main1
        "#]],
    );
    check(
        &get_stdout_with_args(&dir, ["run", "main2"]),
        expect![[r#"
            new_list
            new_queue
            main2
        "#]],
    );
}

#[test]
fn test_diamond_pkg_001() {
    let dir = TestDir::new("diamond-pkg-001.in");
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["check"])
        .assert()
        .success();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["clean"])
        .assert()
        .success();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["build"])
        .assert()
        .success();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["clean"])
        .assert()
        .success();
    check(
        &get_stdout_with_args(&dir, ["run", "main"]),
        expect![[r#"
            A
            B
            A
            C
            main
        "#]],
    );
}

#[test]
fn test_diamond_pkg_002() {
    let dir = TestDir::new("diamond-pkg-002.in");
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["check"])
        .assert()
        .success();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["clean"])
        .assert()
        .success();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["build"])
        .assert()
        .success();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["clean"])
        .assert()
        .success();
    check(
        &get_stdout_with_args(&dir, ["run", "main"]),
        expect![[r#"
            A0
            A1
            A2
            A
            B0
            B1
            B2
            B
            A0
            A1
            A2
            A
            C0
            C1
            C2
            C
            main
        "#]],
    );
}

#[test]
fn test_diamond_pkg_003() {
    let dir = TestDir::new("diamond-pkg-003.in");
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["check"])
        .assert()
        .success();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["clean"])
        .assert()
        .success();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["build"])
        .assert()
        .success();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["clean"])
        .assert()
        .success();
    check(
        &get_stdout_with_args(&dir, ["run", "main"]),
        expect![[r#"
            A0
            A1
            A2
            A
            B0
            B1
            B2
            B
            A0
            A1
            A2
            A
            C0
            C1
            C2
            C
            main
        "#]],
    );
}

#[test]
fn test_extra_flags() {
    let dir = TestDir::new("extra_flags.in");
    check(
        &get_stdout_with_args(&dir, ["build", "--dry-run", "--nostd"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -g -no-builtin
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -g -no-builtin
            moonc link-core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc -no-builtin
        "#]],
    );
    check(
        &get_stdout_with_args(&dir, ["build", "--dry-run", "--debug", "--nostd"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -g -source-map -g -no-builtin
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -g -source-map -g -no-builtin
            moonc link-core ./target/wasm-gc/debug/build/lib/lib.core ./target/wasm-gc/debug/build/main/main.core -main hello/main -o ./target/wasm-gc/debug/build/main/main.wasm -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc -g -source-map -no-builtin
        "#]],
    );

    check(
        &get_stdout_with_args(&dir, ["run", "main", "--dry-run", "--nostd"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -g -no-builtin
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -g -no-builtin
            moonc link-core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc -no-builtin
            moonrun ./target/wasm-gc/release/build/main/main.wasm
        "#]],
    );
    check(
        &get_stdout_with_args(&dir, ["run", "main", "--dry-run", "--debug", "--nostd"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -g -source-map -g -no-builtin
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -g -source-map -g -no-builtin
            moonc link-core ./target/wasm-gc/debug/build/lib/lib.core ./target/wasm-gc/debug/build/main/main.core -main hello/main -o ./target/wasm-gc/debug/build/main/main.wasm -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc -g -source-map -no-builtin
            moonrun ./target/wasm-gc/debug/build/main/main.wasm
        "#]],
    );
}

#[test]
fn test_fancy_import() {
    let dir = TestDir::new("fancy_import.in/import001");
    check(
        &get_stdout_with_args(&dir, ["run", "main"]),
        expect![[r#"
            Hello, world!
        "#]],
    );

    let dir = TestDir::new("fancy_import.in/import002");
    check(
        &get_stdout_with_args(&dir, ["run", "main"]),
        expect![[r#"
            Hello, world!
        "#]],
    );

    let dir = TestDir::new("fancy_import.in/import003");
    check(
        &get_stdout_with_args(&dir, ["run", "main"]),
        expect![[r#"
            Hello, world!
            Hello, world2!
        "#]],
    );

    let dir = TestDir::new("fancy_import.in/import004");
    check(
        &get_stdout_with_args(&dir, ["run", "main"]),
        expect![[r#"
            f1
            f2
            f3
            f4
        "#]],
    );
}

#[test]
fn test_hello() {
    let dir = TestDir::new("hello.in");
    check(
        &get_stdout_with_args(&dir, ["run", "main"]),
        expect![[r#"
            Hello, world!
        "#]],
    );
}

#[test]
fn test_moon_commands() {
    let dir = TestDir::new("moon_commands.in");
    check(
        &get_stdout_with_args(&dir, ["build", "--dry-run", "--nostd", "--sort-input"]),
        expect![[r#"
            moonc build-package ./lib/list/lib.mbt -o ./target/wasm-gc/release/build/lib/list/list.core -pkg design/lib/list -pkg-sources design/lib/list:./lib/list -target wasm-gc
            moonc build-package ./lib/queue/lib.mbt -o ./target/wasm-gc/release/build/lib/queue/queue.core -pkg design/lib/queue -i ./target/wasm-gc/release/build/lib/list/list.mi:list -pkg-sources design/lib/queue:./lib/queue -target wasm-gc
            moonc build-package ./main2/main.mbt -o ./target/wasm-gc/release/build/main2/main2.core -pkg design/main2 -is-main -i ./target/wasm-gc/release/build/lib/queue/queue.mi:queue -pkg-sources design/main2:./main2 -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/lib/list/list.core ./target/wasm-gc/release/build/lib/queue/queue.core ./target/wasm-gc/release/build/main2/main2.core -main design/main2 -o ./target/wasm-gc/release/build/main2/main2.wasm -pkg-sources design/lib/list:./lib/list -pkg-sources design/lib/queue:./lib/queue -pkg-sources design/main2:./main2 -target wasm-gc
            moonc build-package ./main1/main.mbt -o ./target/wasm-gc/release/build/main1/main1.core -pkg design/main1 -is-main -i ./target/wasm-gc/release/build/lib/queue/queue.mi:queue -pkg-sources design/main1:./main1 -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/lib/list/list.core ./target/wasm-gc/release/build/lib/queue/queue.core ./target/wasm-gc/release/build/main1/main1.core -main design/main1 -o ./target/wasm-gc/release/build/main1/main1.wasm -pkg-sources design/lib/list:./lib/list -pkg-sources design/lib/queue:./lib/queue -pkg-sources design/main1:./main1 -target wasm-gc
        "#]],
    );
}

#[test]
fn test_moon_run_main() {
    let dir = TestDir::new("moon_new.in");
    check(
        &get_stdout_with_args(&dir, ["run", "main"]),
        expect![[r#"
            Hello, world!
        "#]],
    );
}

#[test]
#[cfg(unix)]
fn test_moon_new() {
    let dir = TestDir::new_empty();
    get_stdout_with_args(
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
        &get_stdout_with_args(
            &dir,
            [
                "run",
                "--source-dir",
                "./hello",
                "--target-dir",
                "./hello/target",
                "main",
            ],
        ),
        expect![[r#"
            Hello, world!
        "#]],
    );
}

#[test]
fn test_moon_help() {
    let dir = TestDir::new_empty();
    check(
        &get_stdout_with_args(&dir, ["help"]).replace("moon.exe", "moon"),
        expect![[r#"
            MoonBit's build system

            Usage: moon [OPTIONS] <COMMAND>

            Commands:
              new                    Create a new moonbit package
              build                  Build the current package
              check                  Check the current package, but don't build object files
              run                    Run WebAssembly module
              test                   Test the current package
              clean                  Clean the target directory
              fmt                    Format moonbit source code
              doc                    Generate documentation
              info                   Generate public interface (`.mbti`) files for all packages in the module
              add                    Add a dependency
              remove                 Remove a dependency
              install                Install dependencies
              tree                   Display the dependency tree
              login                  Log in to your account
              register               Register an account at mooncakes.io
              publish                Publish the current package
              update                 Update the package registry index
              coverage               Code coverage utilities
              generate-build-matrix  Generate build matrix for benchmarking (legacy feature)
              upgrade                Upgrade toolchains
              version                Print version info and exit
              help                   Print this message or the help of the given subcommand(s)

            Options:
                  --source-dir <SOURCE_DIR>  The source code directory. Defaults to the current directory
                  --target-dir <TARGET_DIR>  The target directory. Defaults to `source_dir/target`
              -q, --quiet                    Suppress output
              -v, --verbose                  Increase verbosity
                  --trace                    Trace the execution of the program
                  --dry-run                  Do not actually run the command
              -h, --help                     Print help
        "#]],
    );
}

#[test]
#[cfg(unix)]
fn test_bench4() {
    let dir = TestDir::new_empty();
    get_stdout_with_args(&dir, ["generate-build-matrix", "-n", "4", "-o", "bench4"]);
    check(
        &get_stdout_with_args(
            &dir,
            [
                "run",
                "--source-dir",
                "./bench4",
                "--target-dir",
                "./bench4/target",
                "main",
            ],
        ),
        expect![[r#"
            ok
        "#]],
    );

    get_stdout_with_args(
        &dir,
        [
            "run",
            "--source-dir",
            "./bench4",
            "--target-dir",
            "./bench4/target",
            "--trace",
            "main",
        ],
    );

    let trace_file = dunce::canonicalize(dir.join("./trace.json")).unwrap();
    let t = std::fs::read_to_string(trace_file).unwrap();
    assert!(t.contains("moonbit::build::read"));
    assert!(t.contains(r#""name":"work.run""#));
    assert!(t.contains(r#""name":"run""#));
    assert!(t.contains(r#""name":"main""#));
}

#[test]
fn test_moon_version() {
    let dir = TestDir::new_empty();
    let output = get_stdout_with_args(&dir, ["version"]);
    assert!(output.contains(&format!("moon {}", get_cargo_pkg_version())));
}

#[test]
fn test_moon_version_json() -> anyhow::Result<()> {
    let dir = TestDir::new_empty();

    let output = get_stdout_with_args(&dir, ["version", "--json"]);
    let items: moonutil::common::VersionItems = serde_json_lenient::from_str(&output)?;
    assert_eq!(items.items.len(), 1);
    assert_eq!(items.items[0].name, "moon");
    assert!(items.items[0].version.contains(&get_cargo_pkg_version()));
    assert!(items.items[0].path.is_some());

    let output = get_stdout_with_args(&dir, ["version", "--all", "--json"]);
    let items: moonutil::common::VersionItems = serde_json_lenient::from_str(&output)?;
    assert_eq!(items.items.len(), 3);
    assert_eq!(items.items[0].name, "moon");
    assert!(items.items[0].version.contains(&get_cargo_pkg_version()));
    assert_eq!(items.items[1].name, "moonc");

    let output = get_stdout_with_args(&dir, ["version", "--all", "--json", "--no-path"]);
    let items: moonutil::common::VersionItems = serde_json_lenient::from_str(&output)?;
    assert!(items.items[0].path.is_none());

    Ok(())
}

#[test]
fn test_moon_new_exist() {
    let dir = TestDir::new("moon_new_exist.in");
    dir.join("hello").rm_rf();
    let res = &get_stdout_with_args(
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
    let dir = TestDir::new("moon_new_new.in");

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
        &get_stdout_with_args(&hello1, ["run", "main"]),
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
        &get_stdout_with_args(&hello2, ["run", "main"]),
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
        &get_stdout_with_args(&hello3, ["test", "-v"]),
        expect![[r#"
            test moonbitlang/hello/lib/hello_test.mbt::hello ok
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    check(
        &get_stdout_with_args(&hello3, ["test"]),
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
        &std::fs::read_to_string(hello4.join("moon.pkg.json")).unwrap(),
        expect![[r#"
            {
              "import": [
                "moonbitlang/hello/lib"
              ]
            }
        "#]],
    );
    check(
        &get_stdout_with_args(&hello4, ["test", "-v"]),
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
    let dir = TestDir::new("moon_new_new.in");

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["new"])
        .stdin("hello5\nexec\nmoonbitlang\nhello5\n\n")
        .assert()
        .success();
    check(
        &std::fs::read_to_string(dir.join("hello5").join("moon.mod.json")).unwrap(),
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
        &std::fs::read_to_string(dir.join("hello6").join("moon.pkg.json")).unwrap(),
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
    let dir = TestDir::new("moon_new_snapshot.in");

    let hello = dir.join("hello");
    if hello.exists() {
        hello.rm_rf();
    }

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["new", "hello"])
        .assert()
        .success();
    check(
        &std::fs::read_to_string(hello.join("lib").join("hello.mbt")).unwrap(),
        expect![[r#"
            pub fn hello() -> String {
              "Hello, world!"
            }
        "#]],
    );

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
        &std::fs::read_to_string(hello.join("lib").join("hello.mbt")).unwrap(),
        expect![[r#"
            pub fn hello() -> String {
              "Hello, world!"
            }
        "#]],
    );
    check(
        &std::fs::read_to_string(hello.join("lib").join("hello_test.mbt")).unwrap(),
        expect![[r#"
            test "hello" {
              if hello() != "Hello, world!" {
                raise "hello() != \"Hello, world!\""
              }
            }
        "#]],
    );
    check(
        &std::fs::read_to_string(hello.join("lib").join("moon.pkg.json")).unwrap(),
        expect!["{}"],
    );
    check(
        &std::fs::read_to_string(hello.join("main").join("main.mbt")).unwrap(),
        expect![[r#"
            fn main {
              println(@lib.hello())
            }
        "#]],
    );
    check(
        &std::fs::read_to_string(hello.join("main").join("moon.pkg.json")).unwrap(),
        expect![[r#"
            {
              "is-main": true,
              "import": [
                "moonbitlang/hello/lib"
              ]
            }"#]],
    );
    hello.rm_rf();
}

#[test]
fn test_moon_new_snapshot_lib() {
    let dir = TestDir::new("moon_new_snapshot.in");

    let hello = dir.join("hello_lib");

    if hello.exists() {
        hello.rm_rf()
    }

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["new", "--lib", "hello_lib"])
        .assert()
        .success();
    check(
        &std::fs::read_to_string(hello.join("lib").join("hello.mbt")).unwrap(),
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
        ])
        .assert()
        .success();
    check(
        &std::fs::read_to_string(hello.join("lib").join("hello.mbt")).unwrap(),
        expect![[r#"
            pub fn hello() -> String {
              "Hello, world!"
            }
        "#]],
    );
    check(
        &std::fs::read_to_string(hello.join("lib").join("hello_test.mbt")).unwrap(),
        expect![[r#"
            test "hello" {
              if hello() != "Hello, world!" {
                raise "hello() != \"Hello, world!\""
              }
            }
        "#]],
    );
    check(
        &std::fs::read_to_string(hello.join("lib").join("moon.pkg.json")).unwrap(),
        expect!["{}"],
    );
    check(
        &std::fs::read_to_string(hello.join("moon.pkg.json")).unwrap(),
        expect![[r#"
            {
              "import": [
                "moonbitlang/hello/lib"
              ]
            }
        "#]],
    );
    check(
        &std::fs::read_to_string(hello.join("moon.mod.json")).unwrap(),
        expect![[r#"
            {
              "name": "moonbitlang/hello",
              "version": "0.1.0",
              "readme": "README.md",
              "repository": "",
              "license": "",
              "keywords": [],
              "description": ""
            }"#]],
    );
    check(
        &std::fs::read_to_string(hello.join("top.mbt")).unwrap(),
        expect![[r#"
            pub fn greeting() -> Unit {
              println(@lib.hello())
            }
        "#]],
    );
    check(
        &std::fs::read_to_string(hello.join("README.md")).unwrap(),
        expect!["# moonbitlang/hello"],
    );
    hello.rm_rf();
}

#[test]
fn test_moon_test() {
    let dir = TestDir::new("moon_test.in");

    let out = snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["test", "--sort-input"])
        .assert()
        .failure()
        .get_output()
        .stdout
        .to_owned();

    let s = std::str::from_utf8(&out).unwrap().to_string();

    check(
        &s,
        expect![[r#"
            test moontest/lib2/hello_test.mbt::0 failed: hello() != "Hello, World"
            test moontest/lib2/nested/lib_test.mbt::0 failed: add1(1) should be 2
            Total tests: 10, passed: 8, failed: 2.
        "#]],
    );
}

#[test]
fn test_moon_test_filter_package() {
    let dir = TestDir::new("test_filter.in");

    check(
        &get_stdout_with_args(&dir, ["test", "-p", "username/hello/A", "--sort-input"]),
        expect![[r#"
            test A
            test B
            test C
            test D
            test hello_0
            test hello_1
            test hello_2
            Total tests: 7, passed: 7, failed: 0.
        "#]],
    );

    check(
        &get_stdout_with_args(&dir, ["test", "-p", "username/hello/lib", "--sort-input"]),
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
    let dir = TestDir::new("test_filter_pkg_with_test_imports.in");

    check(
        &get_stdout_with_args(
            &dir,
            [
                "test",
                "-p",
                "username/hello/lib",
                "username/hello/lib1",
                "username/hello/lib2",
                "--sort-input",
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

    check(
        &get_stdout_with_args(
            &dir,
            [
                "test",
                "-p",
                "username/hello/lib",
                "username/hello/lib1",
                "username/hello/lib2",
                "-p",
                "username/hello/lib3",
                "--sort-input",
            ],
        ),
        expect![[r#"
            Hello from lib1

            Hello from lib2

            Hello from lib7
            Hello from lib3

            Hello from lib4
            Hello from lib3

            Hello from lib7
            Hello from lib6
            Total tests: 7, passed: 7, failed: 0.
        "#]],
    );

    check(
        &get_stdout_with_args(
            &dir,
            [
                "test",
                "-p",
                "username/hello/lib",
                "username/hello/lib1",
                "username/hello/lib2",
                "-f",
                "lib.mbt",
                "-p",
                "username/hello/lib3",
                "--sort-input",
            ],
        ),
        expect![[r#"
            Hello from lib1

            Hello from lib2

            Hello from lib3

            Hello from lib4
            Hello from lib3

            Hello from lib7
            Total tests: 5, passed: 5, failed: 0.
        "#]],
    );

    check(
        &get_stdout_with_args(
            &dir,
            [
                "test",
                "-p",
                "username/hello/lib",
                "username/hello/lib1",
                "username/hello/lib2",
                "-f",
                "lib.mbt",
                "-p",
                "username/hello/lib3",
                "-i",
                "0",
                "--sort-input",
            ],
        ),
        expect![[r#"
            Hello from lib1

            Hello from lib2

            Hello from lib3

            Hello from lib4
            Hello from lib3

            Total tests: 4, passed: 4, failed: 0.
        "#]],
    );
}
#[test]
fn test_moon_test_filter_package_with_deps() {
    let dir = TestDir::new("test_filter_pkg_with_deps.in");

    check(
        &get_stdout_with_args(&dir, ["test", "-p", "username/hello/lib"]),
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
        &get_stdout_with_args(&dir, ["test", "-p", "username/hello/lib2"]),
        expect![[r#"
            Hello from lib2
            Hello from lib4

            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        &get_stdout_with_args(&dir, ["test", "-p", "username/hello/lib4"]),
        expect![[r#"
            Hello from lib4
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_test_filter_package_with_test_imports() {
    let dir = TestDir::new("test_filter_pkg_with_test_imports.in");

    check(
        &get_stdout_with_args(&dir, ["test", "-p", "username/hello/lib"]),
        expect![[r#"
            Hello from lib1

            Hello from lib2

            Hello from lib7
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );

    check(
        &get_stdout_with_args(&dir, ["test", "-p", "username/hello/lib1"]),
        expect![[r#"
            Hello from lib3

            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        &get_stdout_with_args(&dir, ["test", "-p", "username/hello/lib2"]),
        expect![[r#"
            Hello from lib4
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        &get_stdout_with_args(&dir, ["test", "-p", "username/hello/lib3"]),
        expect![[r#"
            Hello from lib3

            Hello from lib7
            Hello from lib6
            Total tests: 3, passed: 3, failed: 0.
        "#]],
    );

    check(
        &get_stdout_with_args(&dir, ["test", "-p", "username/hello/lib4"]),
        expect![[r#"
            Hello from lib5
            Hello from lib5
            Hello from lib7
            Total tests: 3, passed: 3, failed: 0.
        "#]],
    );

    check(
        &get_stdout_with_args(&dir, ["test", "-p", "username/hello/lib5"]),
        expect![[r#"
            Hello from lib5
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        &get_stdout_with_args(&dir, ["test", "-p", "username/hello/lib6"]),
        expect![[r#"
            Hello from lib6
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        &get_stdout_with_args(&dir, ["test", "-p", "username/hello/lib7"]),
        expect![[r#"
            Hello from lib7
            Hello from lib6
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_test_filter_package_dry_run() {
    let dir = TestDir::new("test_filter.in");

    check(
        &get_stdout_with_args_and_replace_dir(
            &dir,
            [
                "test",
                "-p",
                "username/hello/A",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target/wasm-gc/debug/test --package username/hello/A --sort-input
            moonc build-package ./A/hello.mbt ./A/test.mbt ./A/hello_test.mbt ./target/wasm-gc/debug/test/A/__generated_driver_for_underscore_test.mbt -o ./target/wasm-gc/debug/test/A/A.underscore_test.core -pkg username/hello/A -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/A:./A -target wasm-gc -g
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/A/A.underscore_test.core -main username/hello/A -o ./target/wasm-gc/debug/test/A/A.underscore_test.wasm -test-mode -pkg-sources username/hello/A:./A -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc -g
            moonc build-package ./A/hello.mbt ./A/test.mbt ./target/wasm-gc/debug/test/A/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/A/A.internal_test.core -pkg username/hello/A -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/A:./A -target wasm-gc -g
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/A/A.internal_test.core -main username/hello/A -o ./target/wasm-gc/debug/test/A/A.internal_test.wasm -test-mode -pkg-sources username/hello/A:./A -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc -g
        "#]],
    );

    check(
        &get_stdout_with_args_and_replace_dir(&dir, ["test", "--dry-run", "--sort-input"]),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target/wasm-gc/debug/test --sort-input
            moonc build-package ./lib2/lib.mbt ./target/wasm-gc/debug/test/lib2/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib2/lib2.internal_test.core -pkg username/hello/lib2 -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib2:./lib2 -target wasm-gc -g
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib2/lib2.internal_test.core -main username/hello/lib2 -o ./target/wasm-gc/debug/test/lib2/lib2.internal_test.wasm -test-mode -pkg-sources username/hello/lib2:./lib2 -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc -g
            moonc build-package ./lib/hello.mbt ./lib/hello_test.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_underscore_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.underscore_test.core -pkg username/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./lib -target wasm-gc -g
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.underscore_test.core -main username/hello/lib -o ./target/wasm-gc/debug/test/lib/lib.underscore_test.wasm -test-mode -pkg-sources username/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc -g
            moonc build-package ./lib/hello.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./lib -target wasm-gc -g
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-sources username/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc -g
            moonc build-package ./A/hello.mbt ./A/test.mbt ./A/hello_test.mbt ./target/wasm-gc/debug/test/A/__generated_driver_for_underscore_test.mbt -o ./target/wasm-gc/debug/test/A/A.underscore_test.core -pkg username/hello/A -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/A:./A -target wasm-gc -g
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/A/A.underscore_test.core -main username/hello/A -o ./target/wasm-gc/debug/test/A/A.underscore_test.wasm -test-mode -pkg-sources username/hello/A:./A -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc -g
            moonc build-package ./A/hello.mbt ./A/test.mbt ./target/wasm-gc/debug/test/A/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/A/A.internal_test.core -pkg username/hello/A -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/A:./A -target wasm-gc -g
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/A/A.internal_test.core -main username/hello/A -o ./target/wasm-gc/debug/test/A/A.internal_test.wasm -test-mode -pkg-sources username/hello/A:./A -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc -g
        "#]],
    );
}

#[test]
fn test_moon_test_filter_file() {
    let dir = TestDir::new("test_filter.in");

    check(
        &get_stdout_with_args(&dir, ["test", "-p", "username/hello/A", "-f", "hello.mbt"]),
        expect![[r#"
            test A
            test B
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );

    check(
        &get_stdout_with_args(
            &dir,
            ["test", "-p", "username/hello/lib", "-f", "hello_test.mbt"],
        ),
        expect![[r#"
            test hello_0
            test hello_1
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_test_filter_index() {
    let dir = TestDir::new("test_filter.in");

    check(
        &get_stdout_with_args(
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
        &get_stdout_with_args(
            &dir,
            [
                "test",
                "-p",
                "username/hello/lib",
                "-f",
                "hello_test.mbt",
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
fn test_moon_test_filter_index_with_auto_update() {
    let dir = TestDir::new("test_filter.in");

    let _ = get_stdout_with_args(
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
        ],
    );
    check(
        &replace_crlf_to_lf(&std::fs::read_to_string(dir.join("lib2").join("lib.mbt")).unwrap()),
        expect![[r#"
            test {
              println(2)
            }

            test {
              inspect(1, content="1")!
              inspect(1 + 2, content="3")!
              inspect("hello", content="hello")!
              inspect([1, 2, 3], content="[1, 2, 3]")!
            }

            test {
              inspect(2)!
            }
        "#]],
    );

    let dir = TestDir::new("test_filter.in");
    let _ = get_stderr_with_args(
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
        ],
    );
    check(
        &replace_crlf_to_lf(&std::fs::read_to_string(dir.join("lib2").join("lib.mbt")).unwrap()),
        expect![[r#"
            test {
              println(2)
            }

            test {
              inspect(1, content="1")!
              inspect(1 + 2, content="3")!
              inspect("hello")!
              inspect([1, 2, 3])!
            }

            test {
              inspect(2)!
            }
        "#]],
    );

    let dir = TestDir::new("test_filter.in");
    let _ = get_stderr_with_args(
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
        ],
    );
    check(
        &replace_crlf_to_lf(&std::fs::read_to_string(dir.join("lib2").join("lib.mbt")).unwrap()),
        expect![[r#"
            test {
              println(2)
            }

            test {
              inspect(1, content="1")!
              inspect(1 + 2)!
              inspect("hello")!
              inspect([1, 2, 3])!
            }

            test {
              inspect(2, content="2")!
            }
        "#]],
    );
}

#[test]
fn test_moon_test_succ() {
    std::env::set_var("NO_COLOR", "1");
    let dir = TestDir::new("moon_test_succ.in");
    check(
        &get_stdout_with_args_and_replace_dir(&dir, ["test", "-v", "--sort-input"]),
        expect![[r#"
            [1001] Warning: Warning: Unused function 'add1'
               ╭─[$ROOT/lib2/nested/lib.mbt:1:4]
               │
             1 │ fn add1(x : Int) -> Int {
               │    ──┬─  
               │      ╰─── Warning: Unused function 'add1'
            ───╯
            test moontest/lib/hello_test.mbt::0 ok
            test moontest/lib2/hello_test.mbt::0 ok
            test moontest/lib2/nested/lib_test.mbt::1 ok
            test moontest/lib2/nested/lib_test.mbt::0 ok
            test moontest/lib3/hello_test.mbt::0 ok
            test moontest/lib4/hello_test.mbt::0 ok
            Total tests: 6, passed: 6, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_test_hello_exec() {
    let dir = TestDir::new("moon_test_hello_exec.in");
    check(
        &get_stdout_with_args(&dir, ["run", "main"]),
        expect![[r#"
            Hello, world!
        "#]],
    );
    check(
        &get_stdout_with_args(&dir, ["test", "-v"]),
        expect![[r#"
            Warning: tests in the main package `moonbitlang/hello/main` will be ignored
            this is lib test
            test moonbitlang/hello/lib/hello_test.mbt::0 ok
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    check(
        &get_stdout_with_args_and_replace_dir(
            &dir,
            ["test", "--dry-run", "--debug", "--sort-input"],
        ),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target/wasm-gc/debug/test --sort-input
            moonc build-package ./lib/hello.mbt ./lib/hello_test.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_underscore_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.underscore_test.core -pkg moonbitlang/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moonbitlang/hello/lib:./lib -target wasm-gc -g
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.underscore_test.core -main moonbitlang/hello/lib -o ./target/wasm-gc/debug/test/lib/lib.underscore_test.wasm -test-mode -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc -g
            moonc build-package ./lib/hello.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.internal_test.core -pkg moonbitlang/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moonbitlang/hello/lib:./lib -target wasm-gc -g
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.internal_test.core -main moonbitlang/hello/lib -o ./target/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc -g
        "#]],
    );
}

#[test]
fn test_moon_test_hello_exec_fntest() {
    let dir = TestDir::new("moon_test_hello_exec_fntest.in");
    check(
        &get_stdout_with_args(&dir, ["run", "main"]),
        expect![[r#"
            init in main/main.mbt
        "#]],
    );

    check(
        &get_stdout_with_args_and_replace_dir(&dir, ["test", "-v", "--dry-run", "--sort-input"]),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target/wasm-gc/debug/test --sort-input
            moonc build-package ./lib/hello.mbt ./lib/hello_test.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_underscore_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.underscore_test.core -pkg moonbitlang/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moonbitlang/hello/lib:./lib -target wasm-gc -g
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.underscore_test.core -main moonbitlang/hello/lib -o ./target/wasm-gc/debug/test/lib/lib.underscore_test.wasm -test-mode -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc -g
            moonc build-package ./lib/hello.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.internal_test.core -pkg moonbitlang/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moonbitlang/hello/lib:./lib -target wasm-gc -g
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.internal_test.core -main moonbitlang/hello/lib -o ./target/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc -g
        "#]],
    );

    check(
        &get_stdout_with_args(&dir, ["test", "-v", "--sort-input"]),
        expect![[r#"
        test in lib/hello.mbt
        test moonbitlang/hello/lib/hello.mbt::0 ok
        test in lib/hello_test.mbt
        test moonbitlang/hello/lib/hello_test.mbt::0 ok
        Total tests: 2, passed: 2, failed: 0.
    "#]],
    );
}

#[test]
fn test_moon_test_hello_lib() {
    let dir = TestDir::new("moon_test_hello_lib.in");
    check(
        &get_stdout_with_args(&dir, ["test", "-v"]),
        expect![[r#"
            test moonbitlang/hello/lib/hello_test.mbt::0 ok
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_test_with_local_dep() {
    let dir = TestDir::new("moon_test_with_local_dep.in");
    check(
        &get_stdout_with_args(&dir, ["test", "-v", "--frozen"]),
        expect![[r#"
            test hello31/lib/hello_test.mbt::0 ok
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    check(
        &get_stdout_with_args(&dir, ["run", "main", "--frozen"]),
        expect![[r#"
            Hello, world!
        "#]],
    );
    // Run moon info
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["info", "--frozen"])
        .assert()
        .success();
    // Check directory structure by listing all files
    let root_dir = dir.as_ref().to_owned();
    let dir = WalkDir::new(&dir)
        .sort_by_file_name()
        .into_iter()
        .filter_map(|e| e.ok())
        .map(|e| e.path().strip_prefix(&root_dir).unwrap().to_owned())
        // Filter out target directory
        .filter(|p| !p.starts_with("target"))
        // Convert to string and join with newline
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    let joined = dir.join("\n").replace('\\', "/"); // Normalize path separator
    check(
        &joined,
        expect![[r#"

            .gitignore
            lib
            lib/hello.mbt
            lib/hello_test.mbt
            lib/lib.mbti
            lib/moon.pkg.json
            main
            main/main.mbt
            main/main.mbti
            main/moon.pkg.json
            mods
            mods/lijunchen
            mods/lijunchen/mooncake
            mods/lijunchen/mooncake/lib
            mods/lijunchen/mooncake/lib/hello.mbt
            mods/lijunchen/mooncake/lib/hello_test.mbt
            mods/lijunchen/mooncake/lib/moon.pkg.json
            mods/lijunchen/mooncake/moon.mod.json
            mods/lijunchen/mooncake/moon.pkg.json
            mods/lijunchen/mooncake/top.mbt
            moon.mod.json"#]],
    );
}

#[test]
fn test_output_format() {
    let dir = TestDir::new("output-format.in");

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["clean"])
        .assert()
        .success();

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["build", "-q"])
        .assert()
        .success();
    assert!(dir
        .join(&format!(
            "target/{}/release/build/main/main.wasm",
            TargetBackend::default().to_backend_ext()
        ))
        .exists());
    assert!(!dir
        .join(&format!(
            "target/{}/release/build/main/main.wat",
            TargetBackend::default().to_backend_ext()
        ))
        .exists());

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["clean"])
        .assert()
        .success();

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["build", "-q", "--output-wat"])
        .assert()
        .success();
    assert!(dir
        .join(&format!(
            "target/{}/release/build/main/main.wat",
            TargetBackend::default().to_backend_ext()
        ))
        .exists());
    assert!(!dir
        .join(&format!(
            "target/{}/release/build/main/main.wasm",
            TargetBackend::default().to_backend_ext()
        ))
        .exists());

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["clean"])
        .assert()
        .success();

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["run", "main"])
        .assert()
        .success();
    assert!(!dir
        .join(&format!(
            "target/{}/release/build/main/main.wat",
            TargetBackend::default().to_backend_ext()
        ))
        .exists());
    assert!(dir
        .join(&format!(
            "target/{}/release/build/main/main.wasm",
            TargetBackend::default().to_backend_ext()
        ))
        .exists());

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["clean"])
        .assert()
        .success();

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["run", "main", "--output-wat"])
        .assert()
        .failure();

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["clean"])
        .assert()
        .success();
}

#[test]
fn test_simple_pkg() {
    let dir = TestDir::new("simple-pkg-A-001.in");
    check(
        &get_stdout_with_args(&dir, ["run", "main"]),
        expect![[r#"
            A
            main
        "#]],
    );

    let dir = TestDir::new("simple-pkg-A-002.in");
    check(
        &get_stdout_with_args(&dir, ["run", "main"]),
        expect![[r#"
            A
            main
        "#]],
    );

    let dir = TestDir::new("simple-pkg-A-003.in");
    check(
        &get_stdout_with_args(&dir, ["run", "main"]),
        expect![[r#"
            A
            main
        "#]],
    );

    let dir = TestDir::new("simple-pkg-A-004.in");
    check(
        &get_stdout_with_args(&dir, ["run", "main"]),
        expect![[r#"
            A
            main
        "#]],
    );

    let dir = TestDir::new("simple-pkg-A-005.in");
    check(
        &get_stdout_with_args(&dir, ["run", "main"]),
        expect![[r#"
            A
            main
        "#]],
    );

    let dir = TestDir::new("simple-pkg-A-006.in");
    check(
        &get_stdout_with_args(&dir, ["run", "main"]),
        expect![[r#"
            main
        "#]],
    );

    let dir = TestDir::new("simple-pkg-AB-001.in");
    check(
        &get_stdout_with_args(&dir, ["run", "main"]),
        expect![[r#"
            A
            B
            main
        "#]],
    );

    let dir = TestDir::new("simple-pkg-AB-002.in");
    check(
        &get_stdout_with_args(&dir, ["run", "main"]),
        expect![[r#"
            A
            B
            main
        "#]],
    );

    let dir = TestDir::new("simple-pkg-AB-003.in");
    check(
        &get_stdout_with_args(&dir, ["run", "main"]),
        expect![[r#"
            A
            B
            main
        "#]],
    );

    let dir = TestDir::new("simple-pkg-AB-004.in");
    check(
        &get_stdout_with_args(&dir, ["run", "main"]),
        expect![[r#"
            A
            B
            main
        "#]],
    );
}

#[test]
fn test_target_backend() {
    let dir = TestDir::new("target-backend.in");
    check(
        &get_stdout_with_args(&dir, ["build", "--dry-run", "--nostd"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc
        "#]],
    );
    check(
        &get_stdout_with_args(
            &dir,
            ["build", "--dry-run", "--target", "wasm-gc", "--nostd"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc
        "#]],
    );
    check(
        &get_stdout_with_args(&dir, ["build", "--dry-run", "--target", "js", "--nostd"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/js/release/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target js
            moonc build-package ./main/main.mbt -o ./target/js/release/build/main/main.core -pkg hello/main -is-main -i ./target/js/release/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target js
            moonc link-core ./target/js/release/build/lib/lib.core ./target/js/release/build/main/main.core -main hello/main -o ./target/js/release/build/main/main.js -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target js
        "#]],
    );
    check(
        &get_stdout_with_args(&dir, ["build", "--dry-run", "--nostd"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc
        "#]],
    );
    check(
        &get_stdout_with_args(&dir, ["run", "main", "--dry-run", "--nostd"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc
            moonrun ./target/wasm-gc/release/build/main/main.wasm
        "#]],
    );
    check(
        &get_stdout_with_args(
            &dir,
            ["run", "main", "--dry-run", "--target", "wasm-gc", "--nostd"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc
            moonrun ./target/wasm-gc/release/build/main/main.wasm
        "#]],
    );
    check(
        &get_stdout_with_args(
            &dir,
            ["run", "main", "--dry-run", "--target", "js", "--nostd"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/js/release/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target js
            moonc build-package ./main/main.mbt -o ./target/js/release/build/main/main.core -pkg hello/main -is-main -i ./target/js/release/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target js
            moonc link-core ./target/js/release/build/lib/lib.core ./target/js/release/build/main/main.core -main hello/main -o ./target/js/release/build/main/main.js -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target js
            node ./target/js/release/build/main/main.js
        "#]],
    );
}

#[test]
fn test_test_error_report() {
    let dir = TestDir::new("test_error_report.in");
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["test"])
        .assert()
        .failure();
}

#[test]
fn test_moonbit_docs_example() {
    let dir = TestDir::new("unicode_demo.in");
    check(
        &get_stdout_with_args(&dir, ["run", "main"]),
        expect![[r#"
            3
        "#]],
    );

    let dir = TestDir::new("palindrome_string.in");
    check(
        &get_stdout_with_args(&dir, ["run", "main"]),
        expect![[r#"
        aba
    "#]],
    );

    let dir = TestDir::new("avl_tree.in");
    check(
        &get_stdout_with_args(&dir, ["run", "main"]),
        expect![[r#"
            height of the tree: 6
                    0
                  1
                    2
                3
                    4
                  5
                    6
              7
                    8
                  9
                    10
                11
                    12
                  13
                    14
            15
                    16
                  17
                    18
                19
                    20
                  21
                    22
              23
                  24
                25
                    26
                  27
                    28
                      29
            success
        "#]],
    );

    let dir = TestDir::new("docstring-demo.in");
    check(
        &get_stdout_with_args(&dir, ["run", "main"]),
        expect![[r#"
            Hello, world!
        "#]],
    );

    let dir = TestDir::new("multidimensional_arrays.in");
    check(
        &get_stdout_with_args(&dir, ["run", "main"]),
        expect![[r#"
        11
    "#]],
    );
}

#[test]
fn test_moon_inline_test_001() {
    let dir = TestDir::new("moon_inline_test_001.in");
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["test"])
        .assert()
        .success();

    let dir = TestDir::new("moon_inline_test_002.in");
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["test"])
        .assert()
        .success();
    let dir = TestDir::new("moon_inline_test_003.in");
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["test"])
        .assert()
        .failure();
}

#[test]
fn test_moon_inline_test_004() {
    let dir = TestDir::new("moon_inline_test_004.in");
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["test"])
        .assert()
        .failure();
}

#[test]
fn test_moon_inline_test_order() {
    let dir = TestDir::new("moon_inline_test_order.in");
    check(
        &get_stdout_with_args(&dir, ["test", "-v", "--sort-input"]),
        expect![[r#"
            executing A
            executing A::hello.mbt::test_A
            test username/hello/A/hello.mbt::1 ok
            test username/hello/A/hello.mbt::0 ok
            A_test.mbt::init
            A_test.mbt::test_hello_A
            test username/hello/A/A_test.mbt::1 ok
            test username/hello/A/A_test.mbt::0 ok
            executing B
            executing B::hello.mbt::test_B
            test username/hello/B/hello.mbt::1 ok
            test username/hello/B/hello.mbt::0 ok
            B_test.mbt::init
            B_test.mbt::test_hello_B
            test username/hello/B/B_test.mbt::1 ok
            test username/hello/B/B_test.mbt::0 ok
            Total tests: 8, passed: 8, failed: 0.
        "#]],
    );

    check(
        &get_stdout_with_args(&dir, ["run", "main", "--sort-input"]),
        expect![[r#"
            main.mbt::init
        "#]],
    );
}

#[test]
fn test_error_duplicate_alias() {
    let dir = TestDir::new("error_duplicate_alias.in");
    let out = get_stderr_with_args(&dir, ["check"]);
    assert!(out.contains("Duplicate alias `lib`"));
}

#[test]
fn test_core_order() {
    let dir = TestDir::new("core_order.in");
    check(
        &get_stdout_with_args(&dir, ["build", "--dry-run", "--nostd"]),
        expect![[r#"
            moonc build-package ./T/t.mbt -o ./target/wasm-gc/release/build/T/T.core -pkg lijunchen/hello/T -pkg-sources lijunchen/hello/T:./T -target wasm-gc
            moonc build-package ./A/a.mbt -o ./target/wasm-gc/release/build/A/A.core -pkg lijunchen/hello/A -i ./target/wasm-gc/release/build/T/T.mi:T -pkg-sources lijunchen/hello/A:./A -target wasm-gc
            moonc build-package ./B/b.mbt -o ./target/wasm-gc/release/build/B/B.core -pkg lijunchen/hello/B -i ./target/wasm-gc/release/build/T/T.mi:T -pkg-sources lijunchen/hello/B:./B -target wasm-gc
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg lijunchen/hello/main -is-main -i ./target/wasm-gc/release/build/A/A.mi:A -i ./target/wasm-gc/release/build/B/B.mi:B -pkg-sources lijunchen/hello/main:./main -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/T/T.core ./target/wasm-gc/release/build/A/A.core ./target/wasm-gc/release/build/B/B.core ./target/wasm-gc/release/build/main/main.core -main lijunchen/hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-sources lijunchen/hello/T:./T -pkg-sources lijunchen/hello/A:./A -pkg-sources lijunchen/hello/B:./B -pkg-sources lijunchen/hello/main:./main -target wasm-gc
        "#]],
    );
}

#[test]
fn test_moon_bundle() {
    let dir = TestDir::new("moon_bundle.in");
    check(
        &get_stdout_with_args(&dir, ["bundle", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./A/lib.mbt -o ./target/wasm-gc/release/bundle/A/A.core -pkg moonbitlang/core/A -pkg-sources moonbitlang/core/A:./A -target wasm-gc
            moonc build-package ./B/lib.mbt -o ./target/wasm-gc/release/bundle/B/B.core -pkg moonbitlang/core/B -i ./target/wasm-gc/release/bundle/A/A.mi:A -pkg-sources moonbitlang/core/B:./B -target wasm-gc
            moonc build-package ./C/lib.mbt -o ./target/wasm-gc/release/bundle/C/C.core -pkg moonbitlang/core/C -i ./target/wasm-gc/release/bundle/A/A.mi:A -pkg-sources moonbitlang/core/C:./C -target wasm-gc
            moonc build-package ./Orphan/lib.mbt -o ./target/wasm-gc/release/bundle/Orphan/Orphan.core -pkg moonbitlang/core/Orphan -pkg-sources moonbitlang/core/Orphan:./Orphan -target wasm-gc
            moonc bundle-core ./target/wasm-gc/release/bundle/A/A.core ./target/wasm-gc/release/bundle/B/B.core ./target/wasm-gc/release/bundle/C/C.core ./target/wasm-gc/release/bundle/Orphan/Orphan.core -o ./target/wasm-gc/release/bundle/core.core
        "#]],
    );
}

#[cfg(unix)]
#[test]
fn test_expect_test() -> anyhow::Result<()> {
    let tmp_dir_path = TestDir::new("expect_test.in");

    let s = snapbox::cmd::Command::new(moon_bin())
        .current_dir(&tmp_dir_path)
        .args(["test", "--update"])
        .assert()
        .success()
        .get_output()
        .stdout
        .to_owned();
    let out = std::str::from_utf8(&s).unwrap().to_string();

    assert!(out.contains("Auto updating expect tests and retesting ..."));
    assert!(out.contains("Total tests: 30, passed: 30, failed: 0."));
    let updated =
        std::fs::read_to_string(tmp_dir_path.as_ref().join("lib").join("hello.mbt")).unwrap();
    assert!(updated.contains(r#"[a, b, c]"#));

    let s = snapbox::cmd::Command::new(moon_bin())
        .current_dir(&tmp_dir_path)
        .args(["test", "--update"])
        .assert()
        .success()
        .get_output()
        .stdout
        .to_owned();

    let out = std::str::from_utf8(&s).unwrap().to_string();
    assert!(out.contains("Total tests: 30, passed: 30, failed: 0."));
    let out =
        std::fs::read_to_string(tmp_dir_path.as_ref().join("lib").join("hello_test.mbt")).unwrap();
    assert!(out.contains(r#"notbuf.expect(content="haha")!"#));
    Ok(())
}

#[test]
fn test_only_update_expect() {
    let tmp_dir_path = TestDir::new("only_update_expect.in");

    let _ = snapbox::cmd::Command::new(moon_bin())
        .current_dir(&tmp_dir_path)
        .args([
            "test",
            "-p",
            "username/hello/lib",
            "-f",
            "hello.mbt",
            "-i",
            "0",
            "--update",
        ])
        .assert()
        .failure()
        .get_output()
        .stdout
        .to_owned();
}

#[test]
fn test_need_link() {
    let dir = TestDir::new("need_link.in");
    check(
        &get_stdout_with_args(&dir, ["build", "--dry-run", "--nostd", "--sort-input"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target wasm-gc
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main username/hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/lib/lib.core -main username/hello/lib -o ./target/wasm-gc/release/build/lib/lib.wasm -pkg-sources username/hello/lib:./lib -target wasm-gc
        "#]],
    );
}

#[test]
fn test_backend_config() {
    let dir = TestDir::new("backend_config.in");

    let _ = get_stdout_with_args(&dir, ["build", "--output-wat"]);
    let out = std::fs::read_to_string(dir.join(&format!(
        "target/{}/release/build/lib/lib.wat",
        TargetBackend::default().to_backend_ext()
    )))
    .unwrap();
    assert!(out.contains(&format!(
        "(export \"hello_{}\")",
        TargetBackend::default().to_backend_ext().replace('-', "_")
    )));
    check(
        &get_stdout_with_args_and_replace_dir(&dir, ["build", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./lib -target wasm-gc
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm-gc
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main username/hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/lib/lib.core -main username/hello/lib -o ./target/wasm-gc/release/build/lib/lib.wasm -pkg-sources username/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc -exported_functions=hello:hello_wasm_gc
        "#]],
    );

    check(
        &get_stdout_with_args(
            &dir,
            [
                "build",
                "--dry-run",
                "--nostd",
                "--target",
                "wasm-gc",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target wasm-gc
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main username/hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/lib/lib.core -main username/hello/lib -o ./target/wasm-gc/release/build/lib/lib.wasm -pkg-sources username/hello/lib:./lib -target wasm-gc -exported_functions=hello:hello_wasm_gc
        "#]],
    );

    check(
        &get_stdout_with_args(
            &dir,
            [
                "build",
                "--dry-run",
                "--nostd",
                "--target",
                "js",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/js/release/build/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target js
            moonc build-package ./main/main.mbt -o ./target/js/release/build/main/main.core -pkg username/hello/main -is-main -i ./target/js/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target js
            moonc link-core ./target/js/release/build/lib/lib.core ./target/js/release/build/main/main.core -main username/hello/main -o ./target/js/release/build/main/main.js -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -target js
            moonc link-core ./target/js/release/build/lib/lib.core -main username/hello/lib -o ./target/js/release/build/lib/lib.js -pkg-sources username/hello/lib:./lib -target js -exported_functions=hello:hello_js -js-format esm
        "#]],
    );
}

#[test]
#[cfg(unix)]
fn test_mbti() {
    let dir = TestDir::new("mbti.in");
    let _ = get_stdout_with_args(&dir, ["info"]);
    let lib_mi_out = &std::fs::read_to_string(dir.join("lib").join("lib.mbti")).unwrap();
    expect![[r#"
        package username/hello/lib

        // Values
        fn hello() -> String

        // Types and methods

        // Traits

        // Extension Methods

    "#]]
    .assert_eq(lib_mi_out);

    let main_mi_out = &std::fs::read_to_string(dir.join("main").join("main.mbti")).unwrap();
    expect![[r#"
        package username/hello/main

        // Values

        // Types and methods

        // Traits

        // Extension Methods

    "#]]
    .assert_eq(main_mi_out);
}

#[test]
fn test_dummy_core() {
    let test_dir = TestDir::new("dummy-core.in");
    let dir = test_dir.as_ref().canonicalize().unwrap();

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["check", "--sort-input"])
        .assert()
        .success();

    #[cfg(unix)]
    {
        let p = dir.join(format!(
            "target/{}/release/check/packages.json",
            TargetBackend::default().to_backend_ext()
        ));
        check(
            &replace_dir(&std::fs::read_to_string(p).unwrap(), &dir),
            expect![[r#"
                {
                  "source_dir": "$ROOT",
                  "name": "moonbitlang/core",
                  "packages": [
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root": "moonbitlang/core",
                      "rel": "0",
                      "files": [
                        "$ROOT/0/lib.mbt",
                        "$ROOT/0/y.js.mbt",
                        "$ROOT/0/y.wasm-gc.mbt",
                        "$ROOT/0/y.wasm.mbt"
                      ],
                      "test-files": [
                        "$ROOT/0/y_test.js.mbt",
                        "$ROOT/0/y_test.mbt",
                        "$ROOT/0/y_test.wasm-gc.mbt",
                        "$ROOT/0/y_test.wasm.mbt"
                      ],
                      "deps": [],
                      "test-deps": [],
                      "artifact": "$ROOT/target/wasm-gc/release/check/0/0.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root": "moonbitlang/core",
                      "rel": "1",
                      "files": [
                        "$ROOT/1/lib.mbt",
                        "$ROOT/1/x.js.mbt",
                        "$ROOT/1/x.wasm-gc.mbt",
                        "$ROOT/1/x.wasm.mbt"
                      ],
                      "test-files": [
                        "$ROOT/1/x_test.wasm-gc.mbt"
                      ],
                      "deps": [],
                      "test-deps": [],
                      "artifact": "$ROOT/target/wasm-gc/release/check/1/1.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root": "moonbitlang/core",
                      "rel": "2",
                      "files": [
                        "$ROOT/2/lib.mbt"
                      ],
                      "test-files": [],
                      "deps": [
                        {
                          "path": "moonbitlang/core/1",
                          "alias": "1"
                        }
                      ],
                      "test-deps": [],
                      "artifact": "$ROOT/target/wasm-gc/release/check/2/2.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root": "moonbitlang/core",
                      "rel": "char",
                      "files": [],
                      "test-files": [],
                      "deps": [
                        {
                          "path": "moonbitlang/core/coverage",
                          "alias": "coverage"
                        }
                      ],
                      "test-deps": [],
                      "artifact": "$ROOT/target/wasm-gc/release/check/char/char.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root": "moonbitlang/core",
                      "rel": "coverage",
                      "files": [],
                      "test-files": [],
                      "deps": [],
                      "test-deps": [],
                      "artifact": "$ROOT/target/wasm-gc/release/check/coverage/coverage.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root": "moonbitlang/core",
                      "rel": "iter",
                      "files": [],
                      "test-files": [],
                      "deps": [
                        {
                          "path": "moonbitlang/core/coverage",
                          "alias": "coverage"
                        }
                      ],
                      "test-deps": [
                        {
                          "path": "moonbitlang/core/char",
                          "alias": "char"
                        }
                      ],
                      "artifact": "$ROOT/target/wasm-gc/release/check/iter/iter.mi"
                    }
                  ],
                  "deps": [],
                  "backend": "wasm-gc"
                }"#]],
        );
    }
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["check", "--target", "js", "--sort-input"])
        .assert()
        .success();

    #[cfg(unix)]
    {
        let p = dir.join("target/js/release/check/packages.json");
        check(
            &replace_dir(&std::fs::read_to_string(p).unwrap(), &dir),
            expect![[r#"
                {
                  "source_dir": "$ROOT",
                  "name": "moonbitlang/core",
                  "packages": [
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root": "moonbitlang/core",
                      "rel": "0",
                      "files": [
                        "$ROOT/0/lib.mbt",
                        "$ROOT/0/y.js.mbt",
                        "$ROOT/0/y.wasm-gc.mbt",
                        "$ROOT/0/y.wasm.mbt"
                      ],
                      "test-files": [
                        "$ROOT/0/y_test.js.mbt",
                        "$ROOT/0/y_test.mbt",
                        "$ROOT/0/y_test.wasm-gc.mbt",
                        "$ROOT/0/y_test.wasm.mbt"
                      ],
                      "deps": [],
                      "test-deps": [],
                      "artifact": "$ROOT/target/js/release/check/0/0.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root": "moonbitlang/core",
                      "rel": "1",
                      "files": [
                        "$ROOT/1/lib.mbt",
                        "$ROOT/1/x.js.mbt",
                        "$ROOT/1/x.wasm-gc.mbt",
                        "$ROOT/1/x.wasm.mbt"
                      ],
                      "test-files": [
                        "$ROOT/1/x_test.wasm-gc.mbt"
                      ],
                      "deps": [],
                      "test-deps": [],
                      "artifact": "$ROOT/target/js/release/check/1/1.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root": "moonbitlang/core",
                      "rel": "2",
                      "files": [
                        "$ROOT/2/lib.mbt"
                      ],
                      "test-files": [],
                      "deps": [
                        {
                          "path": "moonbitlang/core/1",
                          "alias": "1"
                        }
                      ],
                      "test-deps": [],
                      "artifact": "$ROOT/target/js/release/check/2/2.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root": "moonbitlang/core",
                      "rel": "char",
                      "files": [],
                      "test-files": [],
                      "deps": [
                        {
                          "path": "moonbitlang/core/coverage",
                          "alias": "coverage"
                        }
                      ],
                      "test-deps": [],
                      "artifact": "$ROOT/target/js/release/check/char/char.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root": "moonbitlang/core",
                      "rel": "coverage",
                      "files": [],
                      "test-files": [],
                      "deps": [],
                      "test-deps": [],
                      "artifact": "$ROOT/target/js/release/check/coverage/coverage.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root": "moonbitlang/core",
                      "rel": "iter",
                      "files": [],
                      "test-files": [],
                      "deps": [
                        {
                          "path": "moonbitlang/core/coverage",
                          "alias": "coverage"
                        }
                      ],
                      "test-deps": [
                        {
                          "path": "moonbitlang/core/char",
                          "alias": "char"
                        }
                      ],
                      "artifact": "$ROOT/target/js/release/check/iter/iter.mi"
                    }
                  ],
                  "deps": [],
                  "backend": "js"
                }"#]],
        );
    };

    check(
        &get_stdout_with_args_and_replace_dir(&dir, ["check", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc check -o ./target/wasm-gc/release/check/coverage/coverage.mi -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target wasm-gc
            moonc check -o ./target/wasm-gc/release/check/iter/iter.mi -pkg moonbitlang/core/iter -i ./target/wasm-gc/release/check/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target wasm-gc
            moonc check -o ./target/wasm-gc/release/check/char/char.mi -pkg moonbitlang/core/char -i ./target/wasm-gc/release/check/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target wasm-gc
            moonc check ./1/lib.mbt ./1/x.wasm-gc.mbt -o ./target/wasm-gc/release/check/1/1.mi -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm-gc
            moonc check ./2/lib.mbt -o ./target/wasm-gc/release/check/2/2.mi -pkg moonbitlang/core/2 -i ./target/wasm-gc/release/check/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm-gc
            moonc check ./1/lib.mbt ./1/x.wasm-gc.mbt ./1/x_test.wasm-gc.mbt -o ./target/wasm-gc/release/check/1/1.underscore_test.mi -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm-gc
            moonc check ./0/lib.mbt ./0/y.wasm-gc.mbt ./0/y_test.mbt ./0/y_test.wasm-gc.mbt -o ./target/wasm-gc/release/check/0/0.underscore_test.mi -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm-gc
            moonc check ./0/lib.mbt ./0/y.wasm-gc.mbt -o ./target/wasm-gc/release/check/0/0.mi -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm-gc
        "#]],
    );
    check(
        &get_stdout_with_args_and_replace_dir(
            &dir,
            ["check", "--dry-run", "--target", "wasm", "--sort-input"],
        ),
        expect![[r#"
            moonc check -o ./target/wasm/release/check/coverage/coverage.mi -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target wasm
            moonc check -o ./target/wasm/release/check/iter/iter.mi -pkg moonbitlang/core/iter -i ./target/wasm/release/check/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target wasm
            moonc check -o ./target/wasm/release/check/char/char.mi -pkg moonbitlang/core/char -i ./target/wasm/release/check/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target wasm
            moonc check ./1/lib.mbt ./1/x.wasm.mbt -o ./target/wasm/release/check/1/1.mi -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm
            moonc check ./2/lib.mbt -o ./target/wasm/release/check/2/2.mi -pkg moonbitlang/core/2 -i ./target/wasm/release/check/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm
            moonc check ./1/lib.mbt ./1/x.wasm.mbt -o ./target/wasm/release/check/1/1.underscore_test.mi -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm
            moonc check ./0/lib.mbt ./0/y.wasm.mbt ./0/y_test.mbt ./0/y_test.wasm.mbt -o ./target/wasm/release/check/0/0.underscore_test.mi -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm
            moonc check ./0/lib.mbt ./0/y.wasm.mbt -o ./target/wasm/release/check/0/0.mi -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm
        "#]],
    );
    check(
        &get_stdout_with_args_and_replace_dir(
            &dir,
            ["check", "--dry-run", "--target", "wasm-gc", "--sort-input"],
        ),
        expect![[r#"
            moonc check -o ./target/wasm-gc/release/check/coverage/coverage.mi -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target wasm-gc
            moonc check -o ./target/wasm-gc/release/check/iter/iter.mi -pkg moonbitlang/core/iter -i ./target/wasm-gc/release/check/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target wasm-gc
            moonc check -o ./target/wasm-gc/release/check/char/char.mi -pkg moonbitlang/core/char -i ./target/wasm-gc/release/check/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target wasm-gc
            moonc check ./1/lib.mbt ./1/x.wasm-gc.mbt -o ./target/wasm-gc/release/check/1/1.mi -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm-gc
            moonc check ./2/lib.mbt -o ./target/wasm-gc/release/check/2/2.mi -pkg moonbitlang/core/2 -i ./target/wasm-gc/release/check/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm-gc
            moonc check ./1/lib.mbt ./1/x.wasm-gc.mbt ./1/x_test.wasm-gc.mbt -o ./target/wasm-gc/release/check/1/1.underscore_test.mi -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm-gc
            moonc check ./0/lib.mbt ./0/y.wasm-gc.mbt ./0/y_test.mbt ./0/y_test.wasm-gc.mbt -o ./target/wasm-gc/release/check/0/0.underscore_test.mi -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm-gc
            moonc check ./0/lib.mbt ./0/y.wasm-gc.mbt -o ./target/wasm-gc/release/check/0/0.mi -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm-gc
        "#]],
    );
    check(
        &get_stdout_with_args_and_replace_dir(
            &dir,
            ["check", "--dry-run", "--target", "js", "--sort-input"],
        ),
        expect![[r#"
            moonc check -o ./target/js/release/check/coverage/coverage.mi -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target js
            moonc check -o ./target/js/release/check/iter/iter.mi -pkg moonbitlang/core/iter -i ./target/js/release/check/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target js
            moonc check -o ./target/js/release/check/char/char.mi -pkg moonbitlang/core/char -i ./target/js/release/check/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target js
            moonc check ./1/lib.mbt ./1/x.js.mbt -o ./target/js/release/check/1/1.mi -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target js
            moonc check ./2/lib.mbt -o ./target/js/release/check/2/2.mi -pkg moonbitlang/core/2 -i ./target/js/release/check/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target js
            moonc check ./1/lib.mbt ./1/x.js.mbt -o ./target/js/release/check/1/1.underscore_test.mi -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target js
            moonc check ./0/lib.mbt ./0/y.js.mbt ./0/y_test.js.mbt ./0/y_test.mbt -o ./target/js/release/check/0/0.underscore_test.mi -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target js
            moonc check ./0/lib.mbt ./0/y.js.mbt -o ./target/js/release/check/0/0.mi -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target js
        "#]],
    );

    check(
        &get_stdout_with_args_and_replace_dir(&dir, ["build", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./1/lib.mbt ./1/x.wasm-gc.mbt -o ./target/wasm-gc/release/build/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm-gc
            moonc build-package ./2/lib.mbt -o ./target/wasm-gc/release/build/2/2.core -pkg moonbitlang/core/2 -i ./target/wasm-gc/release/build/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/1/1.core ./target/wasm-gc/release/build/2/2.core -main moonbitlang/core/2 -o ./target/wasm-gc/release/build/2/2.wasm -pkg-sources moonbitlang/core/1:./1 -pkg-sources moonbitlang/core/2:./2 -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/1/1.core -main moonbitlang/core/1 -o ./target/wasm-gc/release/build/1/1.wasm -pkg-sources moonbitlang/core/1:./1 -target wasm-gc
            moonc build-package ./0/lib.mbt ./0/y.wasm-gc.mbt -o ./target/wasm-gc/release/build/0/0.core -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/0/0.core -main moonbitlang/core/0 -o ./target/wasm-gc/release/build/0/0.wasm -pkg-sources moonbitlang/core/0:./0 -target wasm-gc
        "#]],
    );
    check(
        &get_stdout_with_args_and_replace_dir(
            &dir,
            ["build", "--dry-run", "--target", "wasm", "--sort-input"],
        ),
        expect![[r#"
            moonc build-package ./1/lib.mbt ./1/x.wasm.mbt -o ./target/wasm/release/build/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm
            moonc build-package ./2/lib.mbt -o ./target/wasm/release/build/2/2.core -pkg moonbitlang/core/2 -i ./target/wasm/release/build/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm
            moonc link-core ./target/wasm/release/build/1/1.core ./target/wasm/release/build/2/2.core -main moonbitlang/core/2 -o ./target/wasm/release/build/2/2.wasm -pkg-sources moonbitlang/core/1:./1 -pkg-sources moonbitlang/core/2:./2 -target wasm
            moonc link-core ./target/wasm/release/build/1/1.core -main moonbitlang/core/1 -o ./target/wasm/release/build/1/1.wasm -pkg-sources moonbitlang/core/1:./1 -target wasm
            moonc build-package ./0/lib.mbt ./0/y.wasm.mbt -o ./target/wasm/release/build/0/0.core -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm
            moonc link-core ./target/wasm/release/build/0/0.core -main moonbitlang/core/0 -o ./target/wasm/release/build/0/0.wasm -pkg-sources moonbitlang/core/0:./0 -target wasm
        "#]],
    );
    check(
        &get_stdout_with_args_and_replace_dir(
            &dir,
            ["build", "--dry-run", "--target", "wasm-gc", "--sort-input"],
        ),
        expect![[r#"
            moonc build-package ./1/lib.mbt ./1/x.wasm-gc.mbt -o ./target/wasm-gc/release/build/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm-gc
            moonc build-package ./2/lib.mbt -o ./target/wasm-gc/release/build/2/2.core -pkg moonbitlang/core/2 -i ./target/wasm-gc/release/build/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/1/1.core ./target/wasm-gc/release/build/2/2.core -main moonbitlang/core/2 -o ./target/wasm-gc/release/build/2/2.wasm -pkg-sources moonbitlang/core/1:./1 -pkg-sources moonbitlang/core/2:./2 -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/1/1.core -main moonbitlang/core/1 -o ./target/wasm-gc/release/build/1/1.wasm -pkg-sources moonbitlang/core/1:./1 -target wasm-gc
            moonc build-package ./0/lib.mbt ./0/y.wasm-gc.mbt -o ./target/wasm-gc/release/build/0/0.core -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/0/0.core -main moonbitlang/core/0 -o ./target/wasm-gc/release/build/0/0.wasm -pkg-sources moonbitlang/core/0:./0 -target wasm-gc
        "#]],
    );
    check(
        &get_stdout_with_args_and_replace_dir(
            &dir,
            ["build", "--dry-run", "--target", "js", "--sort-input"],
        ),
        expect![[r#"
            moonc build-package ./1/lib.mbt ./1/x.js.mbt -o ./target/js/release/build/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target js
            moonc build-package ./2/lib.mbt -o ./target/js/release/build/2/2.core -pkg moonbitlang/core/2 -i ./target/js/release/build/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target js
            moonc link-core ./target/js/release/build/1/1.core ./target/js/release/build/2/2.core -main moonbitlang/core/2 -o ./target/js/release/build/2/2.js -pkg-sources moonbitlang/core/1:./1 -pkg-sources moonbitlang/core/2:./2 -target js
            moonc link-core ./target/js/release/build/1/1.core -main moonbitlang/core/1 -o ./target/js/release/build/1/1.js -pkg-sources moonbitlang/core/1:./1 -target js
            moonc build-package ./0/lib.mbt ./0/y.js.mbt -o ./target/js/release/build/0/0.core -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target js
            moonc link-core ./target/js/release/build/0/0.core -main moonbitlang/core/0 -o ./target/js/release/build/0/0.js -pkg-sources moonbitlang/core/0:./0 -target js
        "#]],
    );

    check(
        &get_stdout_with_args_and_replace_dir(&dir, ["test", "--dry-run", "--sort-input"]),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target/wasm-gc/debug/test --sort-input
            moonc build-package -o ./target/wasm-gc/debug/test/coverage/coverage.core -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target wasm-gc -g
            moonc build-package ./target/wasm-gc/debug/test/iter/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/iter/iter.internal_test.core -pkg moonbitlang/core/iter -is-main -i ./target/wasm-gc/debug/test/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target wasm-gc -g
            moonc link-core ./target/wasm-gc/debug/test/coverage/coverage.core ./target/wasm-gc/debug/test/iter/iter.internal_test.core -main moonbitlang/core/iter -o ./target/wasm-gc/debug/test/iter/iter.internal_test.wasm -test-mode -pkg-sources moonbitlang/core/coverage:./coverage -pkg-sources moonbitlang/core/iter:./iter -target wasm-gc -g
            moonc build-package ./target/wasm-gc/debug/test/coverage/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/coverage/coverage.internal_test.core -pkg moonbitlang/core/coverage -is-main -pkg-sources moonbitlang/core/coverage:./coverage -target wasm-gc -g
            moonc link-core ./target/wasm-gc/debug/test/coverage/coverage.internal_test.core -main moonbitlang/core/coverage -o ./target/wasm-gc/debug/test/coverage/coverage.internal_test.wasm -test-mode -pkg-sources moonbitlang/core/coverage:./coverage -target wasm-gc -g
            moonc build-package ./target/wasm-gc/debug/test/char/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/char/char.internal_test.core -pkg moonbitlang/core/char -is-main -i ./target/wasm-gc/debug/test/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target wasm-gc -g
            moonc link-core ./target/wasm-gc/debug/test/coverage/coverage.core ./target/wasm-gc/debug/test/char/char.internal_test.core -main moonbitlang/core/char -o ./target/wasm-gc/debug/test/char/char.internal_test.wasm -test-mode -pkg-sources moonbitlang/core/coverage:./coverage -pkg-sources moonbitlang/core/char:./char -target wasm-gc -g
            moonc build-package ./1/lib.mbt ./1/x.wasm-gc.mbt -o ./target/wasm-gc/debug/test/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm-gc -g
            moonc build-package ./2/lib.mbt ./target/wasm-gc/debug/test/2/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/2/2.internal_test.core -pkg moonbitlang/core/2 -is-main -i ./target/wasm-gc/debug/test/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm-gc -g
            moonc link-core ./target/wasm-gc/debug/test/1/1.core ./target/wasm-gc/debug/test/2/2.internal_test.core -main moonbitlang/core/2 -o ./target/wasm-gc/debug/test/2/2.internal_test.wasm -test-mode -pkg-sources moonbitlang/core/1:./1 -pkg-sources moonbitlang/core/2:./2 -target wasm-gc -g
            moonc build-package ./1/lib.mbt ./1/x.wasm-gc.mbt ./1/x_test.wasm-gc.mbt ./target/wasm-gc/debug/test/1/__generated_driver_for_underscore_test.mbt -o ./target/wasm-gc/debug/test/1/1.underscore_test.core -pkg moonbitlang/core/1 -is-main -pkg-sources moonbitlang/core/1:./1 -target wasm-gc -g
            moonc link-core ./target/wasm-gc/debug/test/1/1.underscore_test.core -main moonbitlang/core/1 -o ./target/wasm-gc/debug/test/1/1.underscore_test.wasm -test-mode -pkg-sources moonbitlang/core/1:./1 -target wasm-gc -g
            moonc build-package ./1/lib.mbt ./1/x.wasm-gc.mbt ./target/wasm-gc/debug/test/1/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/1/1.internal_test.core -pkg moonbitlang/core/1 -is-main -pkg-sources moonbitlang/core/1:./1 -target wasm-gc -g
            moonc link-core ./target/wasm-gc/debug/test/1/1.internal_test.core -main moonbitlang/core/1 -o ./target/wasm-gc/debug/test/1/1.internal_test.wasm -test-mode -pkg-sources moonbitlang/core/1:./1 -target wasm-gc -g
            moonc build-package ./0/lib.mbt ./0/y.wasm-gc.mbt ./0/y_test.js.mbt ./0/y_test.mbt ./0/y_test.wasm-gc.mbt ./0/y_test.wasm.mbt ./target/wasm-gc/debug/test/0/__generated_driver_for_underscore_test.mbt -o ./target/wasm-gc/debug/test/0/0.underscore_test.core -pkg moonbitlang/core/0 -is-main -pkg-sources moonbitlang/core/0:./0 -target wasm-gc -g
            moonc link-core ./target/wasm-gc/debug/test/0/0.underscore_test.core -main moonbitlang/core/0 -o ./target/wasm-gc/debug/test/0/0.underscore_test.wasm -test-mode -pkg-sources moonbitlang/core/0:./0 -target wasm-gc -g
            moonc build-package ./0/lib.mbt ./0/y.wasm-gc.mbt ./target/wasm-gc/debug/test/0/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/0/0.internal_test.core -pkg moonbitlang/core/0 -is-main -pkg-sources moonbitlang/core/0:./0 -target wasm-gc -g
            moonc link-core ./target/wasm-gc/debug/test/0/0.internal_test.core -main moonbitlang/core/0 -o ./target/wasm-gc/debug/test/0/0.internal_test.wasm -test-mode -pkg-sources moonbitlang/core/0:./0 -target wasm-gc -g
        "#]],
    );
    check(
        &get_stdout_with_args_and_replace_dir(
            &dir,
            ["test", "--dry-run", "--target", "wasm", "--sort-input"],
        ),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target/wasm/debug/test --sort-input
            moonc build-package -o ./target/wasm/debug/test/coverage/coverage.core -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target wasm -g
            moonc build-package ./target/wasm/debug/test/iter/__generated_driver_for_internal_test.mbt -o ./target/wasm/debug/test/iter/iter.internal_test.core -pkg moonbitlang/core/iter -is-main -i ./target/wasm/debug/test/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target wasm -g
            moonc link-core ./target/wasm/debug/test/coverage/coverage.core ./target/wasm/debug/test/iter/iter.internal_test.core -main moonbitlang/core/iter -o ./target/wasm/debug/test/iter/iter.internal_test.wasm -test-mode -pkg-sources moonbitlang/core/coverage:./coverage -pkg-sources moonbitlang/core/iter:./iter -target wasm -g
            moonc build-package ./target/wasm/debug/test/coverage/__generated_driver_for_internal_test.mbt -o ./target/wasm/debug/test/coverage/coverage.internal_test.core -pkg moonbitlang/core/coverage -is-main -pkg-sources moonbitlang/core/coverage:./coverage -target wasm -g
            moonc link-core ./target/wasm/debug/test/coverage/coverage.internal_test.core -main moonbitlang/core/coverage -o ./target/wasm/debug/test/coverage/coverage.internal_test.wasm -test-mode -pkg-sources moonbitlang/core/coverage:./coverage -target wasm -g
            moonc build-package ./target/wasm/debug/test/char/__generated_driver_for_internal_test.mbt -o ./target/wasm/debug/test/char/char.internal_test.core -pkg moonbitlang/core/char -is-main -i ./target/wasm/debug/test/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target wasm -g
            moonc link-core ./target/wasm/debug/test/coverage/coverage.core ./target/wasm/debug/test/char/char.internal_test.core -main moonbitlang/core/char -o ./target/wasm/debug/test/char/char.internal_test.wasm -test-mode -pkg-sources moonbitlang/core/coverage:./coverage -pkg-sources moonbitlang/core/char:./char -target wasm -g
            moonc build-package ./1/lib.mbt ./1/x.wasm.mbt -o ./target/wasm/debug/test/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm -g
            moonc build-package ./2/lib.mbt ./target/wasm/debug/test/2/__generated_driver_for_internal_test.mbt -o ./target/wasm/debug/test/2/2.internal_test.core -pkg moonbitlang/core/2 -is-main -i ./target/wasm/debug/test/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm -g
            moonc link-core ./target/wasm/debug/test/1/1.core ./target/wasm/debug/test/2/2.internal_test.core -main moonbitlang/core/2 -o ./target/wasm/debug/test/2/2.internal_test.wasm -test-mode -pkg-sources moonbitlang/core/1:./1 -pkg-sources moonbitlang/core/2:./2 -target wasm -g
            moonc build-package ./1/lib.mbt ./1/x.wasm.mbt ./1/x_test.wasm-gc.mbt ./target/wasm/debug/test/1/__generated_driver_for_underscore_test.mbt -o ./target/wasm/debug/test/1/1.underscore_test.core -pkg moonbitlang/core/1 -is-main -pkg-sources moonbitlang/core/1:./1 -target wasm -g
            moonc link-core ./target/wasm/debug/test/1/1.underscore_test.core -main moonbitlang/core/1 -o ./target/wasm/debug/test/1/1.underscore_test.wasm -test-mode -pkg-sources moonbitlang/core/1:./1 -target wasm -g
            moonc build-package ./1/lib.mbt ./1/x.wasm.mbt ./target/wasm/debug/test/1/__generated_driver_for_internal_test.mbt -o ./target/wasm/debug/test/1/1.internal_test.core -pkg moonbitlang/core/1 -is-main -pkg-sources moonbitlang/core/1:./1 -target wasm -g
            moonc link-core ./target/wasm/debug/test/1/1.internal_test.core -main moonbitlang/core/1 -o ./target/wasm/debug/test/1/1.internal_test.wasm -test-mode -pkg-sources moonbitlang/core/1:./1 -target wasm -g
            moonc build-package ./0/lib.mbt ./0/y.wasm.mbt ./0/y_test.js.mbt ./0/y_test.mbt ./0/y_test.wasm-gc.mbt ./0/y_test.wasm.mbt ./target/wasm/debug/test/0/__generated_driver_for_underscore_test.mbt -o ./target/wasm/debug/test/0/0.underscore_test.core -pkg moonbitlang/core/0 -is-main -pkg-sources moonbitlang/core/0:./0 -target wasm -g
            moonc link-core ./target/wasm/debug/test/0/0.underscore_test.core -main moonbitlang/core/0 -o ./target/wasm/debug/test/0/0.underscore_test.wasm -test-mode -pkg-sources moonbitlang/core/0:./0 -target wasm -g
            moonc build-package ./0/lib.mbt ./0/y.wasm.mbt ./target/wasm/debug/test/0/__generated_driver_for_internal_test.mbt -o ./target/wasm/debug/test/0/0.internal_test.core -pkg moonbitlang/core/0 -is-main -pkg-sources moonbitlang/core/0:./0 -target wasm -g
            moonc link-core ./target/wasm/debug/test/0/0.internal_test.core -main moonbitlang/core/0 -o ./target/wasm/debug/test/0/0.internal_test.wasm -test-mode -pkg-sources moonbitlang/core/0:./0 -target wasm -g
        "#]],
    );
    check(
        &get_stdout_with_args_and_replace_dir(
            &dir,
            ["test", "--dry-run", "--target", "wasm-gc", "--sort-input"],
        ),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target/wasm-gc/debug/test --sort-input
            moonc build-package -o ./target/wasm-gc/debug/test/coverage/coverage.core -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target wasm-gc -g
            moonc build-package ./target/wasm-gc/debug/test/iter/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/iter/iter.internal_test.core -pkg moonbitlang/core/iter -is-main -i ./target/wasm-gc/debug/test/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target wasm-gc -g
            moonc link-core ./target/wasm-gc/debug/test/coverage/coverage.core ./target/wasm-gc/debug/test/iter/iter.internal_test.core -main moonbitlang/core/iter -o ./target/wasm-gc/debug/test/iter/iter.internal_test.wasm -test-mode -pkg-sources moonbitlang/core/coverage:./coverage -pkg-sources moonbitlang/core/iter:./iter -target wasm-gc -g
            moonc build-package ./target/wasm-gc/debug/test/coverage/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/coverage/coverage.internal_test.core -pkg moonbitlang/core/coverage -is-main -pkg-sources moonbitlang/core/coverage:./coverage -target wasm-gc -g
            moonc link-core ./target/wasm-gc/debug/test/coverage/coverage.internal_test.core -main moonbitlang/core/coverage -o ./target/wasm-gc/debug/test/coverage/coverage.internal_test.wasm -test-mode -pkg-sources moonbitlang/core/coverage:./coverage -target wasm-gc -g
            moonc build-package ./target/wasm-gc/debug/test/char/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/char/char.internal_test.core -pkg moonbitlang/core/char -is-main -i ./target/wasm-gc/debug/test/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target wasm-gc -g
            moonc link-core ./target/wasm-gc/debug/test/coverage/coverage.core ./target/wasm-gc/debug/test/char/char.internal_test.core -main moonbitlang/core/char -o ./target/wasm-gc/debug/test/char/char.internal_test.wasm -test-mode -pkg-sources moonbitlang/core/coverage:./coverage -pkg-sources moonbitlang/core/char:./char -target wasm-gc -g
            moonc build-package ./1/lib.mbt ./1/x.wasm-gc.mbt -o ./target/wasm-gc/debug/test/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm-gc -g
            moonc build-package ./2/lib.mbt ./target/wasm-gc/debug/test/2/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/2/2.internal_test.core -pkg moonbitlang/core/2 -is-main -i ./target/wasm-gc/debug/test/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm-gc -g
            moonc link-core ./target/wasm-gc/debug/test/1/1.core ./target/wasm-gc/debug/test/2/2.internal_test.core -main moonbitlang/core/2 -o ./target/wasm-gc/debug/test/2/2.internal_test.wasm -test-mode -pkg-sources moonbitlang/core/1:./1 -pkg-sources moonbitlang/core/2:./2 -target wasm-gc -g
            moonc build-package ./1/lib.mbt ./1/x.wasm-gc.mbt ./1/x_test.wasm-gc.mbt ./target/wasm-gc/debug/test/1/__generated_driver_for_underscore_test.mbt -o ./target/wasm-gc/debug/test/1/1.underscore_test.core -pkg moonbitlang/core/1 -is-main -pkg-sources moonbitlang/core/1:./1 -target wasm-gc -g
            moonc link-core ./target/wasm-gc/debug/test/1/1.underscore_test.core -main moonbitlang/core/1 -o ./target/wasm-gc/debug/test/1/1.underscore_test.wasm -test-mode -pkg-sources moonbitlang/core/1:./1 -target wasm-gc -g
            moonc build-package ./1/lib.mbt ./1/x.wasm-gc.mbt ./target/wasm-gc/debug/test/1/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/1/1.internal_test.core -pkg moonbitlang/core/1 -is-main -pkg-sources moonbitlang/core/1:./1 -target wasm-gc -g
            moonc link-core ./target/wasm-gc/debug/test/1/1.internal_test.core -main moonbitlang/core/1 -o ./target/wasm-gc/debug/test/1/1.internal_test.wasm -test-mode -pkg-sources moonbitlang/core/1:./1 -target wasm-gc -g
            moonc build-package ./0/lib.mbt ./0/y.wasm-gc.mbt ./0/y_test.js.mbt ./0/y_test.mbt ./0/y_test.wasm-gc.mbt ./0/y_test.wasm.mbt ./target/wasm-gc/debug/test/0/__generated_driver_for_underscore_test.mbt -o ./target/wasm-gc/debug/test/0/0.underscore_test.core -pkg moonbitlang/core/0 -is-main -pkg-sources moonbitlang/core/0:./0 -target wasm-gc -g
            moonc link-core ./target/wasm-gc/debug/test/0/0.underscore_test.core -main moonbitlang/core/0 -o ./target/wasm-gc/debug/test/0/0.underscore_test.wasm -test-mode -pkg-sources moonbitlang/core/0:./0 -target wasm-gc -g
            moonc build-package ./0/lib.mbt ./0/y.wasm-gc.mbt ./target/wasm-gc/debug/test/0/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/0/0.internal_test.core -pkg moonbitlang/core/0 -is-main -pkg-sources moonbitlang/core/0:./0 -target wasm-gc -g
            moonc link-core ./target/wasm-gc/debug/test/0/0.internal_test.core -main moonbitlang/core/0 -o ./target/wasm-gc/debug/test/0/0.internal_test.wasm -test-mode -pkg-sources moonbitlang/core/0:./0 -target wasm-gc -g
        "#]],
    );
    check(
        &get_stdout_with_args_and_replace_dir(
            &dir,
            ["test", "--dry-run", "--target", "js", "--sort-input"],
        ),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target/js/debug/test --sort-input
            moonc build-package -o ./target/js/debug/test/coverage/coverage.core -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target js -g -ryu
            moonc build-package ./target/js/debug/test/iter/__generated_driver_for_internal_test.mbt -o ./target/js/debug/test/iter/iter.internal_test.core -pkg moonbitlang/core/iter -is-main -i ./target/js/debug/test/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target js -g -ryu
            moonc link-core ./target/js/debug/test/coverage/coverage.core ./target/js/debug/test/iter/iter.internal_test.core -main moonbitlang/core/iter -o ./target/js/debug/test/iter/iter.internal_test.js -test-mode -pkg-sources moonbitlang/core/coverage:./coverage -pkg-sources moonbitlang/core/iter:./iter -target js -g -ryu
            moonc build-package ./target/js/debug/test/coverage/__generated_driver_for_internal_test.mbt -o ./target/js/debug/test/coverage/coverage.internal_test.core -pkg moonbitlang/core/coverage -is-main -pkg-sources moonbitlang/core/coverage:./coverage -target js -g -ryu
            moonc link-core ./target/js/debug/test/coverage/coverage.internal_test.core -main moonbitlang/core/coverage -o ./target/js/debug/test/coverage/coverage.internal_test.js -test-mode -pkg-sources moonbitlang/core/coverage:./coverage -target js -g -ryu
            moonc build-package ./target/js/debug/test/char/__generated_driver_for_internal_test.mbt -o ./target/js/debug/test/char/char.internal_test.core -pkg moonbitlang/core/char -is-main -i ./target/js/debug/test/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target js -g -ryu
            moonc link-core ./target/js/debug/test/coverage/coverage.core ./target/js/debug/test/char/char.internal_test.core -main moonbitlang/core/char -o ./target/js/debug/test/char/char.internal_test.js -test-mode -pkg-sources moonbitlang/core/coverage:./coverage -pkg-sources moonbitlang/core/char:./char -target js -g -ryu
            moonc build-package ./1/lib.mbt ./1/x.js.mbt -o ./target/js/debug/test/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target js -g -ryu
            moonc build-package ./2/lib.mbt ./target/js/debug/test/2/__generated_driver_for_internal_test.mbt -o ./target/js/debug/test/2/2.internal_test.core -pkg moonbitlang/core/2 -is-main -i ./target/js/debug/test/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target js -g -ryu
            moonc link-core ./target/js/debug/test/1/1.core ./target/js/debug/test/2/2.internal_test.core -main moonbitlang/core/2 -o ./target/js/debug/test/2/2.internal_test.js -test-mode -pkg-sources moonbitlang/core/1:./1 -pkg-sources moonbitlang/core/2:./2 -target js -g -ryu
            moonc build-package ./1/lib.mbt ./1/x.js.mbt ./1/x_test.wasm-gc.mbt ./target/js/debug/test/1/__generated_driver_for_underscore_test.mbt -o ./target/js/debug/test/1/1.underscore_test.core -pkg moonbitlang/core/1 -is-main -pkg-sources moonbitlang/core/1:./1 -target js -g -ryu
            moonc link-core ./target/js/debug/test/1/1.underscore_test.core -main moonbitlang/core/1 -o ./target/js/debug/test/1/1.underscore_test.js -test-mode -pkg-sources moonbitlang/core/1:./1 -target js -g -ryu
            moonc build-package ./1/lib.mbt ./1/x.js.mbt ./target/js/debug/test/1/__generated_driver_for_internal_test.mbt -o ./target/js/debug/test/1/1.internal_test.core -pkg moonbitlang/core/1 -is-main -pkg-sources moonbitlang/core/1:./1 -target js -g -ryu
            moonc link-core ./target/js/debug/test/1/1.internal_test.core -main moonbitlang/core/1 -o ./target/js/debug/test/1/1.internal_test.js -test-mode -pkg-sources moonbitlang/core/1:./1 -target js -g -ryu
            moonc build-package ./0/lib.mbt ./0/y.js.mbt ./0/y_test.js.mbt ./0/y_test.mbt ./0/y_test.wasm-gc.mbt ./0/y_test.wasm.mbt ./target/js/debug/test/0/__generated_driver_for_underscore_test.mbt -o ./target/js/debug/test/0/0.underscore_test.core -pkg moonbitlang/core/0 -is-main -pkg-sources moonbitlang/core/0:./0 -target js -g -ryu
            moonc link-core ./target/js/debug/test/0/0.underscore_test.core -main moonbitlang/core/0 -o ./target/js/debug/test/0/0.underscore_test.js -test-mode -pkg-sources moonbitlang/core/0:./0 -target js -g -ryu
            moonc build-package ./0/lib.mbt ./0/y.js.mbt ./target/js/debug/test/0/__generated_driver_for_internal_test.mbt -o ./target/js/debug/test/0/0.internal_test.core -pkg moonbitlang/core/0 -is-main -pkg-sources moonbitlang/core/0:./0 -target js -g -ryu
            moonc link-core ./target/js/debug/test/0/0.internal_test.core -main moonbitlang/core/0 -o ./target/js/debug/test/0/0.internal_test.js -test-mode -pkg-sources moonbitlang/core/0:./0 -target js -g -ryu
        "#]],
    );
    check(
        &get_stdout_with_args_and_replace_dir(
            &dir,
            ["test", "--dry-run", "--enable-coverage", "--sort-input"],
        ),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target/wasm-gc/debug/test --sort-input
            moonc build-package -o ./target/wasm-gc/debug/test/coverage/coverage.core -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target wasm-gc -g -enable-coverage -coverage-package-override=@self
            moonc build-package ./target/wasm-gc/debug/test/iter/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/iter/iter.internal_test.core -pkg moonbitlang/core/iter -is-main -i ./target/wasm-gc/debug/test/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target wasm-gc -g -enable-coverage
            moonc link-core ./target/wasm-gc/debug/test/coverage/coverage.core ./target/wasm-gc/debug/test/iter/iter.internal_test.core -main moonbitlang/core/iter -o ./target/wasm-gc/debug/test/iter/iter.internal_test.wasm -test-mode -pkg-sources moonbitlang/core/coverage:./coverage -pkg-sources moonbitlang/core/iter:./iter -target wasm-gc -g
            moonc build-package ./target/wasm-gc/debug/test/coverage/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/coverage/coverage.internal_test.core -pkg moonbitlang/core/coverage -is-main -pkg-sources moonbitlang/core/coverage:./coverage -target wasm-gc -g -enable-coverage -coverage-package-override=@self
            moonc link-core ./target/wasm-gc/debug/test/coverage/coverage.internal_test.core -main moonbitlang/core/coverage -o ./target/wasm-gc/debug/test/coverage/coverage.internal_test.wasm -test-mode -pkg-sources moonbitlang/core/coverage:./coverage -target wasm-gc -g
            moonc build-package ./target/wasm-gc/debug/test/char/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/char/char.internal_test.core -pkg moonbitlang/core/char -is-main -i ./target/wasm-gc/debug/test/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target wasm-gc -g -enable-coverage
            moonc link-core ./target/wasm-gc/debug/test/coverage/coverage.core ./target/wasm-gc/debug/test/char/char.internal_test.core -main moonbitlang/core/char -o ./target/wasm-gc/debug/test/char/char.internal_test.wasm -test-mode -pkg-sources moonbitlang/core/coverage:./coverage -pkg-sources moonbitlang/core/char:./char -target wasm-gc -g
            moonc build-package ./1/lib.mbt ./1/x.wasm-gc.mbt -o ./target/wasm-gc/debug/test/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm-gc -g -enable-coverage
            moonc build-package ./2/lib.mbt ./target/wasm-gc/debug/test/2/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/2/2.internal_test.core -pkg moonbitlang/core/2 -is-main -i ./target/wasm-gc/debug/test/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm-gc -g -enable-coverage
            moonc link-core ./target/wasm-gc/debug/test/1/1.core ./target/wasm-gc/debug/test/2/2.internal_test.core -main moonbitlang/core/2 -o ./target/wasm-gc/debug/test/2/2.internal_test.wasm -test-mode -pkg-sources moonbitlang/core/1:./1 -pkg-sources moonbitlang/core/2:./2 -target wasm-gc -g
            moonc build-package ./1/lib.mbt ./1/x.wasm-gc.mbt ./1/x_test.wasm-gc.mbt ./target/wasm-gc/debug/test/1/__generated_driver_for_underscore_test.mbt -o ./target/wasm-gc/debug/test/1/1.underscore_test.core -pkg moonbitlang/core/1 -is-main -pkg-sources moonbitlang/core/1:./1 -target wasm-gc -g -enable-coverage
            moonc link-core ./target/wasm-gc/debug/test/1/1.underscore_test.core -main moonbitlang/core/1 -o ./target/wasm-gc/debug/test/1/1.underscore_test.wasm -test-mode -pkg-sources moonbitlang/core/1:./1 -target wasm-gc -g
            moonc build-package ./1/lib.mbt ./1/x.wasm-gc.mbt ./target/wasm-gc/debug/test/1/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/1/1.internal_test.core -pkg moonbitlang/core/1 -is-main -pkg-sources moonbitlang/core/1:./1 -target wasm-gc -g -enable-coverage
            moonc link-core ./target/wasm-gc/debug/test/1/1.internal_test.core -main moonbitlang/core/1 -o ./target/wasm-gc/debug/test/1/1.internal_test.wasm -test-mode -pkg-sources moonbitlang/core/1:./1 -target wasm-gc -g
            moonc build-package ./0/lib.mbt ./0/y.wasm-gc.mbt ./0/y_test.js.mbt ./0/y_test.mbt ./0/y_test.wasm-gc.mbt ./0/y_test.wasm.mbt ./target/wasm-gc/debug/test/0/__generated_driver_for_underscore_test.mbt -o ./target/wasm-gc/debug/test/0/0.underscore_test.core -pkg moonbitlang/core/0 -is-main -pkg-sources moonbitlang/core/0:./0 -target wasm-gc -g -enable-coverage
            moonc link-core ./target/wasm-gc/debug/test/0/0.underscore_test.core -main moonbitlang/core/0 -o ./target/wasm-gc/debug/test/0/0.underscore_test.wasm -test-mode -pkg-sources moonbitlang/core/0:./0 -target wasm-gc -g
            moonc build-package ./0/lib.mbt ./0/y.wasm-gc.mbt ./target/wasm-gc/debug/test/0/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/0/0.internal_test.core -pkg moonbitlang/core/0 -is-main -pkg-sources moonbitlang/core/0:./0 -target wasm-gc -g -enable-coverage
            moonc link-core ./target/wasm-gc/debug/test/0/0.internal_test.core -main moonbitlang/core/0 -o ./target/wasm-gc/debug/test/0/0.internal_test.wasm -test-mode -pkg-sources moonbitlang/core/0:./0 -target wasm-gc -g
        "#]],
    );
    check(
        &get_stdout_with_args_and_replace_dir(&dir, ["bundle", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./1/lib.mbt ./1/x.wasm-gc.mbt -o ./target/wasm-gc/release/bundle/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm-gc
            moonc build-package -o ./target/wasm-gc/release/bundle/coverage/coverage.core -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target wasm-gc
            moonc build-package ./0/lib.mbt ./0/y.wasm-gc.mbt -o ./target/wasm-gc/release/bundle/0/0.core -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm-gc
            moonc build-package ./2/lib.mbt -o ./target/wasm-gc/release/bundle/2/2.core -pkg moonbitlang/core/2 -i ./target/wasm-gc/release/bundle/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm-gc
            moonc build-package -o ./target/wasm-gc/release/bundle/char/char.core -pkg moonbitlang/core/char -i ./target/wasm-gc/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target wasm-gc
            moonc build-package -o ./target/wasm-gc/release/bundle/iter/iter.core -pkg moonbitlang/core/iter -i ./target/wasm-gc/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target wasm-gc
            moonc bundle-core ./target/wasm-gc/release/bundle/0/0.core ./target/wasm-gc/release/bundle/1/1.core ./target/wasm-gc/release/bundle/2/2.core ./target/wasm-gc/release/bundle/coverage/coverage.core ./target/wasm-gc/release/bundle/char/char.core ./target/wasm-gc/release/bundle/iter/iter.core -o ./target/wasm-gc/release/bundle/core.core
        "#]],
    );
    check(
        &get_stdout_with_args_and_replace_dir(
            &dir,
            ["bundle", "--dry-run", "--target", "wasm", "--sort-input"],
        ),
        expect![[r#"
            moonc build-package ./1/lib.mbt ./1/x.wasm.mbt -o ./target/wasm/release/bundle/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm
            moonc build-package -o ./target/wasm/release/bundle/coverage/coverage.core -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target wasm
            moonc build-package ./0/lib.mbt ./0/y.wasm.mbt -o ./target/wasm/release/bundle/0/0.core -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm
            moonc build-package ./2/lib.mbt -o ./target/wasm/release/bundle/2/2.core -pkg moonbitlang/core/2 -i ./target/wasm/release/bundle/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm
            moonc build-package -o ./target/wasm/release/bundle/char/char.core -pkg moonbitlang/core/char -i ./target/wasm/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target wasm
            moonc build-package -o ./target/wasm/release/bundle/iter/iter.core -pkg moonbitlang/core/iter -i ./target/wasm/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target wasm
            moonc bundle-core ./target/wasm/release/bundle/0/0.core ./target/wasm/release/bundle/1/1.core ./target/wasm/release/bundle/2/2.core ./target/wasm/release/bundle/coverage/coverage.core ./target/wasm/release/bundle/char/char.core ./target/wasm/release/bundle/iter/iter.core -o ./target/wasm/release/bundle/core.core
        "#]],
    );
    check(
        &get_stdout_with_args_and_replace_dir(
            &dir,
            ["bundle", "--dry-run", "--target", "wasm-gc", "--sort-input"],
        ),
        expect![[r#"
            moonc build-package ./1/lib.mbt ./1/x.wasm-gc.mbt -o ./target/wasm-gc/release/bundle/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm-gc
            moonc build-package -o ./target/wasm-gc/release/bundle/coverage/coverage.core -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target wasm-gc
            moonc build-package ./0/lib.mbt ./0/y.wasm-gc.mbt -o ./target/wasm-gc/release/bundle/0/0.core -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm-gc
            moonc build-package ./2/lib.mbt -o ./target/wasm-gc/release/bundle/2/2.core -pkg moonbitlang/core/2 -i ./target/wasm-gc/release/bundle/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm-gc
            moonc build-package -o ./target/wasm-gc/release/bundle/char/char.core -pkg moonbitlang/core/char -i ./target/wasm-gc/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target wasm-gc
            moonc build-package -o ./target/wasm-gc/release/bundle/iter/iter.core -pkg moonbitlang/core/iter -i ./target/wasm-gc/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target wasm-gc
            moonc bundle-core ./target/wasm-gc/release/bundle/0/0.core ./target/wasm-gc/release/bundle/1/1.core ./target/wasm-gc/release/bundle/2/2.core ./target/wasm-gc/release/bundle/coverage/coverage.core ./target/wasm-gc/release/bundle/char/char.core ./target/wasm-gc/release/bundle/iter/iter.core -o ./target/wasm-gc/release/bundle/core.core
        "#]],
    );
    check(
        &get_stdout_with_args_and_replace_dir(
            &dir,
            ["bundle", "--dry-run", "--target", "js", "--sort-input"],
        ),
        expect![[r#"
            moonc build-package ./1/lib.mbt ./1/x.js.mbt -o ./target/js/release/bundle/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target js
            moonc build-package -o ./target/js/release/bundle/coverage/coverage.core -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target js
            moonc build-package ./0/lib.mbt ./0/y.js.mbt -o ./target/js/release/bundle/0/0.core -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target js
            moonc build-package ./2/lib.mbt -o ./target/js/release/bundle/2/2.core -pkg moonbitlang/core/2 -i ./target/js/release/bundle/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target js
            moonc build-package -o ./target/js/release/bundle/char/char.core -pkg moonbitlang/core/char -i ./target/js/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target js
            moonc build-package -o ./target/js/release/bundle/iter/iter.core -pkg moonbitlang/core/iter -i ./target/js/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target js
            moonc bundle-core ./target/js/release/bundle/0/0.core ./target/js/release/bundle/1/1.core ./target/js/release/bundle/2/2.core ./target/js/release/bundle/coverage/coverage.core ./target/js/release/bundle/char/char.core ./target/js/release/bundle/iter/iter.core -o ./target/js/release/bundle/core.core
        "#]],
    );
    check(
        &get_stdout_with_args_and_replace_dir(
            &dir,
            ["bundle", "--all", "--dry-run", "--sort-input"],
        ),
        expect![[r#"
            moonc build-package ./1/lib.mbt ./1/x.wasm.mbt -o ./target/wasm/release/bundle/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm
            moonc build-package -o ./target/wasm/release/bundle/coverage/coverage.core -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target wasm
            moonc build-package ./0/lib.mbt ./0/y.wasm.mbt -o ./target/wasm/release/bundle/0/0.core -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm
            moonc build-package ./2/lib.mbt -o ./target/wasm/release/bundle/2/2.core -pkg moonbitlang/core/2 -i ./target/wasm/release/bundle/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm
            moonc build-package -o ./target/wasm/release/bundle/char/char.core -pkg moonbitlang/core/char -i ./target/wasm/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target wasm
            moonc build-package -o ./target/wasm/release/bundle/iter/iter.core -pkg moonbitlang/core/iter -i ./target/wasm/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target wasm
            moonc bundle-core ./target/wasm/release/bundle/0/0.core ./target/wasm/release/bundle/1/1.core ./target/wasm/release/bundle/2/2.core ./target/wasm/release/bundle/coverage/coverage.core ./target/wasm/release/bundle/char/char.core ./target/wasm/release/bundle/iter/iter.core -o ./target/wasm/release/bundle/core.core
            moonc build-package ./1/lib.mbt ./1/x.wasm-gc.mbt -o ./target/wasm-gc/release/bundle/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm-gc
            moonc build-package -o ./target/wasm-gc/release/bundle/coverage/coverage.core -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target wasm-gc
            moonc build-package ./0/lib.mbt ./0/y.wasm-gc.mbt -o ./target/wasm-gc/release/bundle/0/0.core -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm-gc
            moonc build-package ./2/lib.mbt -o ./target/wasm-gc/release/bundle/2/2.core -pkg moonbitlang/core/2 -i ./target/wasm-gc/release/bundle/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm-gc
            moonc build-package -o ./target/wasm-gc/release/bundle/char/char.core -pkg moonbitlang/core/char -i ./target/wasm-gc/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target wasm-gc
            moonc build-package -o ./target/wasm-gc/release/bundle/iter/iter.core -pkg moonbitlang/core/iter -i ./target/wasm-gc/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target wasm-gc
            moonc bundle-core ./target/wasm-gc/release/bundle/0/0.core ./target/wasm-gc/release/bundle/1/1.core ./target/wasm-gc/release/bundle/2/2.core ./target/wasm-gc/release/bundle/coverage/coverage.core ./target/wasm-gc/release/bundle/char/char.core ./target/wasm-gc/release/bundle/iter/iter.core -o ./target/wasm-gc/release/bundle/core.core
            moonc build-package ./1/lib.mbt ./1/x.js.mbt -o ./target/js/release/bundle/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target js
            moonc build-package -o ./target/js/release/bundle/coverage/coverage.core -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target js
            moonc build-package ./0/lib.mbt ./0/y.js.mbt -o ./target/js/release/bundle/0/0.core -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target js
            moonc build-package ./2/lib.mbt -o ./target/js/release/bundle/2/2.core -pkg moonbitlang/core/2 -i ./target/js/release/bundle/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target js
            moonc build-package -o ./target/js/release/bundle/char/char.core -pkg moonbitlang/core/char -i ./target/js/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target js
            moonc build-package -o ./target/js/release/bundle/iter/iter.core -pkg moonbitlang/core/iter -i ./target/js/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target js
            moonc bundle-core ./target/js/release/bundle/0/0.core ./target/js/release/bundle/1/1.core ./target/js/release/bundle/2/2.core ./target/js/release/bundle/coverage/coverage.core ./target/js/release/bundle/char/char.core ./target/js/release/bundle/iter/iter.core -o ./target/js/release/bundle/core.core
        "#]],
    );
}

#[test]
#[ignore = "not implemented"]
fn test_backend_flag() {
    let dir = TestDir::new("backend-flag.in");

    check(
        &get_stdout_with_args_and_replace_dir(&dir, ["check", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc check ./lib/hello.mbt -o ./target/js/release/check/lib/lib.mi -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/js/release/bundle -pkg-sources username/hello/lib:./lib
            moonc check ./main/main.mbt -o ./target/js/release/check/main/main.mi -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/js/release/bundle -i ./target/js/release/check/lib/lib.mi:lib -pkg-sources username/hello/main:./main
            moonc check ./lib/hello.mbt ./lib/hello_test.mbt -o ./target/js/release/check/lib/lib.underscore_test.mi -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/js/release/bundle -pkg-sources username/hello/lib:./lib
        "#]],
    );

    check(
        &get_stdout_with_args_and_replace_dir(&dir, ["build", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/js/release/build/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/js/release/bundle -pkg-sources username/hello/lib:./lib
            moonc build-package ./main/main.mbt -o ./target/js/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/js/release/bundle -i ./target/js/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main
            moonc link-core $MOON_HOME/lib/core/target/js/release/bundle/core.core ./target/js/release/build/lib/lib.core ./target/js/release/build/main/main.core -main username/hello/main -o ./target/js/release/build/main/main.js -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -target js
        "#]],
    );

    check(
        &get_stdout_with_args_and_replace_dir(&dir, ["test", "--dry-run", "--sort-input"]),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target/js/debug/test
            moonc build-package ./lib/hello.mbt ./lib/hello_test.mbt ./target/js/debug/test/lib/__generated_driver_for_underscore_test.mbt -o ./target/js/debug/test/lib/lib.underscore_test.core -pkg username/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/js/release/bundle -pkg-sources username/hello/lib:./lib -g -ryu
            moonc link-core $MOON_HOME/lib/core/target/js/release/bundle/core.core ./target/js/debug/test/lib/lib.underscore_test.core -main username/hello/lib -o ./target/js/debug/test/lib/lib.underscore_test.js -test-mode -pkg-sources username/hello/lib:./lib -target js -ryu
            moonc build-package ./lib/hello.mbt ./target/js/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/js/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/js/release/bundle -pkg-sources username/hello/lib:./lib -g -ryu
            moonc link-core $MOON_HOME/lib/core/target/js/release/bundle/core.core ./target/js/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/js/debug/test/lib/lib.internal_test.js -test-mode -pkg-sources username/hello/lib:./lib -target js -ryu
        "#]],
    );

    check(
        &get_stdout_with_args_and_replace_dir(&dir, ["bundle", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/js/release/bundle/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib
            moonc build-package ./main/main.mbt -o ./target/js/release/bundle/main/main.core -pkg username/hello/main -is-main -i ./target/js/release/bundle/lib/lib.mi:lib -pkg-sources username/hello/main:./main
            moonc bundle-core ./target/js/release/bundle/lib/lib.core ./target/js/release/bundle/main/main.core -o ./target/js/release/bundle/hello.core
        "#]],
    );
}

#[test]
fn test_source_map() {
    let dir = TestDir::new("hello.in");

    // no -source-map in wasm backend
    check(
        &get_stdout_with_args_and_replace_dir(
            &dir,
            [
                "build",
                "--target",
                "wasm",
                "--debug",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc build-package ./main/main.mbt -o ./target/wasm/debug/build/main/main.core -pkg hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm/release/bundle -pkg-sources hello/main:./main -target wasm -g
            moonc link-core $MOON_HOME/lib/core/target/wasm/release/bundle/core.core ./target/wasm/debug/build/main/main.core -main hello/main -o ./target/wasm/debug/build/main/main.wasm -pkg-sources hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm -g
        "#]],
    );
    check(
        &get_stdout_with_args_and_replace_dir(
            &dir,
            [
                "build",
                "--target",
                "wasm-gc",
                "--debug",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/build/main/main.core -pkg hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources hello/main:./main -target wasm-gc -g -source-map
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/build/main/main.core -main hello/main -o ./target/wasm-gc/debug/build/main/main.wasm -pkg-sources hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc -g -source-map
        "#]],
    );
    check(
        &get_stdout_with_args_and_replace_dir(
            &dir,
            [
                "build",
                "--target",
                "js",
                "--debug",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc build-package ./main/main.mbt -o ./target/js/debug/build/main/main.core -pkg hello/main -is-main -std-path $MOON_HOME/lib/core/target/js/release/bundle -pkg-sources hello/main:./main -target js -g -source-map
            moonc link-core $MOON_HOME/lib/core/target/js/release/bundle/core.core ./target/js/debug/build/main/main.core -main hello/main -o ./target/js/debug/build/main/main.js -pkg-sources hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target js -g -source-map
        "#]],
    );
}

#[test]
fn test_find_ancestor_with_mod() {
    let dir = TestDir::new("hello.in");

    check(
        &get_stdout_with_args_and_replace_dir(&dir, ["run", "main"]),
        expect![[r#"
            Hello, world!
        "#]],
    );

    check(
        &get_stdout_with_args_and_replace_dir(&dir, ["run", "main"]),
        expect![[r#"
            Hello, world!
        "#]],
    );
}

#[test]
fn test_js_format() {
    let dir = TestDir::new("js_format.in");
    check(
        &get_stdout_with_args_and_replace_dir(
            &dir,
            [
                "build",
                "--target",
                "js",
                "--sort-input",
                "--dry-run",
                "--nostd",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib3/hello.mbt -o ./target/js/release/build/lib3/lib3.core -pkg username/hello/lib3 -pkg-sources username/hello/lib3:./lib3 -target js
            moonc link-core ./target/js/release/build/lib3/lib3.core -main username/hello/lib3 -o ./target/js/release/build/lib3/lib3.js -pkg-sources username/hello/lib3:./lib3 -target js -exported_functions=hello -js-format iife
            moonc build-package ./lib2/hello.mbt -o ./target/js/release/build/lib2/lib2.core -pkg username/hello/lib2 -pkg-sources username/hello/lib2:./lib2 -target js
            moonc link-core ./target/js/release/build/lib2/lib2.core -main username/hello/lib2 -o ./target/js/release/build/lib2/lib2.js -pkg-sources username/hello/lib2:./lib2 -target js -exported_functions=hello -js-format cjs
            moonc build-package ./lib1/hello.mbt -o ./target/js/release/build/lib1/lib1.core -pkg username/hello/lib1 -pkg-sources username/hello/lib1:./lib1 -target js
            moonc link-core ./target/js/release/build/lib1/lib1.core -main username/hello/lib1 -o ./target/js/release/build/lib1/lib1.js -pkg-sources username/hello/lib1:./lib1 -target js -exported_functions=hello -js-format esm
            moonc build-package ./lib0/hello.mbt -o ./target/js/release/build/lib0/lib0.core -pkg username/hello/lib0 -pkg-sources username/hello/lib0:./lib0 -target js
            moonc link-core ./target/js/release/build/lib0/lib0.core -main username/hello/lib0 -o ./target/js/release/build/lib0/lib0.js -pkg-sources username/hello/lib0:./lib0 -target js -exported_functions=hello -js-format esm
        "#]],
    );
    let _ = get_stdout_with_args_and_replace_dir(&dir, ["build", "--target", "js", "--nostd"]);
    let t = dir.join("target").join("js").join("release").join("build");
    check(
        &std::fs::read_to_string(t.join("lib0").join("lib0.js"))
            .unwrap()
            .replace("\r\n", "\n"),
        expect![[r#"
            function username$hello$lib0$$hello() {
              return "Hello, world!";
            }
            export { username$hello$lib0$$hello as hello }
        "#]],
    );
    check(
        &std::fs::read_to_string(t.join("lib1").join("lib1.js"))
            .unwrap()
            .replace("\r\n", "\n"),
        expect![[r#"
            function username$hello$lib1$$hello() {
              return "Hello, world!";
            }
            export { username$hello$lib1$$hello as hello }
        "#]],
    );
    check(
        &std::fs::read_to_string(t.join("lib2").join("lib2.js"))
            .unwrap()
            .replace("\r\n", "\n"),
        expect![[r#"
            function username$hello$lib2$$hello() {
              return "Hello, world!";
            }
            exports.hello = username$hello$lib2$$hello;
        "#]],
    );
    check(
        &std::fs::read_to_string(t.join("lib3").join("lib3.js"))
            .unwrap()
            .replace("\r\n", "\n"),
        expect![[r#"
            (() => {
              function username$hello$lib3$$hello() {
                return "Hello, world!";
              }
              globalThis.hello = username$hello$lib3$$hello;
            })();
        "#]],
    );
}

#[test]
fn test_warn_list() {
    let dir = TestDir::new("warn_list.in");

    check(
        &get_stdout_with_args_and_replace_dir(&dir, ["build", "--sort-input", "--no-render"]),
        expect![[r#"
            moonc build-package $ROOT/lib/hello.mbt -w -2 -o $ROOT/target/wasm-gc/release/build/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:$ROOT/lib -target wasm-gc
            moonc build-package $ROOT/lib1/hello.mbt -w -1 -o $ROOT/target/wasm-gc/release/build/lib1/lib1.core -pkg username/hello/lib1 -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib1:$ROOT/lib1 -target wasm-gc
            moonc build-package $ROOT/main/main.mbt -w -1-2 -o $ROOT/target/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i $ROOT/target/wasm-gc/release/build/lib/lib.mi:lib -i $ROOT/target/wasm-gc/release/build/lib1/lib1.mi:lib1 -pkg-sources username/hello/main:$ROOT/main -target wasm-gc
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core $ROOT/target/wasm-gc/release/build/lib/lib.core $ROOT/target/wasm-gc/release/build/lib1/lib1.core $ROOT/target/wasm-gc/release/build/main/main.core -main username/hello/main -o $ROOT/target/wasm-gc/release/build/main/main.wasm -pkg-sources username/hello/lib:$ROOT/lib -pkg-sources username/hello/lib1:$ROOT/lib1 -pkg-sources username/hello/main:$ROOT/main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc
            moon: ran 4 tasks, now up to date
        "#]],
    );

    check(
        &get_stdout_with_args_and_replace_dir(&dir, ["test", "--sort-input"]),
        expect![[r#"
            Total tests: 0, passed: 0, failed: 0.
        "#]],
    );

    check(
        &get_stdout_with_args_and_replace_dir(&dir, ["bundle", "--sort-input", "--no-render"]),
        expect![[r#"
            moonc build-package $ROOT/lib/hello.mbt -w -2 -o $ROOT/target/wasm-gc/release/bundle/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:$ROOT/lib -target wasm-gc
            moonc build-package $ROOT/lib1/hello.mbt -w -1 -o $ROOT/target/wasm-gc/release/bundle/lib1/lib1.core -pkg username/hello/lib1 -pkg-sources username/hello/lib1:$ROOT/lib1 -target wasm-gc
            moonc build-package $ROOT/main/main.mbt -w -1-2 -o $ROOT/target/wasm-gc/release/bundle/main/main.core -pkg username/hello/main -is-main -i $ROOT/target/wasm-gc/release/bundle/lib/lib.mi:lib -i $ROOT/target/wasm-gc/release/bundle/lib1/lib1.mi:lib1 -pkg-sources username/hello/main:$ROOT/main -target wasm-gc
            moonc bundle-core $ROOT/target/wasm-gc/release/bundle/lib/lib.core $ROOT/target/wasm-gc/release/bundle/lib1/lib1.core $ROOT/target/wasm-gc/release/bundle/main/main.core -o $ROOT/target/wasm-gc/release/bundle/hello.core
            moon: ran 4 tasks, now up to date
        "#]],
    );

    // to cover `moon bundle` no work to do
    get_stdout_with_args_and_replace_dir(&dir, ["bundle", "--sort-input"]);

    check(
        &get_stdout_with_args_and_replace_dir(&dir, ["check", "--sort-input", "--no-render"]),
        expect![[r#"
            moonc check $ROOT/lib/hello.mbt -w -2 -o $ROOT/target/wasm-gc/release/check/lib/lib.mi -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:$ROOT/lib -target wasm-gc
            moonc check $ROOT/lib1/hello.mbt -w -1 -o $ROOT/target/wasm-gc/release/check/lib1/lib1.mi -pkg username/hello/lib1 -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib1:$ROOT/lib1 -target wasm-gc
            moonc check $ROOT/main/main.mbt -w -1-2 -o $ROOT/target/wasm-gc/release/check/main/main.mi -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i $ROOT/target/wasm-gc/release/check/lib/lib.mi:lib -i $ROOT/target/wasm-gc/release/check/lib1/lib1.mi:lib1 -pkg-sources username/hello/main:$ROOT/main -target wasm-gc
            moon: ran 3 tasks, now up to date
        "#]],
    );
}

#[test]
fn test_alert_list() {
    std::env::set_var("NO_COLOR", "1");
    let dir = TestDir::new("alert_list.in");

    check(
        &get_stdout_with_args_and_replace_dir(&dir, ["build", "--sort-input"]),
        expect![[r#"
            moonc build-package -error-format json $ROOT/lib/hello.mbt -w -2 -alert -alert_1-alert_2 -o $ROOT/target/wasm-gc/release/build/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:$ROOT/lib -target wasm-gc
            moonc build-package -error-format json $ROOT/main/main.mbt -w -1-2 -alert -alert_1 -o $ROOT/target/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i $ROOT/target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:$ROOT/main -target wasm-gc
            [2000] Warning: Warning (Alert alert_2): alert_2
               ╭─[$ROOT/main/main.mbt:3:3]
               │
             3 │   alert_2();
               │   ───┬───  
               │      ╰───── Warning (Alert alert_2): alert_2
            ───╯
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core $ROOT/target/wasm-gc/release/build/lib/lib.core $ROOT/target/wasm-gc/release/build/main/main.core -main username/hello/main -o $ROOT/target/wasm-gc/release/build/main/main.wasm -pkg-sources username/hello/lib:$ROOT/lib -pkg-sources username/hello/main:$ROOT/main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc
            moon: ran 3 tasks, now up to date
        "#]],
    );

    check(
        &get_stdout_with_args_and_replace_dir(&dir, ["test", "--sort-input"]),
        expect![[r#"
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        &get_stdout_with_args_and_replace_dir(&dir, ["bundle", "--sort-input"]),
        expect![[r#"
            moonc build-package -error-format json $ROOT/lib/hello.mbt -w -2 -alert -alert_1-alert_2 -o $ROOT/target/wasm-gc/release/bundle/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:$ROOT/lib -target wasm-gc
            moonc build-package -error-format json $ROOT/main/main.mbt -w -1-2 -alert -alert_1 -o $ROOT/target/wasm-gc/release/bundle/main/main.core -pkg username/hello/main -is-main -i $ROOT/target/wasm-gc/release/bundle/lib/lib.mi:lib -pkg-sources username/hello/main:$ROOT/main -target wasm-gc
            [2000] Warning: Warning (Alert alert_2): alert_2
               ╭─[$ROOT/main/main.mbt:3:3]
               │
             3 │   alert_2();
               │   ───┬───  
               │      ╰───── Warning (Alert alert_2): alert_2
            ───╯
            moonc bundle-core $ROOT/target/wasm-gc/release/bundle/lib/lib.core $ROOT/target/wasm-gc/release/bundle/main/main.core -o $ROOT/target/wasm-gc/release/bundle/hello.core
            moon: ran 3 tasks, now up to date
        "#]],
    );

    check(
        &get_stdout_with_args_and_replace_dir(&dir, ["check", "--sort-input"]),
        expect![[r#"
            moonc check -error-format json $ROOT/lib/hello.mbt -w -2 -alert -alert_1-alert_2 -o $ROOT/target/wasm-gc/release/check/lib/lib.mi -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:$ROOT/lib -target wasm-gc
            moonc check -error-format json $ROOT/main/main.mbt -w -1-2 -alert -alert_1 -o $ROOT/target/wasm-gc/release/check/main/main.mi -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i $ROOT/target/wasm-gc/release/check/lib/lib.mi:lib -pkg-sources username/hello/main:$ROOT/main -target wasm-gc
            [2000] Warning: Warning (Alert alert_2): alert_2
               ╭─[$ROOT/main/main.mbt:3:3]
               │
             3 │   alert_2();
               │   ───┬───  
               │      ╰───── Warning (Alert alert_2): alert_2
            ───╯
            moon: ran 2 tasks, now up to date
        "#]],
    );
}

#[test]
fn test_no_work_to_do() {
    let dir = TestDir::new("moon_new.in");
    let out = get_stdout_with_args_and_replace_dir(&dir, ["check"]);
    assert!(out.contains("now up to date"));

    let out = get_stdout_with_args_and_replace_dir(&dir, ["check"]);
    assert!(out.contains("moon: no work to do"));

    let out = get_stdout_with_args_and_replace_dir(&dir, ["build"]);
    assert!(out.contains("now up to date"));
    let out = get_stdout_with_args_and_replace_dir(&dir, ["build"]);
    assert!(out.contains("moon: no work to do"));
}

#[test]
fn test_moon_test_release() {
    let dir = TestDir::new("test_release.in");

    check(
        &get_stdout_with_args_and_replace_dir(&dir, ["test", "--dry-run", "--sort-input"]),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target/wasm-gc/debug/test --sort-input
            moonc build-package ./lib/hello.mbt ./lib/hello_test.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_underscore_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.underscore_test.core -pkg username/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./lib -target wasm-gc -g
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.underscore_test.core -main username/hello/lib -o ./target/wasm-gc/debug/test/lib/lib.underscore_test.wasm -test-mode -pkg-sources username/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc -g
            moonc build-package ./lib/hello.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./lib -target wasm-gc -g
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-sources username/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc -g
        "#]],
    );

    check(
        &get_stdout_with_args_and_replace_dir(
            &dir,
            ["test", "--release", "--dry-run", "--sort-input"],
        ),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target/wasm-gc/release/test --sort-input
            moonc build-package ./lib/hello.mbt ./lib/hello_test.mbt ./target/wasm-gc/release/test/lib/__generated_driver_for_underscore_test.mbt -o ./target/wasm-gc/release/test/lib/lib.underscore_test.core -pkg username/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./lib -target wasm-gc
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/test/lib/lib.underscore_test.core -main username/hello/lib -o ./target/wasm-gc/release/test/lib/lib.underscore_test.wasm -test-mode -pkg-sources username/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc
            moonc build-package ./lib/hello.mbt ./target/wasm-gc/release/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/release/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./lib -target wasm-gc
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/wasm-gc/release/test/lib/lib.internal_test.wasm -test-mode -pkg-sources username/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc
        "#]],
    );

    check(
        &get_stdout_with_args_and_replace_dir(&dir, ["test", "--release", "--sort-input"]),
        expect![[r#"
            test A
            test hello_0
            test hello_1
            Total tests: 3, passed: 3, failed: 0.
        "#]],
    );
}

#[test]
fn test_backtrace() {
    let dir = TestDir::new("backtrace.in");

    let out = get_stderr_with_args(&dir, ["run", "main"]);
    assert!(!out.contains("main.foo.fn"));
    assert!(!out.contains("main.bar.fn"));

    let out = get_stderr_with_args(&dir, ["run", "main", "--debug"]);
    assert!(out.contains("main.foo.fn"));
    assert!(out.contains("main.bar.fn"));
}

#[test]
fn test_deny_warn() {
    std::env::set_var("NO_COLOR", "1");
    let dir = TestDir::new("test_deny_warn.in");

    check(
        &get_stdout_with_args_and_replace_dir(&dir, ["check", "--sort-input"]),
        expect![[r#"
            moonc check -error-format json $ROOT/lib/hello.mbt -o $ROOT/target/wasm-gc/release/check/lib/lib.mi -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:$ROOT/lib -target wasm-gc
            [2000] Warning: Warning (Alert alert_2): alert_2
                ╭─[$ROOT/lib/hello.mbt:14:3]
                │
             14 │   alert_2();
                │   ───┬───  
                │      ╰───── Warning (Alert alert_2): alert_2
            ────╯
            [2000] Warning: Warning (Alert alert_1): alert_1
                ╭─[$ROOT/lib/hello.mbt:13:3]
                │
             13 │   alert_1();
                │   ───┬───  
                │      ╰───── Warning (Alert alert_1): alert_1
            ────╯
            [1002] Warning: Warning: Unused variable 'a'
               ╭─[$ROOT/lib/hello.mbt:4:7]
               │
             4 │   let a = 1;
               │       ┬  
               │       ╰── Warning: Unused variable 'a'
            ───╯
            [1002] Warning: Warning: Unused variable '中文'
                ╭─[$ROOT/lib/hello.mbt:11:7]
                │
             11 │   let 中文 = 2
                │       ──┬─  
                │         ╰─── Warning: Unused variable '中文'
            ────╯
            [1002] Warning: Warning: Unused variable '🤣😭🤣😭🤣'
                ╭─[$ROOT/lib/hello.mbt:12:7]
                │
             12 │   let 🤣😭🤣😭🤣 = 2
                │       ────┬─────  
                │           ╰─────── Warning: Unused variable '🤣😭🤣😭🤣'
            ────╯
            moonc check -error-format json $ROOT/main/main.mbt -o $ROOT/target/wasm-gc/release/check/main/main.mi -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i $ROOT/target/wasm-gc/release/check/lib/lib.mi:lib -pkg-sources username/hello/main:$ROOT/main -target wasm-gc
            [1002] Warning: Warning: Unused variable 'a'
               ╭─[$ROOT/main/main.mbt:2:7]
               │
             2 │   let a = 0
               │       ┬  
               │       ╰── Warning: Unused variable 'a'
            ───╯
            moon: ran 2 tasks, now up to date
        "#]],
    );

    let out = snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["check", "--deny-warn", "--sort-input"])
        .assert()
        .failure()
        .get_output()
        .stdout
        .to_owned();

    let s = std::str::from_utf8(&out).unwrap().to_string();

    assert!(s.contains(
        "failed: moonc check -error-format json -w @a -alert @all-raise-throw-unsafe+deprecated"
    ));

    check(
        &get_stdout_with_args_and_replace_dir(&dir, ["build", "--sort-input"]),
        expect![[r#"
            moonc build-package -error-format json $ROOT/lib/hello.mbt -o $ROOT/target/wasm-gc/release/build/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:$ROOT/lib -target wasm-gc
            [2000] Warning: Warning (Alert alert_2): alert_2
                ╭─[$ROOT/lib/hello.mbt:14:3]
                │
             14 │   alert_2();
                │   ───┬───  
                │      ╰───── Warning (Alert alert_2): alert_2
            ────╯
            [2000] Warning: Warning (Alert alert_1): alert_1
                ╭─[$ROOT/lib/hello.mbt:13:3]
                │
             13 │   alert_1();
                │   ───┬───  
                │      ╰───── Warning (Alert alert_1): alert_1
            ────╯
            [1002] Warning: Warning: Unused variable 'a'
               ╭─[$ROOT/lib/hello.mbt:4:7]
               │
             4 │   let a = 1;
               │       ┬  
               │       ╰── Warning: Unused variable 'a'
            ───╯
            [1002] Warning: Warning: Unused variable '中文'
                ╭─[$ROOT/lib/hello.mbt:11:7]
                │
             11 │   let 中文 = 2
                │       ──┬─  
                │         ╰─── Warning: Unused variable '中文'
            ────╯
            [1002] Warning: Warning: Unused variable '🤣😭🤣😭🤣'
                ╭─[$ROOT/lib/hello.mbt:12:7]
                │
             12 │   let 🤣😭🤣😭🤣 = 2
                │       ────┬─────  
                │           ╰─────── Warning: Unused variable '🤣😭🤣😭🤣'
            ────╯
            moonc build-package -error-format json $ROOT/main/main.mbt -o $ROOT/target/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i $ROOT/target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:$ROOT/main -target wasm-gc
            [1002] Warning: Warning: Unused variable 'a'
               ╭─[$ROOT/main/main.mbt:2:7]
               │
             2 │   let a = 0
               │       ┬  
               │       ╰── Warning: Unused variable 'a'
            ───╯
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core $ROOT/target/wasm-gc/release/build/lib/lib.core $ROOT/target/wasm-gc/release/build/main/main.core -main username/hello/main -o $ROOT/target/wasm-gc/release/build/main/main.wasm -pkg-sources username/hello/lib:$ROOT/lib -pkg-sources username/hello/main:$ROOT/main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc
            moon: ran 3 tasks, now up to date
        "#]],
    );

    let out = snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["build", "--deny-warn", "--sort-input"])
        .assert()
        .failure()
        .get_output()
        .stdout
        .to_owned();

    let s = std::str::from_utf8(&out).unwrap().to_string();

    assert!(
        s.contains("failed: moonc build-package -error-format json -w @a -alert @all-raise-throw-unsafe+deprecated")
    );
}

#[test]
fn test_moon_test_no_entry_warning() {
    let dir = TestDir::new("moon_test_no_entry_warning.in");

    let out = snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["test"])
        .assert()
        .success()
        .get_output()
        .stderr
        .to_owned();

    check(
        std::str::from_utf8(&out).unwrap(),
        expect![[r#"
            Warning: no test entry found
        "#]],
    );
}

#[test]
fn test_moon_fmt() {
    fn read(p: &Path) -> String {
        std::fs::read_to_string(p).unwrap().replace('\r', "")
    }

    {
        let dir = TestDir::new("moon_fmt.in");
        check(
            &read(&dir.join("lib").join("hello.mbt")),
            expect![[r#"
                pub fn hello() -> String { "Hello, world!" }
            "#]],
        );
        check(
            &read(&dir.join("main").join("main.mbt")),
            expect![[r#"
                fn main { println(@lib.hello()) }"#]],
        );
        let _ = get_stdout_with_args_and_replace_dir(&dir, ["fmt"]);
        check(
            &read(&dir.join("lib").join("hello.mbt")),
            expect![[r#"
                pub fn hello() -> String {
                  "Hello, world!"
                }
            "#]],
        );
        check(
            &read(&dir.join("main").join("main.mbt")),
            expect![[r#"
                fn main {
                  println(@lib.hello())
                }
            "#]],
        );
    }

    {
        let dir = TestDir::new("moon_fmt.in");
        let _ = snapbox::cmd::Command::new(moon_bin())
            .current_dir(&dir)
            .args(["fmt", "--check"])
            .assert()
            .failure()
            .get_output()
            .stdout
            .to_owned();
        check(
            &read(&dir.join("lib").join("hello.mbt")),
            expect![[r#"
                pub fn hello() -> String { "Hello, world!" }
            "#]],
        );
        check(
            &read(&dir.join("main").join("main.mbt")),
            expect![[r#"
                fn main { println(@lib.hello()) }"#]],
        );
        check(
            &read(
                &dir.join("target")
                    .join(TargetBackend::default().to_dir_name())
                    .join("release")
                    .join("format")
                    .join("lib")
                    .join("hello.mbt"),
            ),
            expect![[r#"
                pub fn hello() -> String {
                  "Hello, world!"
                }
            "#]],
        );
        check(
            &read(
                &dir.join("target")
                    .join(TargetBackend::default().to_dir_name())
                    .join("release")
                    .join("format")
                    .join("main")
                    .join("main.mbt"),
            ),
            expect![[r#"
                fn main {
                  println(@lib.hello())
                }
            "#]],
        );
    }
}

#[test]
fn test_export_memory_name() {
    let dir = TestDir::new("export_memory.in");
    let _ = get_stdout_with_args_and_replace_dir(
        &dir,
        ["build", "--target", "wasm-gc", "--output-wat"],
    );
    let content = std::fs::read_to_string(
        dir.join("target")
            .join("wasm-gc")
            .join("release")
            .join("build")
            .join("main")
            .join("main.wat"),
    )
    .unwrap();
    assert!(content.contains("awesome_memory"));

    check(
        &get_stdout_with_args_and_replace_dir(&dir, ["build", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./lib -target wasm-gc
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm-gc
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main username/hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc -export-memory-name awesome_memory
        "#]],
    );

    check(
        &get_stdout_with_args_and_replace_dir(
            &dir,
            ["build", "--dry-run", "--sort-input", "--target", "wasm"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm/release/build/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm/release/bundle -pkg-sources username/hello/lib:./lib -target wasm
            moonc build-package ./main/main.mbt -o ./target/wasm/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm/release/bundle -i ./target/wasm/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm
            moonc link-core $MOON_HOME/lib/core/target/wasm/release/bundle/core.core ./target/wasm/release/build/lib/lib.core ./target/wasm/release/build/main/main.core -main username/hello/main -o ./target/wasm/release/build/main/main.wasm -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm -export-memory-name awesome_memory
        "#]],
    );

    check(
        &get_stdout_with_args_and_replace_dir(
            &dir,
            ["build", "--dry-run", "--sort-input", "--target", "js"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/js/release/build/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/js/release/bundle -pkg-sources username/hello/lib:./lib -target js
            moonc build-package ./main/main.mbt -o ./target/js/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/js/release/bundle -i ./target/js/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target js
            moonc link-core $MOON_HOME/lib/core/target/js/release/bundle/core.core ./target/js/release/build/lib/lib.core ./target/js/release/build/main/main.core -main username/hello/main -o ./target/js/release/build/main/main.js -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target js
        "#]],
    );
}

#[test]
fn test_no_block_params() {
    let dir = TestDir::new("no_block_params.in");
    check(
        &get_stdout_with_args_and_replace_dir(&dir, ["build", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./lib -target wasm-gc
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm-gc
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main username/hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc -no-block-params
        "#]],
    );

    check(
        &get_stdout_with_args_and_replace_dir(
            &dir,
            ["build", "--dry-run", "--sort-input", "--target", "wasm"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm/release/build/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm/release/bundle -pkg-sources username/hello/lib:./lib -target wasm
            moonc build-package ./main/main.mbt -o ./target/wasm/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm/release/bundle -i ./target/wasm/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm
            moonc link-core $MOON_HOME/lib/core/target/wasm/release/bundle/core.core ./target/wasm/release/build/lib/lib.core ./target/wasm/release/build/main/main.core -main username/hello/main -o ./target/wasm/release/build/main/main.wasm -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm -no-block-params
        "#]],
    );

    check(
        &get_stdout_with_args_and_replace_dir(
            &dir,
            ["build", "--dry-run", "--sort-input", "--target", "js"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/js/release/build/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/js/release/bundle -pkg-sources username/hello/lib:./lib -target js
            moonc build-package ./main/main.mbt -o ./target/js/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/js/release/bundle -i ./target/js/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target js
            moonc link-core $MOON_HOME/lib/core/target/js/release/bundle/core.core ./target/js/release/build/lib/lib.core ./target/js/release/build/main/main.core -main username/hello/main -o ./target/js/release/build/main/main.js -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target js
        "#]],
    );
}

#[test]
fn test_panic() {
    let dir = TestDir::new("panic.in");
    let data = snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["test"])
        .assert()
        .failure()
        .get_output()
        .stdout
        .to_owned();
    let out = String::from_utf8_lossy(&data).to_string();
    check(
        &out,
        expect![[r#"
            test username/hello/lib/hello_test.mbt::panic failed: panic is expected
            Total tests: 2, passed: 1, failed: 1.
        "#]],
    );
}

#[test]
fn test_validate_import() {
    let dir = TestDir::new("validate_import.in");
    check(
        &get_stderr_with_args_and_replace_dir(&dir, ["check"]),
        expect![[r#"
            error: failed to read import path in "$ROOT/main/moon.pkg.json"

            Caused by:
                No matching module was found for mbt/core/set
        "#]],
    );
    check(
        &get_stderr_with_args_and_replace_dir(&dir, ["build"]),
        expect![[r#"
            error: failed to read import path in "$ROOT/main/moon.pkg.json"

            Caused by:
                No matching module was found for mbt/core/set
        "#]],
    );
    check(
        &get_stderr_with_args_and_replace_dir(&dir, ["test"]),
        expect![[r#"
            error: failed to read import path in "$ROOT/main/moon.pkg.json"

            Caused by:
                No matching module was found for mbt/core/set
        "#]],
    );
    check(
        &get_stderr_with_args_and_replace_dir(&dir, ["bundle"]),
        expect![[r#"
            error: failed to read import path in "$ROOT/main/moon.pkg.json"

            Caused by:
                No matching module was found for mbt/core/set
        "#]],
    );
}

#[test]
fn test_multi_process() {
    use std::process::Command;
    use std::thread;

    let dir = TestDir::new("test_multi_process");
    let path: PathBuf = dir.as_ref().into();

    let (num_threads, inner_loop) = (16, 10);
    let mut container = vec![];

    let success = std::sync::Arc::new(std::sync::atomic::AtomicI32::new(0));

    for _ in 0..num_threads {
        let path = path.clone();
        let success = success.clone();
        let work = thread::spawn(move || {
            for _ in 0..inner_loop {
                let _ = std::fs::OpenOptions::new()
                    .append(true)
                    .open(path.join("lib/hello.mbt"))
                    .unwrap()
                    .write(b"\n")
                    .unwrap();

                let output = Command::new(moon_bin())
                    .arg("check")
                    .current_dir(path.clone())
                    .output()
                    .expect("Failed to execute command");

                if output.status.success() {
                    success.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    let out = String::from_utf8(output.stdout).unwrap();
                    assert!(out.contains("no work to do") || out.contains("now up to date"));
                } else {
                    println!("moon output: {:?}", String::from_utf8(output.stdout));
                    let error_message = String::from_utf8_lossy(&output.stderr);
                    println!("{}", error_message);
                }
            }
        });
        container.push(work);
    }

    for i in container {
        i.join().unwrap();
    }

    assert_eq!(
        success.load(std::sync::atomic::Ordering::SeqCst),
        num_threads * inner_loop
    );
}

#[test]
fn test_internal_package() {
    let dir = TestDir::new("internal_package.in");
    check(
        &get_stderr_with_args_and_replace_dir(&dir, ["check"]),
        expect![[r#"
            error: $ROOT/lib2/moon.pkg.json: cannot import internal package `username/hello/lib/internal` in `username/hello/lib2`
            $ROOT/lib2/moon.pkg.json: cannot import internal package `username/hello/lib/internal/b` in `username/hello/lib2`
            $ROOT/main/moon.pkg.json: cannot import internal package `username/hello/lib/internal` in `username/hello/main`
        "#]],
    );
}

#[test]
fn mooncakes_io_smoke_test() {
    let dir = TestDir::new("hello.in");
    let _ = get_stdout_with_args(&dir, ["update"]);
    let _ = get_stdout_with_args(&dir, ["add", "lijunchen/hello2@0.1.0"]);
    check(
        &std::fs::read_to_string(dir.join("moon.mod.json")).unwrap(),
        expect![[r#"
            {
              "name": "hello",
              "deps": {
                "lijunchen/hello2": "0.1.0"
              }
            }"#]],
    );
    let _ = get_stdout_with_args(&dir, ["remove", "lijunchen/hello2"]);
    check(
        &std::fs::read_to_string(dir.join("moon.mod.json")).unwrap(),
        expect![[r#"
            {
              "name": "hello",
              "deps": {}
            }"#]],
    );
    let _ = get_stdout_with_args(&dir, ["add", "lijunchen/hello2@0.1.0"]);
    std::fs::write(
        dir.join("main/main.mbt"),
        r#"fn main {
  println(@lib.hello2())
}
"#,
    )
    .unwrap();

    assert!(dir
        .join(DEP_PATH)
        .join("lijunchen")
        .join("hello")
        .join(MOON_MOD_JSON)
        .exists());

    std::fs::remove_dir_all(dir.join(DEP_PATH)).unwrap();
    let out = get_stdout_with_args(&dir, ["install"]);
    let mut lines = out.lines().collect::<Vec<_>>();
    lines.sort();
    check(
        &lines.join("\n"),
        expect![[r#"
            Using cached lijunchen/hello2@0.1.0
            Using cached lijunchen/hello@0.1.0"#]],
    );

    std::fs::write(
        dir.join("main/moon.pkg.json"),
        r#"{
          "is-main": true,
          "import": [
            "lijunchen/hello2/lib"
          ]
        }
    "#,
    )
    .unwrap();

    check(
        &get_stdout_with_args(&dir, ["run", "main"]),
        expect![[r#"
            Hello, world!Hello, world2!
        "#]],
    );
}

#[test]
#[ignore = "where to download mooncake?"]
fn mooncake_cli_smoke_test() {
    let dir = TestDir::new("hello.in");
    let out = std::process::Command::new(moon_bin())
        .env("RUST_BACKTRACE", "0")
        .current_dir(&dir)
        .args(["publish"])
        .output()
        .unwrap();
    let s = std::str::from_utf8(&out.stderr).unwrap().to_string();
    assert!(s.contains("failed to open credentials file"));
}

#[test]
fn bench2_test() {
    let dir = TestDir::new("bench2_test.in");
    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin("moon"))
        .current_dir(&dir)
        .args(["run", "main"])
        .assert()
        .success()
        .stdout_matches("ok[..]");
}

#[test]
fn cakenew_test() {
    let dir = TestDir::new("cakenew_test.in");
    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin("moon"))
        .current_dir(&dir)
        .args(["run", "main"])
        .assert()
        .success()
        .stdout_matches("Hello,[..]");
}

#[test]
fn capture_abort_test() {
    let dir = super::TestDir::new("capture_abort_test.in");
    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin("moon"))
        .current_dir(&dir)
        .args(["run", "main", "--nostd"])
        .assert()
        .failure();
}

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
    //         moonc check './main lib/hello.mbt' './main lib/hello_test.mbt' -o './target/check/main lib/main lib.underscore_test.mi' -pkg 'username/hello/main lib' -pkg-sources 'username/hello/main lib:./main lib'
    //         moonc check './main lib/hello.mbt' -o './target/check/main lib/main lib.mi' -pkg 'username/hello/main lib' -pkg-sources 'username/hello/main lib:./main lib'
    //         moonc check './main exe/main.mbt' -o './target/check/main exe/main exe.mi' -pkg 'username/hello/main exe' -is-main -i './target/check/main lib/main lib.mi:lib' -pkg-sources 'username/hello/main exe:./main exe'
    //     "#]],
    // );

    check(
        &get_stdout_with_args(&dir, ["build", "--dry-run", "--nostd"]),
        expect![[r#"
            moonc build-package "./main lib/hello.mbt" -o "./target/wasm-gc/release/build/main lib/main lib.core" -pkg "username/hello/main lib" -pkg-sources "username/hello/main lib:./main lib" -target wasm-gc
            moonc build-package "./main exe/main.mbt" -o "./target/wasm-gc/release/build/main exe/main exe.core" -pkg "username/hello/main exe" -is-main -i "./target/wasm-gc/release/build/main lib/main lib.mi:lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc
            moonc link-core "./target/wasm-gc/release/build/main lib/main lib.core" "./target/wasm-gc/release/build/main exe/main exe.core" -main "username/hello/main exe" -o "./target/wasm-gc/release/build/main exe/main exe.wasm" -pkg-sources "username/hello/main lib:./main lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc
        "#]],
    );

    check(
        &get_stdout_with_args(&dir, ["build", "--dry-run", "--debug", "--nostd"]),
        expect![[r#"
            moonc build-package "./main lib/hello.mbt" -o "./target/wasm-gc/debug/build/main lib/main lib.core" -pkg "username/hello/main lib" -pkg-sources "username/hello/main lib:./main lib" -target wasm-gc -g -source-map
            moonc build-package "./main exe/main.mbt" -o "./target/wasm-gc/debug/build/main exe/main exe.core" -pkg "username/hello/main exe" -is-main -i "./target/wasm-gc/debug/build/main lib/main lib.mi:lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc -g -source-map
            moonc link-core "./target/wasm-gc/debug/build/main lib/main lib.core" "./target/wasm-gc/debug/build/main exe/main exe.core" -main "username/hello/main exe" -o "./target/wasm-gc/debug/build/main exe/main exe.wasm" -pkg-sources "username/hello/main lib:./main lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc -g -source-map
        "#]],
    );

    check(
        &get_stdout_with_args(&dir, ["run", "main exe", "--dry-run", "--nostd"]),
        expect![[r#"
            moonc build-package "./main lib/hello.mbt" -o "./target/wasm-gc/release/build/main lib/main lib.core" -pkg "username/hello/main lib" -pkg-sources "username/hello/main lib:./main lib" -target wasm-gc
            moonc build-package "./main exe/main.mbt" -o "./target/wasm-gc/release/build/main exe/main exe.core" -pkg "username/hello/main exe" -is-main -i "./target/wasm-gc/release/build/main lib/main lib.mi:lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc
            moonc link-core "./target/wasm-gc/release/build/main lib/main lib.core" "./target/wasm-gc/release/build/main exe/main exe.core" -main "username/hello/main exe" -o "./target/wasm-gc/release/build/main exe/main exe.wasm" -pkg-sources "username/hello/main lib:./main lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc
            moonrun ./target/wasm-gc/release/build/main exe/main exe.wasm
        "#]],
    );

    check(
        &get_stdout_with_args(&dir, ["run", "main exe", "--dry-run", "--debug", "--nostd"]),
        expect![[r#"
            moonc build-package "./main lib/hello.mbt" -o "./target/wasm-gc/debug/build/main lib/main lib.core" -pkg "username/hello/main lib" -pkg-sources "username/hello/main lib:./main lib" -target wasm-gc -g -source-map
            moonc build-package "./main exe/main.mbt" -o "./target/wasm-gc/debug/build/main exe/main exe.core" -pkg "username/hello/main exe" -is-main -i "./target/wasm-gc/debug/build/main lib/main lib.mi:lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc -g -source-map
            moonc link-core "./target/wasm-gc/debug/build/main lib/main lib.core" "./target/wasm-gc/debug/build/main exe/main exe.core" -main "username/hello/main exe" -o "./target/wasm-gc/debug/build/main exe/main exe.wasm" -pkg-sources "username/hello/main lib:./main lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc -g -source-map
            moonrun ./target/wasm-gc/debug/build/main exe/main exe.wasm
        "#]],
    );
    check(
        &get_stdout_with_args(
            &dir,
            ["build", "--target", "wasm-gc", "--dry-run", "--nostd"],
        ),
        expect![[r#"
            moonc build-package "./main lib/hello.mbt" -o "./target/wasm-gc/release/build/main lib/main lib.core" -pkg "username/hello/main lib" -pkg-sources "username/hello/main lib:./main lib" -target wasm-gc
            moonc build-package "./main exe/main.mbt" -o "./target/wasm-gc/release/build/main exe/main exe.core" -pkg "username/hello/main exe" -is-main -i "./target/wasm-gc/release/build/main lib/main lib.mi:lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc
            moonc link-core "./target/wasm-gc/release/build/main lib/main lib.core" "./target/wasm-gc/release/build/main exe/main exe.core" -main "username/hello/main exe" -o "./target/wasm-gc/release/build/main exe/main exe.wasm" -pkg-sources "username/hello/main lib:./main lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc
        "#]],
    );
    check(
        &get_stdout_with_args(
            &dir,
            [
                "build",
                "--dry-run",
                "--target",
                "wasm-gc",
                "--debug",
                "--nostd",
            ],
        ),
        expect![[r#"
            moonc build-package "./main lib/hello.mbt" -o "./target/wasm-gc/debug/build/main lib/main lib.core" -pkg "username/hello/main lib" -pkg-sources "username/hello/main lib:./main lib" -target wasm-gc -g -source-map
            moonc build-package "./main exe/main.mbt" -o "./target/wasm-gc/debug/build/main exe/main exe.core" -pkg "username/hello/main exe" -is-main -i "./target/wasm-gc/debug/build/main lib/main lib.mi:lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc -g -source-map
            moonc link-core "./target/wasm-gc/debug/build/main lib/main lib.core" "./target/wasm-gc/debug/build/main exe/main exe.core" -main "username/hello/main exe" -o "./target/wasm-gc/debug/build/main exe/main exe.wasm" -pkg-sources "username/hello/main lib:./main lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc -g -source-map
        "#]],
    );

    check(
        &get_stdout_with_args(
            &dir,
            [
                "run",
                "main exe",
                "--target",
                "wasm-gc",
                "--dry-run",
                "--nostd",
            ],
        ),
        expect![[r#"
            moonc build-package "./main lib/hello.mbt" -o "./target/wasm-gc/release/build/main lib/main lib.core" -pkg "username/hello/main lib" -pkg-sources "username/hello/main lib:./main lib" -target wasm-gc
            moonc build-package "./main exe/main.mbt" -o "./target/wasm-gc/release/build/main exe/main exe.core" -pkg "username/hello/main exe" -is-main -i "./target/wasm-gc/release/build/main lib/main lib.mi:lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc
            moonc link-core "./target/wasm-gc/release/build/main lib/main lib.core" "./target/wasm-gc/release/build/main exe/main exe.core" -main "username/hello/main exe" -o "./target/wasm-gc/release/build/main exe/main exe.wasm" -pkg-sources "username/hello/main lib:./main lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc
            moonrun ./target/wasm-gc/release/build/main exe/main exe.wasm
        "#]],
    );

    check(
        &get_stdout_with_args(
            &dir,
            [
                "run",
                "main exe",
                "--target",
                "wasm-gc",
                "--dry-run",
                "--debug",
                "--nostd",
            ],
        ),
        expect![[r#"
            moonc build-package "./main lib/hello.mbt" -o "./target/wasm-gc/debug/build/main lib/main lib.core" -pkg "username/hello/main lib" -pkg-sources "username/hello/main lib:./main lib" -target wasm-gc -g -source-map
            moonc build-package "./main exe/main.mbt" -o "./target/wasm-gc/debug/build/main exe/main exe.core" -pkg "username/hello/main exe" -is-main -i "./target/wasm-gc/debug/build/main lib/main lib.mi:lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc -g -source-map
            moonc link-core "./target/wasm-gc/debug/build/main lib/main lib.core" "./target/wasm-gc/debug/build/main exe/main exe.core" -main "username/hello/main exe" -o "./target/wasm-gc/debug/build/main exe/main exe.wasm" -pkg-sources "username/hello/main lib:./main lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc -g -source-map
            moonrun ./target/wasm-gc/debug/build/main exe/main exe.wasm
        "#]],
    );

    check(
        &get_stdout_with_args(&dir, ["run", "main exe"]),
        expect![[r#"
            Hello, world!
        "#]],
    );

    let out = get_stdout_with_args(&dir, ["check"]);
    assert!(out.contains("moon: ran 3 tasks, now up to date"));
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

    let out = get_stdout_with_args(&path_with_space, ["build", "--no-render"]);
    let out = out.replace(&prefix, ".");
    let out = out.replace(
        &moonutil::moon_dir::home()
            .to_str()
            .unwrap()
            .replace('\\', "/"),
        "$MOON_HOME",
    );

    copy(&dir, &path_with_space)?;
    check(
        &out,
        expect![[r#"
            moonc build-package "./with space/main lib/hello.mbt" -o "./with space/target/wasm-gc/release/build/main lib/main lib.core" -pkg "username/hello/main lib" -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources "username/hello/main lib:./with space/main lib" -target wasm-gc
            moonc build-package "./with space/main exe/main.mbt" -o "./with space/target/wasm-gc/release/build/main exe/main exe.core" -pkg "username/hello/main exe" -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i "./with space/target/wasm-gc/release/build/main lib/main lib.mi:lib" -pkg-sources "username/hello/main exe:./with space/main exe" -target wasm-gc
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core "./with space/target/wasm-gc/release/build/main lib/main lib.core" "./with space/target/wasm-gc/release/build/main exe/main exe.core" -main "username/hello/main exe" -o "./with space/target/wasm-gc/release/build/main exe/main exe.wasm" -pkg-sources "username/hello/main lib:./with space/main lib" -pkg-sources "username/hello/main exe:./with space/main exe" -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc
            moon: ran 3 tasks, now up to date
        "#]],
    );
    Ok(())
}

#[test]
fn circle_pkg_test() {
    let dir = TestDir::new("circle_pkg_AB_001_test.in");
    let stderr = get_stderr_with_args(&dir, ["run", "main", "--nostd"]);
    assert!(stderr.contains("cyclic dependency"), "stderr: {}", stderr);
}

#[test]
fn debug_flag_test() {
    let dir = TestDir::new("debug_flag_test.in");
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["clean"])
        .assert()
        .success();

    check(
        &get_stdout_with_args(&dir, ["check", "--dry-run", "--nostd"]),
        expect![[r#"
            moonc check ./lib/hello.mbt -o ./target/wasm-gc/release/check/lib/lib.mi -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc
            moonc check ./main/main.mbt -o ./target/wasm-gc/release/check/main/main.mi -pkg hello/main -is-main -i ./target/wasm-gc/release/check/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc
        "#]],
    );

    check(
        &get_stdout_with_args(&dir, ["build", "--dry-run", "--nostd"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc
        "#]],
    );

    check(
        &get_stdout_with_args(&dir, ["build", "--dry-run", "--debug", "--nostd"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -g -source-map
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -g -source-map
            moonc link-core ./target/wasm-gc/debug/build/lib/lib.core ./target/wasm-gc/debug/build/main/main.core -main hello/main -o ./target/wasm-gc/debug/build/main/main.wasm -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc -g -source-map
        "#]],
    );

    check(
        &get_stdout_with_args(&dir, ["run", "main", "--dry-run", "--nostd"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc
            moonrun ./target/wasm-gc/release/build/main/main.wasm
        "#]],
    );

    check(
        &get_stdout_with_args(&dir, ["run", "main", "--dry-run", "--debug", "--nostd"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -g -source-map
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -g -source-map
            moonc link-core ./target/wasm-gc/debug/build/lib/lib.core ./target/wasm-gc/debug/build/main/main.core -main hello/main -o ./target/wasm-gc/debug/build/main/main.wasm -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc -g -source-map
            moonrun ./target/wasm-gc/debug/build/main/main.wasm
        "#]],
    );
    check(
        &get_stdout_with_args(
            &dir,
            ["build", "--target", "wasm-gc", "--dry-run", "--nostd"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc
        "#]],
    );
    check(
        &get_stdout_with_args(
            &dir,
            [
                "build",
                "--dry-run",
                "--target",
                "wasm-gc",
                "--debug",
                "--nostd",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -g -source-map
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -g -source-map
            moonc link-core ./target/wasm-gc/debug/build/lib/lib.core ./target/wasm-gc/debug/build/main/main.core -main hello/main -o ./target/wasm-gc/debug/build/main/main.wasm -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc -g -source-map
        "#]],
    );

    check(
        &get_stdout_with_args(
            &dir,
            ["run", "main", "--target", "wasm-gc", "--dry-run", "--nostd"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc
            moonrun ./target/wasm-gc/release/build/main/main.wasm
        "#]],
    );

    check(
        &get_stdout_with_args(
            &dir,
            [
                "run",
                "main",
                "--target",
                "wasm-gc",
                "--dry-run",
                "--debug",
                "--nostd",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -g -source-map
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -g -source-map
            moonc link-core ./target/wasm-gc/debug/build/lib/lib.core ./target/wasm-gc/debug/build/main/main.core -main hello/main -o ./target/wasm-gc/debug/build/main/main.wasm -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc -g -source-map
            moonrun ./target/wasm-gc/debug/build/main/main.wasm
        "#]],
    );
}

#[test]
fn test_check_failed_should_write_pkg_json() {
    let dir = TestDir::new("check_failed_should_write_pkg_json.in");
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["check"])
        .assert()
        .failure();

    let pkg_json = dir.join("target/wasm-gc/release/check/packages.json");
    assert!(pkg_json.exists());
}

#[test]
fn test_render_no_location() {
    std::env::set_var("NO_COLOR", "1");
    let dir = TestDir::new("render_no_location.in");

    let output = snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .arg("check")
        .assert()
        .failure()
        .get_output()
        .stdout
        .to_owned();

    let output = String::from_utf8_lossy(&output);
    assert!(output.contains("[4067] Error: Missing main function in the main package."));
}

#[test]
fn test_moon_run_with_cli_args() {
    let dir = TestDir::new("moo_run_with_cli_args.in");

    check(
        &get_stdout_with_args_and_replace_dir(&dir, ["run", "main", "--dry-run"]),
        expect![[r#"
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/main:./main -target wasm-gc
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/main/main.core -main username/hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-sources username/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc
            moonrun ./target/wasm-gc/release/build/main/main.wasm
        "#]],
    );

    check(
        &get_stdout_with_args_and_replace_dir(
            &dir,
            [
                "run",
                "main",
                "--dry-run",
                "--",
                "中文",
                "😄👍",
                "hello",
                "1242",
            ],
        ),
        expect![[r#"
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/main:./main -target wasm-gc
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/main/main.core -main username/hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-sources username/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc
            moonrun ./target/wasm-gc/release/build/main/main.wasm -- 中文 😄👍 hello 1242
        "#]],
    );

    check(
        &get_stdout_with_args_and_replace_dir(
            &dir,
            ["run", "main", "--", "中文", "😄👍", "hello", "1242"],
        ),
        expect![[r#"
            [中文, 😄👍, hello, 1242]
        "#]],
    );
}

#[test]
fn test_third_party() {
    let dir = TestDir::new("third_party.in");
    get_stdout_with_args_and_replace_dir(&dir, ["update"]);
    get_stdout_with_args_and_replace_dir(&dir, ["build"]);
    get_stdout_with_args_and_replace_dir(&dir, ["clean"]);

    let actual = &get_stdout_with_args_and_replace_dir(&dir, ["check"]);
    assert!(actual.contains("moon: ran 4 tasks, now up to date"));

    check(
        &get_stdout_with_args_and_replace_dir(&dir, ["test", "--dry-run", "--sort-input"]),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target/wasm-gc/debug/test --sort-input
            moonc build-package ./.mooncakes/lijunchen/hello18/lib/hello.mbt -o ./target/wasm-gc/debug/test/.mooncakes/lijunchen/hello18/lib/lib.core -pkg lijunchen/hello18/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources lijunchen/hello18/lib:./.mooncakes/lijunchen/hello18/lib -target wasm-gc -g
            moonc build-package ./lib/test.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/.mooncakes/lijunchen/hello18/lib/lib.mi:lib -pkg-sources username/hello/lib:./lib -target wasm-gc -g
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/.mooncakes/lijunchen/hello18/lib/lib.core ./target/wasm-gc/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-sources lijunchen/hello18/lib:./lib -pkg-sources username/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc -g
        "#]],
    );

    check(
        &get_stdout_with_args_and_replace_dir(&dir, ["test", "--sort-input"]),
        expect![[r#"
            Hello, world!
            Hello, world!
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );

    let actual = &get_stdout_with_args_and_replace_dir(&dir, ["build"]);
    assert!(actual.contains("moon: ran 5 tasks, now up to date"));

    let actual = &get_stdout_with_args_and_replace_dir(&dir, ["run", "main"]);
    assert!(actual.contains("Hello, world!"));
}

#[test]
fn test_blackbox_success() {
    let dir = TestDir::new("blackbox_success_test.in");

    check(
        &get_stdout_with_args_and_replace_dir(
            &dir,
            [
                "test",
                "-p",
                "username/hello/A",
                "-f",
                "hello_bbtest.mbt",
                "-i",
                "0",
                "--dry-run",
                "--nostd",
            ],
        ),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target/wasm-gc/debug/test --package username/hello/A --file hello_bbtest.mbt --index 0
            moonc build-package ./A/hello.mbt -o ./target/wasm-gc/debug/test/A/A.core -pkg username/hello/A -pkg-sources username/hello/A:./A -target wasm-gc -g
            moonc build-package ./C/hello.mbt -o ./target/wasm-gc/debug/test/C/C.core -pkg username/hello/C -pkg-sources username/hello/C:./C -target wasm-gc -g
            moonc build-package ./A/hello_bbtest.mbt ./target/wasm-gc/debug/test/A/__generated_driver_for_blackbox_test.mbt -o ./target/wasm-gc/debug/test/A/A.blackbox_test.core -pkg username/hello/A_blackbox_test -is-main -i ./target/wasm-gc/debug/test/A/A.mi:A -i ./target/wasm-gc/debug/test/C/C.mi:C -pkg-sources username/hello/A_blackbox_test:./A -target wasm-gc -g
            moonc link-core ./target/wasm-gc/debug/test/C/C.core ./target/wasm-gc/debug/test/A/A.core ./target/wasm-gc/debug/test/A/A.blackbox_test.core -main username/hello/A -o ./target/wasm-gc/debug/test/A/A.blackbox_test.wasm -test-mode -pkg-sources username/hello/C:./C -pkg-sources username/hello/A:./A -target wasm-gc -g
            moonc build-package ./B/hello.mbt -o ./target/wasm-gc/debug/test/B/B.core -pkg username/hello/B -pkg-sources username/hello/B:./B -target wasm-gc -g
            moonc build-package ./A/hello.mbt ./A/hello_test.mbt ./target/wasm-gc/debug/test/A/__generated_driver_for_underscore_test.mbt -o ./target/wasm-gc/debug/test/A/A.underscore_test.core -pkg username/hello/A -is-main -i ./target/wasm-gc/debug/test/B/B.mi:B -pkg-sources username/hello/A:./A -target wasm-gc -g
            moonc link-core ./target/wasm-gc/debug/test/B/B.core ./target/wasm-gc/debug/test/A/A.underscore_test.core -main username/hello/A -o ./target/wasm-gc/debug/test/A/A.underscore_test.wasm -test-mode -pkg-sources username/hello/B:./B -pkg-sources username/hello/A:./A -target wasm-gc -g
            moonc build-package ./A/hello.mbt ./target/wasm-gc/debug/test/A/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/A/A.internal_test.core -pkg username/hello/A -is-main -pkg-sources username/hello/A:./A -target wasm-gc -g
            moonc link-core ./target/wasm-gc/debug/test/A/A.internal_test.core -main username/hello/A -o ./target/wasm-gc/debug/test/A/A.internal_test.wasm -test-mode -pkg-sources username/hello/A:./A -target wasm-gc -g
        "#]],
    );

    check(
        &get_stdout_with_args_and_replace_dir(&dir, ["test"]),
        expect![[r#"
            output from A/hello.mbt!
            output from C/hello.mbt!
            self.a: 33
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );
}

#[test]
fn test_blackbox_failed() {
    let dir = TestDir::new("blackbox_failed_test.in");

    let output = snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .arg("test")
        .assert()
        .failure()
        .get_output()
        .stdout
        .to_owned();

    let output = String::from_utf8_lossy(&output);
    // bbtest can not use private function in bbtest_import
    assert!(output.contains("Value _private_hello not found in package \"A\""));
    // bbtest_import could no be used in _test.mbt
    assert!(output.contains("Package \"C\" not found in the loaded packages."));
}
