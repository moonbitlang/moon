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

use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

use expect_test::Expect;
use moonutil::{compiler_flags, text::StringExt};

static MOONRUN_BIN: OnceLock<PathBuf> = OnceLock::new();

pub(crate) fn check<S: AsRef<str>>(actual: S, expect: Expect) {
    expect.assert_eq(actual.as_ref())
}

pub(crate) fn moon_bin() -> PathBuf {
    snapbox::cargo_bin!("moon").to_owned()
}

pub(crate) fn moonrun_bin() -> PathBuf {
    MOONRUN_BIN
        .get_or_init(|| {
            escargot::CargoBuild::new()
                .manifest_path(
                    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../moonrun/Cargo.toml"),
                )
                .bin("moonrun")
                .current_release()
                .current_target()
                .run()
                .expect("failed to build moonrun")
                .path()
                .to_owned()
        })
        .clone()
}

pub(crate) fn toolchain_root_for_tests() -> PathBuf {
    if let Some(path) = std::env::var_os("MOON_TOOLCHAIN_ROOT") {
        return PathBuf::from(path);
    }

    let moonc = std::fs::canonicalize(&*moonutil::toolchain::BINARIES.moonc)
        .unwrap_or(moonutil::toolchain::BINARIES.moonc.clone());
    if let Some(bin_dir) = moonc.parent()
        && bin_dir.file_name().is_some_and(|name| name == "bin")
        && let Some(root) = bin_dir.parent()
        && moonutil::toolchain::is_toolchain_root(root)
    {
        return root.to_path_buf();
    }

    moonutil::toolchain::toolchain_root()
}

fn replace_known_paths(mut output: String, known_paths: &[(PathBuf, String)]) -> String {
    for (path, replacement) in known_paths {
        let mut redactions = snapbox::Redactions::new();
        moon_test_util::insert_path_redaction(&mut redactions, "[MOON_TEST_KNOWN_PATH]", path)
            .expect("valid known path redaction");
        output = redactions
            .redact(&output)
            .replace("[MOON_TEST_KNOWN_PATH]", replacement);
    }
    output
}

pub(crate) fn replace_dir(s: &str, dir: impl AsRef<std::path::Path>) -> String {
    let mut known_paths = moonutil::toolchain::BINARIES
        .all_moon_bins()
        .into_iter()
        .map(|(name, path)| {
            let path = match name {
                #[allow(deprecated)]
                "moon" | "moonrun" => snapbox::cmd::cargo_bin(name),
                _ => path,
            };
            (path, name.to_owned())
        })
        .collect::<Vec<_>>();
    if let Some(path) = MOONRUN_BIN.get() {
        known_paths.push((path.clone(), "moonrun".to_owned()));
    }
    if let Ok(toolchain) = compiler_flags::default_native_toolchain(None) {
        let cc = toolchain.cc();
        known_paths.extend([
            (PathBuf::from(&cc.ar_path), cc.ar_name().to_owned()),
            (PathBuf::from(&cc.cc_path), cc.cc_name().to_owned()),
        ]);
    }
    known_paths.push((moon_bin(), "moon".to_owned()));

    let toolchain_root = toolchain_root_for_tests();
    let moon_home = moonutil::toolchain::home();
    let show_toolchain_root = match (
        std::fs::canonicalize(&toolchain_root),
        std::fs::canonicalize(&moon_home),
    ) {
        (Ok(toolchain_root), Ok(moon_home)) => toolchain_root != moon_home,
        _ => toolchain_root != moon_home,
    };

    let mut path_redactions = snapbox::Redactions::new();
    moon_test_util::insert_path_redaction(&mut path_redactions, "[ROOT]", dir.as_ref())
        .expect("valid ROOT redaction");
    if show_toolchain_root {
        moon_test_util::insert_path_redaction(
            &mut path_redactions,
            "[MOON_TOOLCHAIN_ROOT]",
            &toolchain_root,
        )
        .expect("valid MOON_TOOLCHAIN_ROOT redaction");
    }
    moon_test_util::insert_path_redaction(&mut path_redactions, "[MOON_HOME]", &moon_home)
        .expect("valid MOON_HOME redaction");

    let output = replace_known_paths(s.to_owned(), &known_paths);
    let output = path_redactions.redact(&output);
    // JSON diagnostics escape backslashes. Collapse them only after redacting
    // ordinary paths so a verbatim `\\?\` prefix is not damaged.
    let output = output.replace("\\\\", "\\");
    let output = replace_known_paths(output, &known_paths);
    let output = path_redactions.redact(&output);
    let output = output
        .replace("[ROOT]", "$ROOT")
        .replace("[MOON_TOOLCHAIN_ROOT]", "$MOON_TOOLCHAIN_ROOT")
        .replace("[MOON_HOME]", "$MOON_HOME")
        .replace("moon.exe", "moon");
    snapbox::filter::normalize_paths(&snapbox::filter::normalize_lines(&output))
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
    let diff_found = command_lines_differ(s.as_ref(), expect.data()).unwrap_or_else(|err| {
        panic!("{err}");
    });

    if diff_found {
        expect.assert_eq(s.as_ref());
    }
}

fn command_lines_differ(actual: &str, expected: &str) -> Result<bool, String> {
    let actual_lines = actual.trim().lines().collect::<Vec<_>>();
    let expected_lines = expected.trim().lines().collect::<Vec<_>>();

    if actual_lines.len() != expected_lines.len() {
        println!(
            "Line count differs:\nActual:   {}\nExpected: {}",
            actual_lines.len(),
            expected_lines.len()
        );
        return Ok(true);
    }

    for (idx, (actual_line, expected_line)) in
        actual_lines.iter().zip(expected_lines.iter()).enumerate()
    {
        let actual_parts = split_command_line("actual", idx, actual_line)?;
        let expected_parts = split_command_line("expected", idx, expected_line)?;

        if actual_parts != expected_parts {
            println!(
                "Diff found on line {}:\nActual:   {:?}\nExpected: {:?}",
                idx + 1,
                actual_parts,
                expected_parts
            );
            return Ok(true);
        }
    }

    Ok(false)
}

fn split_command_line(kind: &str, idx: usize, line: &str) -> Result<Vec<String>, String> {
    shlex::split(line).ok_or_else(|| {
        format!(
            "failed to parse {kind} command line {} with shlex: {line:?}",
            idx + 1
        )
    })
}

#[track_caller]
pub(crate) fn run_moon_cmdtest(case_dir: &str) {
    let test_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/test_cases")
        .join(case_dir)
        .join("moon.test");

    let update = std::env::var_os("UPDATE_EXPECT").is_some();
    let toolchain_root = toolchain_root_for_tests();
    let exit_code =
        moon_test_util::cmdtest::run::t(&test_path, &moon_bin(), update, Some(&toolchain_root));

    assert_eq!(exit_code, 0, "cmdtest failed for {}", test_path.display());
}

#[cfg(test)]
mod tests {
    use super::{assert_command_matches, replace_dir};
    use expect_test::expect;

    #[test]
    fn replace_dir_replaces_forward_slash_root_paths() {
        let dir = tempfile::tempdir().unwrap();
        let canonical = std::fs::canonicalize(dir.path()).unwrap();
        let root = canonical.to_str().unwrap().replace('\\', "/");
        let output = format!(
            "moonc check {root}/b/hello.mbt -pkg-sources username/b:{root}/b -workspace-path {root}/b"
        );

        assert_eq!(
            replace_dir(&output, dir.path()),
            "moonc check $ROOT/b/hello.mbt -pkg-sources username/b:$ROOT/b -workspace-path $ROOT/b"
        );
    }

    #[test]
    fn replace_dir_preserves_cli_path_metavariables() {
        let dir = tempfile::tempdir().unwrap();

        assert_eq!(
            replace_dir("Usage: moon test --release [PATH]...", dir.path()),
            "Usage: moon test --release [PATH]..."
        );
    }

    #[cfg(windows)]
    #[test]
    fn replace_dir_redacts_both_windows_path_spellings_and_json_escaping() {
        let dir = tempfile::tempdir().unwrap();
        let canonical_file = std::fs::canonicalize(dir.path()).unwrap().join("main.mbt");
        let legacy = canonical_file
            .to_string_lossy()
            .strip_prefix(r"\\?\")
            .unwrap()
            .to_owned();
        let escaped = canonical_file.to_string_lossy().replace('\\', "\\\\");
        let output = format!("{}\n{}\n{escaped}", canonical_file.display(), legacy,);

        assert_eq!(
            replace_dir(&output, dir.path()),
            "$ROOT/main.mbt\n$ROOT/main.mbt\n$ROOT/main.mbt"
        );
    }

    #[test]
    #[should_panic]
    fn assert_command_matches_rejects_added_lines() {
        assert_command_matches(
            "moonc check\nmoonc build",
            expect![[r#"
                moonc check
            "#]],
        );
    }

    #[test]
    #[should_panic(expected = "failed to parse actual command line 1")]
    fn assert_command_matches_rejects_invalid_shell_syntax() {
        assert_command_matches(
            "moonc \"unterminated",
            expect![[r#"
                moonc
            "#]],
        );
    }
}
