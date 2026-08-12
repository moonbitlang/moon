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

use crate::{
    moon_cmd,
    util::{moon_bin, moonrun_bin},
};

const MOONBIT_ASYNC_CHECK_FD_LEAK: &str = "MOONBIT_ASYNC_CHECK_FD_LEAK";

#[test]
#[ignore = "run in CI when Moonrun or upstream async changes"]
fn test_async_wasm_upstream() {
    let async_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .unwrap()
        .join("third_party/moonbitlang_async");
    let moonrun = moonrun_bin();
    let build_dir = async_dir.join("_build");
    std::fs::create_dir_all(&build_dir).expect("failed to create async test build dir");
    let target_dir = tempfile::Builder::new()
        .prefix("moon-test-target-")
        .tempdir_in(&build_dir)
        .expect("failed to create async test target dir");
    let moon = moon_bin();
    let path = std::env::join_paths(
        std::iter::once(
            moon.parent()
                .expect("test moon binary should have a parent directory")
                .to_path_buf(),
        )
        .chain(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        )),
    )
    .expect("failed to put the test moon binary on PATH");
    moon_cmd(&async_dir)
        .env("MOON_OVERRIDE", &moon)
        .env("MOONRUN_OVERRIDE", &moonrun)
        .env(MOONBIT_ASYNC_CHECK_FD_LEAK, "1")
        .env("PATH", path)
        .arg("--target-dir")
        .arg(target_dir.path())
        .args(["test", "--target", "wasm"])
        .assert()
        .success();
}
