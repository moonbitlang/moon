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
    time::Instant,
};

use expect_test::Expect;
use moonutil::{common::StringExt, compiler_flags::CC};

pub(crate) fn check<S: AsRef<str>>(actual: S, expect: Expect) {
    expect.assert_eq(actual.as_ref())
}

struct ReplaceDirCache {
    moon_bin: String,
    moon_home: String,
    cc_path: String,
    cc_name: String,
    ar_path: String,
    ar_name: String,
    node: Option<String>,
    moon_binaries: Vec<(String, String)>,
}

pub(crate) fn moon_bin() -> &'static PathBuf {
    static MOON_BIN: OnceLock<PathBuf> = OnceLock::new();
    MOON_BIN.get_or_init(|| snapbox::cargo_bin!("moon").to_owned())
}

fn replace_dir_cache() -> &'static ReplaceDirCache {
    static CACHE: OnceLock<ReplaceDirCache> = OnceLock::new();
    CACHE.get_or_init(|| {
        let cc = CC::default();
        let moon_binaries = moonutil::BINARIES
            .all_moon_bins()
            .iter()
            .map(|(name, path)| {
                let path = match *name {
                    #[allow(deprecated)]
                    "moon" | "moonrun" => snapbox::cmd::cargo_bin(name),
                    _ => path.clone(),
                };
                (path.to_string_lossy().into_owned(), (*name).to_string())
            })
            .collect();

        ReplaceDirCache {
            moon_bin: moon_bin().to_string_lossy().into_owned(),
            moon_home: dunce::canonicalize(moonutil::moon_dir::home())
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            cc_path: cc.cc_path.clone(),
            cc_name: cc.cc_name().to_string(),
            ar_path: cc.ar_path.clone(),
            ar_name: cc.ar_name().to_string(),
            node: moonutil::BINARIES
                .node
                .as_ref()
                .map(|node| node.to_string_lossy().into_owned()),
            moon_binaries,
        }
    })
}

pub(crate) fn replace_dir(s: &str, dir: impl AsRef<std::path::Path>) -> String {
    let start = Instant::now();
    let cache = replace_dir_cache();
    let s = s.replace("\\\\", "\\");
    let s = cache
        .moon_binaries
        .iter()
        .fold(s.to_string(), |s, (path, name)| s.replace(path, name));
    let path_str1 = dunce::canonicalize(dir)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    // for something like "{...\"loc\":{\"path\":\"C:\\\\Users\\\\runneradmin\\\\AppData\\\\Local\\\\Temp\\\\.tmpP0u4VZ\\\\main\\\\main.mbt\"...\r\n" on windows
    // https://github.com/moonbitlang/moon/actions/runs/10092428950/job/27906057649#step:13:149
    let s = s.replace(&path_str1, "$ROOT");
    let s = s.replace(&cache.moon_home, "$MOON_HOME");
    let s = s.replace(&cache.ar_path, &cache.ar_name);
    let s = s.replace(&cache.cc_path, &cache.cc_name);
    let s = s.replace(&cache.moon_bin, "moon");
    let s = cache
        .node
        .as_ref()
        .map(|node| s.replace(node, "node"))
        .unwrap_or(s);
    let normalized = s.replace("\r\n", "\n").replace('\\', "/");
    moon_test_util::perf::record_normalize_output(start.elapsed());
    normalized
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
    let exit_code = moon_test_util::cmdtest::run::t(&test_path, moon_bin(), update);

    assert_eq!(exit_code, 0, "cmdtest failed for {}", test_path.display());
}
