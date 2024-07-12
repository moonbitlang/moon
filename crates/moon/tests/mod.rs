mod test_cases;

use expect_test::Expect;
use std::path::{Path, PathBuf};

fn check(actual: &str, expect: Expect) {
    expect.assert_eq(actual)
}

struct TestDir {
    // tempfile::TempDir has a drop implementation that will remove the directory
    // copy the test directory to a temporary directory to abvoid conflict with other tests when `cargo test` parallelly testing
    path: tempfile::TempDir,
}

impl TestDir {
    // create a new TestDir with the test directory in tests/test_cases/<sub>
    fn new(sub: &str) -> Self {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/test_cases")
            .join(sub);
        let tmp_dir = tempfile::TempDir::new().unwrap();
        copy(&dir, tmp_dir.path()).unwrap();
        Self { path: tmp_dir }
    }

    // create a empty TestDir
    fn new_empty() -> Self {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        Self { path: tmp_dir }
    }

    fn join(&self, sub: impl AsRef<str>) -> PathBuf {
        self.path.path().join(sub.as_ref())
    }
}

impl AsRef<Path> for TestDir {
    fn as_ref(&self) -> &Path {
        self.path.path()
    }
}

pub fn moon_bin() -> PathBuf {
    snapbox::cmd::cargo_bin("moon")
}

pub fn get_stdout_with_args_without_replace(
    dir: &impl AsRef<std::path::Path>,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
) -> String {
    let out = snapbox::cmd::Command::new(moon_bin())
        .current_dir(dir)
        .args(args)
        .assert()
        .success()
        .get_output()
        .stdout
        .to_owned();

    let s = std::str::from_utf8(&out).unwrap().to_string();
    s
}

pub fn get_stderr_with_args_without_replace(
    dir: &impl AsRef<std::path::Path>,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
) -> String {
    let out = snapbox::cmd::Command::new(moon_bin())
        .current_dir(dir)
        .args(args)
        .assert()
        .failure()
        .get_output()
        .stderr
        .to_owned();

    let s = std::str::from_utf8(&out).unwrap().to_string();
    s
}

pub fn get_stdout_with_args(
    dir: &impl AsRef<std::path::Path>,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
) -> String {
    let s = get_stdout_with_args_without_replace(dir, args);
    let s = s.replace("\r\n", "\n");

    s.replace('\\', "/")
}

pub fn replace_dir(s: &str, dir: &impl AsRef<std::path::Path>) -> String {
    let path_str1 = dunce::canonicalize(dir)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let s = s.replace("\\\\", "\\");
    let s = s.replace(&path_str1, "$ROOT");
    let s = s.replace(
        dunce::canonicalize(moonutil::moon_dir::home())
            .unwrap()
            .to_str()
            .unwrap(),
        "$MOON_HOME",
    );
    let s = s.replace(moon_bin().to_string_lossy().as_ref(), "moon");
    s.replace("\r\n", "\n").replace('\\', "/")
}

pub fn get_stdout_with_args_and_replace_dir(
    dir: &impl AsRef<std::path::Path>,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
) -> String {
    let s = get_stdout_with_args_without_replace(dir, args);
    replace_dir(&s, dir)
}

pub fn get_stderr_with_args_and_replace_dir(
    dir: &impl AsRef<std::path::Path>,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
) -> String {
    let s = get_stderr_with_args_without_replace(dir, args);
    replace_dir(&s, dir)
}

pub fn get_stderr_with_args(
    dir: &impl AsRef<std::path::Path>,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
) -> String {
    let out = snapbox::cmd::Command::new(moon_bin())
        .current_dir(dir)
        .args(args)
        .assert()
        .failure()
        .get_output()
        .stderr
        .to_owned();

    let s = std::str::from_utf8(&out).unwrap().to_string();
    let s = s.replace("\r\n", "\n");

    s.replace('\\', "/")
}

pub fn copy(src: &Path, dest: &Path) -> anyhow::Result<()> {
    if src.is_dir() {
        if !dest.exists() {
            std::fs::create_dir_all(dest)?;
        }
        for entry in walkdir::WalkDir::new(src) {
            let entry = entry?;
            let path = entry.path();
            let relative_path = path.strip_prefix(src)?;
            let dest_path = dest.join(relative_path);
            if path.is_dir() {
                if !dest_path.exists() {
                    std::fs::create_dir_all(dest_path)?;
                }
            } else {
                std::fs::copy(path, dest_path)?;
            }
        }
    } else {
        std::fs::copy(src, dest)?;
    }
    Ok(())
}

pub fn replace_crlf_to_lf(s: &str) -> String {
    s.replace("\r\n", "\n")
}
