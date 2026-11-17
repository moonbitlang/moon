// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

#[cfg(feature = "v8")]
#[path = "../common/test_init.rs"]
mod test_init;

mod async_host;
mod cli;
mod embedding;
mod engine_backends;
mod guest_environment;
mod memory_sanitizer;
mod policy;
mod sqlite;
mod stack_traces;
mod stdio;

use std::{path::PathBuf, sync::OnceLock};

const MOONBIT_ASYNC_CHECK_FD_LEAK: &str = "MOONBIT_ASYNC_CHECK_FD_LEAK";

fn moon_cmd() -> snapbox::cmd::Command {
    snapbox::cmd::Command::new(moon_bin())
        .env("MOONRUN_OVERRIDE", snapbox::cmd::cargo_bin!("moonrun"))
}

fn moon_bin() -> &'static PathBuf {
    static MOON_BIN: OnceLock<PathBuf> = OnceLock::new();
    MOON_BIN.get_or_init(|| {
        let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../moon/Cargo.toml");
        // `moon` is a host tool. Keep Cargo's default target layout so it can
        // reuse artifacts from the outer Cargo invocation.
        escargot::CargoBuild::new()
            .manifest_path(manifest_path)
            .bin("moon")
            .current_release()
            .run()
            .expect("failed to build moon")
            .path()
            .to_owned()
    })
}

#[test]
#[ignore = "run in CI when Moonrun or upstream async changes"]
fn test_moonrun_against_upstream_async() {
    let async_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .unwrap()
        .join("third_party/moonbitlang_async");
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

    // The process and file-lock tests assert subsecond child-output ordering.
    // Build their native helpers before starting the async tests; a first lazy
    // compilation can otherwise overlap those deadlines and strand a pipe reader.
    moon_cmd()
        .current_dir(&async_dir)
        .args([
            "build",
            "test_programs",
            "--debug",
            "--target",
            "native",
            "--target-dir",
            "_build/test_programs",
        ])
        .assert()
        .success();

    moon_cmd()
        .current_dir(&async_dir)
        .env("MOON_OVERRIDE", moon)
        .env(MOONBIT_ASYNC_CHECK_FD_LEAK, "1")
        .env("PATH", path)
        .arg("--target-dir")
        .arg(target_dir.path())
        // Upstream process tests assert subsecond child-output ordering.
        // Avoid competing test packages delaying child startup past those gaps.
        .args(["test", "--target", "wasm", "--no-parallelize"])
        .assert()
        .success()
        .stdout_eq("Total tests: 592, passed: 592, failed: 0.\n");
}

struct TestDir(moon_test_util::test_dir::TestDir);

impl TestDir {
    // create a new TestDir with the test directory in tests/test_cases/<sub>
    fn new(sub: &str) -> Self {
        let case_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/test_cases");
        Self(moon_test_util::test_dir::TestDir::from_case_root(
            case_root, sub, false,
        ))
    }

    fn join(&self, sub: &str) -> PathBuf {
        self.0.join(sub)
    }
}

impl AsRef<std::path::Path> for TestDir {
    fn as_ref(&self) -> &std::path::Path {
        self.0.as_ref()
    }
}
