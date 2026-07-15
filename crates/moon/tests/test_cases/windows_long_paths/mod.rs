use super::*;
use std::{
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
};

const LEGACY_PATH_LIMIT: usize = 260;

fn windows_path_len(path: &Path) -> usize {
    path.as_os_str().encode_wide().count()
}

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

#[test]
fn toolchain_commands_handle_artifacts_beyond_the_legacy_path_limit() {
    let dir = TestDir::new_empty();
    let root = dir.as_ref();
    assert!(
        windows_path_len(&root.join("_build")) < LEGACY_PATH_LIMIT,
        "the test needs a shallow target root"
    );

    let mut package_rel = PathBuf::new();
    // Keep the inputs below the legacy limit while making the package deep
    // enough for the `_build` artifact to cross it.
    for index in 0.. {
        let next = package_rel.join(format!("segment{index:02}"));
        if windows_path_len(&root.join(&next).join("moon.pkg.json")) >= LEGACY_PATH_LIMIT {
            break;
        }
        package_rel = next;
    }

    let package_dir = root.join(&package_rel);
    let package_manifest = package_dir.join("moon.pkg.json");
    let source_file = package_dir.join("lib.mbt");
    assert!(
        windows_path_len(&package_manifest) < LEGACY_PATH_LIMIT,
        "the package manifest must remain addressable as a legacy path"
    );
    assert!(
        windows_path_len(&source_file) < LEGACY_PATH_LIMIT,
        "the source file must remain addressable as a legacy path"
    );

    write_file(
        &root.join("moon.mod.json"),
        r#"{
  "name": "test/long-path"
}
"#,
    );
    write_file(
        &package_manifest,
        r#"{
  "is-main": true
}
"#,
    );
    write_file(
        &source_file,
        r#"pub fn answer() -> Int { 42 }

fn main {
  println(answer())
}

test "answer" {
  inspect(answer(), content="42")
}
"#,
    );

    let dry_run = moon_cmd(&dir)
        .args(["check", "--dry-run", "--sort-input"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let dry_run = String::from_utf8(dry_run).expect("dry-run output should be UTF-8");
    let command_args = dry_run
        .lines()
        .find_map(|line| {
            let args = moonutil::shlex::split_native(line);
            args.windows(2).any(|pair| pair[0] == "-o").then_some(args)
        })
        .expect("dry run should expose the compiler command");
    let output_path = command_args
        .windows(2)
        .find(|pair| pair[0] == "-o")
        .map(|pair| PathBuf::from(&pair[1]))
        .expect("dry run should expose the package artifact after `-o`");
    let output_path = if output_path.is_absolute() {
        output_path
    } else {
        root.join(output_path)
    };
    assert!(
        windows_path_len(&output_path) > LEGACY_PATH_LIMIT,
        "artifact path should exceed the legacy limit: {}",
        output_path.display()
    );

    for command in ["fmt", "check", "build", "test", "bundle", "info"] {
        moon_cmd(&dir).arg("clean").assert().success();
        moon_cmd(&dir).arg(command).assert().success();
    }

    moon_cmd(&dir).arg("clean").assert().success();
    // TODO: `moondoc` receives a short output root, then appends enough package
    // components internally to create an invalid long legacy path.
    moon_cmd(&dir)
        .arg("doc")
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
Fatal error: exception Sys_error("[..]: No such file or directory")
...
"#]]);

    moon_cmd(&dir).arg("clean").assert().success();
    moon_cmd(&dir)
        .arg("run")
        .arg(&package_rel)
        .assert()
        .success();

    moon_cmd(&dir).arg("clean").assert().success();
    moon_cmd(&dir)
        .arg("run")
        .arg(&package_rel)
        .args(["--target", "js"])
        .assert()
        .failure()
        // TODO: Remove this expected failure after Node.js accepts valid
        // extended-path entry points again: https://github.com/nodejs/node/issues/62446
        .stderr_eq(snapbox::str![[r#"
node:fs:[..]
    const out = binding.lstat(base, false, undefined, true /* throwIfNoEntry */);
                        ^

Error: EISDIR: illegal operation on a directory, lstat 'C:'
...
"#]]);

    for command in ["check", "info"] {
        moon_cmd(&dir).arg("clean").assert().success();
        moon_cmd(&dir)
            .args([command, "--target", "native"])
            .assert()
            .success();
    }

    // TODO: MSVC cannot open long paths in either legacy or verbatim form. Keep
    // the real commands covered so that limitation becomes visible if it changes.
    for command in ["build", "test"] {
        moon_cmd(&dir).arg("clean").assert().success();
        moon_cmd(&dir)
            .args([command, "--target", "native", "--dry-run"])
            .assert()
            .success();
        moon_cmd(&dir)
            .args([command, "--target", "native"])
            .assert()
            .failure();
    }
}
