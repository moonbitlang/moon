use expect_test::expect_file;

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
            if rel.eq_ignore_ascii_case("LICENSE") {
                // Skip LICENSE file content
                file_items.push((rel_file, "<LICENSE file content>\n".to_string()));
                continue;
            } else if rel.eq_ignore_ascii_case("Agents.md") {
                // Skip Agents.md file content
                file_items.push((rel_file, "<AGENTS.md file content>\n".to_string()));
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
        ["new", "hello", "--user", "moonbitlang", "--name", "hello"],
    );
    check(
        get_stdout(
            &dir,
            [
                "run",
                "--manifest-path",
                "./hello/moon.mod.json",
                "--target-dir",
                "./hello/target",
                "cmd/main",
            ],
        ),
        expect![[r#"
            Hello
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "-C",
                "./hello",
                "run",
                "--target-dir",
                "./target",
                "cmd/main",
            ],
        ),
        expect![[r#"
            Hello
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "-C",
                "./hello",
                "run",
                "--target-dir",
                "./target",
                "cmd/main",
            ],
        ),
        expect![[r#"
            Hello
        "#]],
    );
}

#[test]
fn test_moon_new_exist() {
    let dir = TestDir::new("moon_new/exist");
    dir.join("hello").rm_rf();
    let res = &get_stdout(
        &dir,
        ["new", "hello", "--user", "moonbitlang", "--name", "hello"],
    );

    assert!(res.contains("Created moonbitlang/hello at hello"));
    assert!(res.contains("Initialized empty Git repository"));

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(dir.join("hello"))
        .args(["new", ".", "--user", "moonbitlang", "--name", "hello"])
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
        .args(["new", "hello", "--user", "moonbitlang", "--name", "hello"])
        .assert()
        .success();
    check(
        get_stdout(&hello1, ["run", "cmd/main"]),
        expect![[r#"
            Hello
        "#]],
    );
    hello1.rm_rf();

    let hello2 = dir.join("hello2");
    std::fs::create_dir_all(&hello2).unwrap();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&hello2)
        .args(["new", ".", "--user", "moonbitlang", "--name", "hello"])
        .assert()
        .success();
    check(
        get_stdout(&hello2, ["run", "cmd/main"]),
        expect![[r#"
            Hello
        "#]],
    );
    hello2.rm_rf();

    let hello3 = dir.join("hello3");
    if hello3.exists() {
        hello3.rm_rf();
    }
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["new", "hello3", "--user", "moonbitlang", "--name", "hello"])
        .assert()
        .success();
    check(
        get_stdout(&hello3, ["test", "-v"]),
        expect![[r#"
            Total tests: 0, passed: 0, failed: 0.
        "#]],
    );
    check(
        get_stdout(&hello3, ["test"]),
        expect![[r#"
            Total tests: 0, passed: 0, failed: 0.
        "#]],
    );
    hello3.rm_rf();

    let hello4 = dir.join("hello4");
    std::fs::create_dir_all(&hello4).unwrap();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&hello4)
        .args(["new", ".", "--user", "moonbitlang", "--name", "hello"])
        .assert()
        .success();
    check(
        std::fs::read_to_string(hello4.join("moon.pkg")).unwrap(),
        expect![[""]],
    );
    check(
        get_stdout(&hello4, ["test", "-v"]),
        expect![[r#"
            Total tests: 0, passed: 0, failed: 0.
        "#]],
    );
    hello4.rm_rf();
}

#[test]
fn test_moon_new_snapshot() {
    let dir = TestDir::new("moon_new/snapshot");

    let asdf = dir.join("asdf");
    if asdf.exists() {
        asdf.rm_rf();
    }

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["new", "asdf"])
        .env("MOON_HOME", dir.as_ref())
        .assert()
        .success();

    // New snapshot: layout first, then file contents
    let snap = snapshot_layout_and_files(&asdf);
    expect_file!["new_snapshot.expect"].assert_eq(&snap);

    if asdf.exists() {
        asdf.rm_rf();
    }

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["new", "asdf", "--user", "moonbitlang", "--name", "hello"])
        .assert()
        .success();

    let snap2 = snapshot_layout_and_files(&asdf);
    expect_file!["new_snapshot_with_user_name.expect"].assert_eq(&snap2);
    asdf.rm_rf();

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["new", "asdf", "--user", "moonbitlang", "--name", "wow"])
        .assert()
        .success();

    let snap3 = snapshot_layout_and_files(&asdf);
    expect_file!["new_snapshot_with_user_name_different.expect"].assert_eq(&snap3);
    asdf.rm_rf();
}
