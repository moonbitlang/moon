use super::*;
use std::fs;

fn write_markdown(dir: &TestDir, config: &str, code: &str) {
    fs::write(
        dir.join("readme.mbt.md"),
        format!("---\nmoonbit:\n{config}\n---\n```mbt check\n{code}\n```\n"),
    )
    .unwrap();
}

#[test]
fn test_standalone_markdown_check_and_test_without_main() {
    let dir = TestDir::new_empty();
    for imports in ["[]", "{}"] {
        write_markdown(
            &dir,
            &format!("  import: {imports}"),
            "test { assert_eq(1, 1) }",
        );
        moon_cmd(&dir)
            .args(["check", "readme.mbt.md"])
            .assert()
            .success();
        moon_cmd(&dir)
            .args(["test", "readme.mbt.md"])
            .assert()
            .success()
            .stdout_eq("Total tests: 1, passed: 1, failed: 0.\n");
    }
    assert!(dir.join("_build/readme.mbt.md/.moon_db").is_file());
    assert!(!dir.join("_build/.moon_db").exists());
    assert!(!dir.join(".mooncakes").exists());
}

#[test]
fn test_standalone_markdown_ignores_surrounding_project() {
    let dir = TestDir::new_empty();
    fs::write(dir.join("moon.mod"), "invalid module manifest").unwrap();
    fs::write(dir.join("moon.pkg"), "import { \"unavailable/package\" }").unwrap();
    fs::write(dir.join("broken.mbt"), "invalid MoonBit source").unwrap();
    write_markdown(&dir, "  import: []", "test { assert_eq(1, 1) }");
    moon_cmd(&dir)
        .args(["check", "readme.mbt.md"])
        .assert()
        .success();
    moon_cmd(&dir)
        .args(["test", "readme.mbt.md"])
        .assert()
        .success()
        .stdout_eq("Total tests: 1, passed: 1, failed: 0.\n");

    for command in ["check", "test"] {
        moon_cmd(&dir)
            .args([command, "readme.mbt.md", "."])
            .assert()
            .failure()
            .stderr_eq(format!(
                "Error: standalone `.mbt.md` `moon {command}` expects exactly one `PATH`\n"
            ));
    }
}

#[test]
fn test_standalone_markdown_aliases_and_implicit_core_imports() {
    let dir = TestDir::new_empty();
    for imports in [
        "[{path: moonbitlang/core/int, alias: integer}]",
        "{moonbitlang/core/int: integer}",
    ] {
        write_markdown(
            &dir,
            &format!("  import: {imports}"),
            "test { assert_eq(@integer.MAX_VALUE, 2147483647) }",
        );
        moon_cmd(&dir)
            .args(["test", "readme.mbt.md"])
            .assert()
            .success()
            .stdout_eq("Total tests: 1, passed: 1, failed: 0.\n");
    }
    write_markdown(
        &dir,
        "  import: []",
        "test { assert_true(@env.args().length() >= 0) }",
    );
    moon_cmd(&dir)
        .args(["check", "readme.mbt.md", "--warn-list=-a"])
        .assert()
        .success();
    moon_cmd(&dir)
        .args(["test", "readme.mbt.md", "--warn-list=-a"])
        .assert()
        .success()
        .stdout_eq("Total tests: 1, passed: 1, failed: 0.\n");
}

#[test]
fn test_standalone_markdown_backend_selection() {
    let dir = TestDir::new_empty();
    for (yaml_backend, flags, backends) in [
        ("", vec![], vec!["wasm"]),
        ("  backend: js\n", vec![], vec!["js"]),
        (
            "  backend: js\n",
            vec!["--target", "wasm-gc"],
            vec!["wasm-gc"],
        ),
        (
            "  backend: native\n",
            vec!["--target", "wasm,js"],
            vec!["wasm", "js"],
        ),
    ] {
        write_markdown(
            &dir,
            &format!("{yaml_backend}  import: []"),
            "test { assert_eq(1, 1) }",
        );
        moon_cmd(&dir)
            .args(["check", "readme.mbt.md"])
            .args(&flags)
            .assert()
            .success();
        let target_dir = standalone_target_dir(&dir, "readme.mbt.md");
        let index: Vec<String> =
            serde_json::from_str(&read(target_packages_index_path(&target_dir))).unwrap();
        assert_eq!(index, backends);
        moon_cmd(&dir)
            .args(["test", "readme.mbt.md"])
            .args(&flags)
            .assert()
            .success()
            .stdout_eq(if backends.len() == 1 {
                "Total tests: 1, passed: 1, failed: 0.\n".to_owned()
            } else {
                "Total tests: 1, passed: 1, failed: 0. [wasm]\nTotal tests: 1, passed: 1, failed: 0. [js]\n".to_owned()
            });
    }
    moon_cmd(&dir)
        .args(["test", "readme.mbt.md", "--target", "all"])
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
Total tests: 1, passed: 1, failed: 0. [wasm]
Total tests: 1, passed: 1, failed: 0. [wasm-gc]
Total tests: 1, passed: 1, failed: 0. [js]
Total tests: 1, passed: 1, failed: 0. [native]

"#]]);
}

#[test]
fn test_standalone_markdown_rejects_build_and_run() {
    let dir = TestDir::new_empty();
    write_markdown(&dir, "  import: []", "test { assert_eq(1, 1) }");
    for in_project in [false, true] {
        if in_project {
            fs::write(dir.join("moon.mod"), "name = \"test/project\"\n").unwrap();
            fs::write(dir.join("moon.pkg"), "pkgtype(kind: \"executable\")\n").unwrap();
            fs::write(dir.join("main.mbt"), "fn main {}\n").unwrap();
        }
        for command in ["build", "run"] {
            moon_cmd(&dir)
                .args([command, "readme.mbt.md"])
                .assert()
                .failure()
                .stdout_eq("")
                .stderr_eq(format!("Error: standalone Markdown `readme.mbt.md` only supports `moon check` and `moon test`; use one of these commands instead of `moon {command}`\n"));
        }
    }
    assert!(!dir.join("_build").exists());
}

#[test]
fn test_markdown_without_imports_requires_project() {
    let dir = TestDir::new_empty();
    for config in ["  backend: js", "  deps: {}"] {
        write_markdown(&dir, config, "test { assert_eq(1, 1) }");
        for command in ["check", "test"] {
            moon_cmd(&dir)
                .env(MOON_WORK_ENV, "auto")
                .args([command, "readme.mbt.md"])
                .assert()
                .failure()
                .stderr_eq(snapbox::str![[r#"
Error: not in a Moon project (no moon.mod, moon.mod.json, or moon.work found starting from [..] or its ancestors)

"#]]);
        }
    }
    assert!(!dir.join("_build").exists());
}

#[test]
fn test_standalone_markdown_invalid_config_reports_path_without_project_fallback() {
    let dir = TestDir::new_empty();
    fs::write(dir.join("moon.mod"), "invalid module manifest").unwrap();
    for config in [
        "  import: invalid",
        "  import: []\n  backend: invalid",
        "  backend: invalid",
        "  deps: {}\n  import: []",
        "  import: [",
    ] {
        write_markdown(&dir, config, "test {}");
        for command in ["check", "test", "build", "run"] {
            moon_cmd(&dir)
                .args([command, "readme.mbt.md", "--target", "wasm"])
                .assert()
                .failure()
                .stdout_eq("")
                .stderr_eq(snapbox::str![[r#"
Error: invalid Markdown front matter in `readme.mbt.md`

Caused by:
...

"#]]);
        }
    }
    write_markdown(&dir, "  import: ['example/lib/pkg']", "test {}");
    moon_cmd(&dir)
        .args(["check", "readme.mbt.md"])
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
Error: invalid moonbit.import in `readme.mbt.md`

Caused by:
    module 'example/lib' must include a version in moonbit.import (e.g. example/lib@0.4.40[/package])

"#]]);
    assert!(!dir.join("_build").exists());
}

#[test]
fn test_markdown_suffix_directory_selects_package() {
    let dir = TestDir::new_empty();
    fs::write(dir.join("moon.mod"), "name = \"test/project\"\n").unwrap();
    fs::create_dir(dir.join("app.mbt.md")).unwrap();
    fs::write(dir.join("app.mbt.md/moon.pkg"), "\n").unwrap();
    fs::write(
        dir.join("app.mbt.md/test.mbt"),
        "test { assert_eq(1, 1) }\n",
    )
    .unwrap();
    for command in ["check", "test", "fmt"] {
        moon_cmd(&dir)
            .args([command, "app.mbt.md"])
            .assert()
            .success();
    }
    assert!(!dir.join("app.mbt.md/_build").exists());
}

#[cfg(unix)]
#[test]
fn test_standalone_markdown_invoked_path_and_symlink() {
    let dir = TestDir::new_empty();
    write_markdown(&dir, "  import: []", "test { assert_eq(1, 1) }");
    fs::rename(dir.join("readme.mbt.md"), dir.join("original.data")).unwrap();
    std::os::unix::fs::symlink("original.data", dir.join("alias.mbt.md")).unwrap();
    moon_cmd(&dir)
        .args(["check", "./alias.mbt.md"])
        .assert()
        .success();
    let caller = TestDir::new_empty();
    moon_cmd(&caller)
        .arg("test")
        .arg(dir.join("alias.mbt.md"))
        .assert()
        .success()
        .stdout_eq("Total tests: 1, passed: 1, failed: 0.\n");
    assert!(dir.join("_build/original.data/.moon_db").is_file());
}
