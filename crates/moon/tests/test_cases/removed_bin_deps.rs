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

use super::*;

#[test]
fn root_bin_deps_fail_before_dependency_acquisition() {
    for dependency in [r#""0.1.0""#, r#"{"path": "missing-tool"}"#] {
        let dir = TestDir::new_empty();
        std::fs::write(
            dir.join("moon.mod"),
            format!(
                "name = \"test/removed\"\noptions(\"bin-deps\": {{\"missing/tool\": {dependency}}})\n"
            ),
        )
        .unwrap();
        for command in ["check", "build", "test", "bench", "bundle", "tree"] {
            moon_cmd(&dir)
                .args([command, "--quiet"])
                .assert()
                .failure()
                .stdout_eq("")
                .stderr_eq(snapbox::str![[r#"
...
[..]`bin-deps` is no longer supported in module `test/removed`; remove it and run portable Wasm tools with `moonx` instead
...
"#]]);
        }
        assert!(!dir.join(".mooncakes").exists());
    }
}

#[test]
fn workspace_sync_rejects_member_bin_deps() {
    let dir = TestDir::new_empty();
    std::fs::write(dir.join("moon.work"), "members = [\"app\"]\n").unwrap();
    std::fs::create_dir(dir.join("app")).unwrap();
    std::fs::write(
        dir.join("app/moon.mod"),
        "name = \"test/member\"\noptions(\"bin-deps\": {\"missing/tool\": \"0.1.0\"})\n",
    )
    .unwrap();
    moon_cmd(&dir)
        .args(["work", "sync"])
        .assert()
        .failure()
        .stdout_eq("")
        .stderr_eq(snapbox::str![[r#"
...
[..]`bin-deps` is no longer supported in module `test/member`; remove it and run portable Wasm tools with `moonx` instead
...
"#]]);
}

#[test]
fn legacy_dependency_bin_deps_and_empty_root_declaration_are_ignored() {
    let dir = TestDir::new_empty();
    let moon_home = tempfile::tempdir().unwrap();
    cache_registry_package(
        moon_home.path(),
        "test/dep",
        "0.1.0",
        &[
            (
                "moon.mod",
                b"name = \"test/dep\"\nversion = \"0.1.0\"\noptions(\"bin-deps\": {\"missing/tool\": \"0.1.0\"})\n".to_vec(),
            ),
            ("moon.pkg", b"".to_vec()),
            ("lib.mbt", b"pub fn value() -> Int { 42 }\n".to_vec()),
        ],
    );
    std::fs::write(dir.join("moon.pkg"), "import { \"test/dep\" }\n").unwrap();
    std::fs::write(
        dir.join("lib.mbt"),
        "pub fn value() -> Int { @dep.value() }\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("moon.mod"),
        "name = \"test/root\"\nimport { \"test/dep@0.1.0\" }\noptions(\"bin-deps\": {})\n",
    )
    .unwrap();
    moon_cmd(&dir)
        .env("MOON_HOME", moon_home.path())
        .args(["check", "--quiet"])
        .assert()
        .success()
        .stdout_eq("")
        .stderr_eq("");
}
