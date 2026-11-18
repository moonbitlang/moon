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
        "name = \"test/diagnostics\"\noptions(future: true)",
    )
    .unwrap();
    std::fs::write(dir.join("main.mbt"), "fn main {}\n").unwrap();
    std::fs::write(
        dir.join("moon.pkg"),
        r#"pkgtype(kind: "executable")
regex_backend = "table"
options(
  "alert-list": "+all",
  link: {
    "native": { "cc_link_flags": "-lm" },
    "wasm-gc": { "use-ffi-bytes": true }
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
Warning: Unknown configuration key `future` in `[..]/moon.mod`.
Warning: Unknown configuration key `link.native.cc_link_flags` in `[..]/moon.pkg`.
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
