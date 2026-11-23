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
fn manifest_diagnostics_reach_build_commands() {
    let dir = TestDir::new_empty();
    std::fs::write(
        dir.join("moon.mod"),
        "name = \"test/diagnostics\"\noptions(checksum: \"obsolete\", scripts: {\"prebuild\": \"unused-command\"}, \"--moonbit-unstable-prebuild\": \"build.js\")",
    )
    .unwrap();
    std::fs::write(dir.join("main.mbt"), "fn main {}\n").unwrap();
    std::fs::write(
        dir.join("moon.pkg"),
        r#"regex_backend = "table"
options(
  "is-main": true,
  "alert-list": "+all",
  "bin-name": "obsolete-name",
  link: {
    "native": { "cc_link_flags": "-lm" },
    "wasm-gc": { "exports": [] }
  }
)
"#,
    )
    .unwrap();
    // A real build also checks that compiler-consumed options remain usable.
    moon_cmd(&dir)
        .args(["build", "--target", "wasm-gc"])
        .assert()
        .success()
        .stderr_eq(snapbox::str![[r#"
Warning: Unknown configuration key `checksum` in `[..]/moon.mod`.
Warning: Unknown configuration key `scripts.prebuild` in `[..]/moon.mod`.
Warning: Legacy stdin JSON for prebuild script `build.js` in module `test/diagnostics` is deprecated and will be removed; read `MOON_MOD`, `MOON_BUILD_DIR`, and the other `MOON_*` environment variables instead.
Warning: Unknown configuration key `bin-name` in `[..]/moon.pkg`.
Warning: `is-main: true` is deprecated; use `pkgtype(kind: "executable")` instead.
Warning: Unknown configuration key `link.native.cc_link_flags` in `[..]/moon.pkg`.
Warning: `link.wasm-gc.exports` is deprecated; use `#export_name` attributes in MoonBit source instead.
Finished. moon: ran 2 tasks, now up to date

"#]]);
    moon_cmd(&dir)
        .args(["build", "--target", "wasm-gc", "--quiet"])
        .assert()
        .success()
        .stderr_eq("");

    std::fs::write(dir.join("moon.pkg"), "warnings = 123").unwrap();
    moon_cmd(&dir)
        .args(["check", "--quiet"])
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
Error: Failed to calculate build plan

Caused by:
    0: Failed when discovering packages
    1: Unable to read `moon.pkg` for module 'test/diagnostics@0.0.0 (local [..])' package '' at path '[..]', error: `warnings` in moon.pkg must be a string

"#]]);
}

#[test]
fn removed_package_name_requires_explicit_pkgtype() {
    let dir = TestDir::new_empty();
    std::fs::write(dir.join("moon.mod"), "name = \"test/removed-name\"\n").unwrap();
    std::fs::write(dir.join("moon.pkg"), "options(name: \"main\")\n").unwrap();
    std::fs::write(dir.join("main.mbt"), "fn main { println(\"ok\") }\n").unwrap();

    moon_cmd(&dir)
        .args(["run", ".", "--target", "wasm-gc"])
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
Warning: Unknown configuration key `name` in `[..]/moon.pkg`.
Error: `.` is not a main package

"#]]);

    std::fs::write(dir.join("moon.pkg"), "pkgtype(kind: \"executable\")\n").unwrap();
    moon_cmd(&dir)
        .args(["run", ".", "--target", "wasm-gc", "--quiet"])
        .assert()
        .success()
        .stdout_eq("ok\n")
        .stderr_eq("");
}

#[test]
fn duplicate_module_target_aliases_are_errors_even_when_quiet() {
    let dir = TestDir::new_empty();
    std::fs::write(
        dir.join("moon.mod"),
        "name = \"test/aliases\"\npreferred_target = \"wasm\"\noptions(\"preferred-target\": 123)\n",
    )
    .unwrap();
    std::fs::write(dir.join("moon.pkg"), "").unwrap();
    moon_cmd(&dir)
        .args(["check", "--quiet"])
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
Error: Failed to calculate build plan

Caused by:
    0: Failed to resolve the module dependency graph
    1: Duplicate key 'preferred_target' found in moon.mod.

"#]]);
}
