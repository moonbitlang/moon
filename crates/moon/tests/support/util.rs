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

use std::path::{Path, PathBuf};

use expect_test::Expect;
use moonutil::common::StringExt;

pub(crate) fn check<S: AsRef<str>>(actual: S, expect: Expect) {
    expect.assert_eq(actual.as_ref())
}

pub(crate) fn moon_bin() -> PathBuf {
    snapbox::cargo_bin!("moon").to_owned()
}

pub(crate) fn replace_dir(s: &str, dir: impl AsRef<std::path::Path>) -> String {
    moon_test_util::redact::common_output_redactor(dir.as_ref()).redact(s)
}

pub(crate) fn copy(src: &Path, dest: &Path) -> anyhow::Result<()> {
    moon_test_util::test_dir::copy_tree(src, dest, true)
}

#[track_caller]
pub(crate) fn read<P: AsRef<Path>>(p: P) -> String {
    std::fs::read_to_string(p).unwrap().replace_crlf_to_lf()
}

/// Asserts the `shlex`'d result of the given string is equal to the expected
/// string. However, still updates if `UPDATE_EXPECT` is set, just like the
/// original [`Expect`] functionality.
pub(crate) fn assert_command_matches(s: impl AsRef<str>, expect: Expect) {
    let actual_lines = s.as_ref().trim().lines().collect::<Vec<_>>();
    let expected_lines = expect.data().trim().lines().collect::<Vec<_>>();

    let mut diff_found = false;
    for (l, r) in actual_lines.iter().zip(expected_lines.iter()) {
        let actual_parts = shlex::split(l).unwrap_or_default();
        let expected_parts = shlex::split(r).unwrap_or_default();

        if actual_parts != expected_parts {
            println!(
                "Diff found:\nActual:   {:?}\nExpected: {:?}",
                actual_parts, expected_parts
            );
            diff_found = true;
            break;
        }
    }

    if diff_found {
        expect.assert_eq(s.as_ref());
    }
}

#[track_caller]
pub(crate) fn run_moon_cmdtest(case_dir: &str) {
    let test_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/test_cases")
        .join(case_dir)
        .join("moon.test");

    let update = std::env::var_os("UPDATE_EXPECT").is_some();
    let exit_code = moon_test_util::cmdtest::run::t(&test_path, &moon_bin(), update);

    assert_eq!(exit_code, 0, "cmdtest failed for {}", test_path.display());
}

#[cfg(test)]
mod tests {
    use super::replace_dir;

    #[test]
    fn replace_dir_replaces_forward_slash_root_paths() {
        let dir = tempfile::tempdir().unwrap();
        let canonical = dunce::canonicalize(dir.path()).unwrap();
        let root = canonical.to_str().unwrap().replace('\\', "/");
        let output = format!(
            "moonc check {root}/b/hello.mbt -pkg-sources username/b:{root}/b -workspace-path {root}/b"
        );

        assert_eq!(
            replace_dir(&output, dir.path()),
            "moonc check $ROOT/b/hello.mbt -pkg-sources username/b:$ROOT/b -workspace-path $ROOT/b"
        );
    }
}
