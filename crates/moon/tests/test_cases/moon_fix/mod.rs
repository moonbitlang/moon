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

//! CLI tests for `moon fix`. The released compiler does not attach fixes yet,
//! so a stand-in `moonc` (`tests/fixtures/fix_moonc.rs`) replays the
//! diagnostics written next to each source file. Every check the fix driver
//! makes has unit tests beside it; these tests cover the command: running the
//! check, agreeing across backends, writing files, rounds, and output.

use super::*;

/// The stand-in moonc, compiled once per test process.
static FAKE_MOONC: std::sync::LazyLock<PathBuf> = std::sync::LazyLock::new(|| {
    let moonc = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "fix-moonc-{}{}",
        std::process::id(),
        std::env::consts::EXE_SUFFIX
    ));
    snapbox::cmd::Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fix_moonc.rs"))
        .args(["--edition", "2021", "-o"])
        .arg(&moonc)
        .assert()
        .success();
    moonc
});

/// A project with one package at its root holding `files`, and the stand-in
/// moonc. `files` maps each name to its content; a `F.diagnostics` file holds
/// the canned diagnostics of `F` (see the fixture).
fn project(files: &[(&str, &str)]) -> (TestDir, PathBuf) {
    let dir = TestDir::new_empty();
    let project = dir.join("project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("moon.mod"), "name = \"u/p\"\n").unwrap();
    std::fs::write(project.join("moon.pkg"), "").unwrap();
    for (name, content) in files {
        std::fs::write(project.join(name), content).unwrap();
    }
    (dir, FAKE_MOONC.clone())
}

fn fix_cmd(dir: &TestDir, moonc: &Path) -> snapbox::cmd::Command {
    moon_cmd(dir)
        .current_dir(dir.join("project"))
        .env("MOONC_OVERRIDE", moonc)
        .arg("fix")
}

fn read_file(dir: &TestDir, name: &str) -> String {
    std::fs::read_to_string(dir.join("project").join(name)).unwrap()
}

/// An unused-`mut` diagnostic proposing to remove `mut` from `let mut x` on
/// line `line`, reported while the file still contains `let mut x`. Its level
/// is error, as moonc reports unused `mut` by default: fixes of errors apply.
fn unused_mut(target: &str, line: u32) -> String {
    format!(
        r#"{target}|let mut x|{{"$message_type":"diagnostic","level":"error","error_code":15,"path":"$FILE","loc":"{line}:11-{line}:12","message":"unused mut","fixes":[{{"title":"Remove `mut`","edits":[{{"path":"$FILE","loc":"{line}:3-{line}:11","expect":"let mut ","new_text":"let "}}]}}]}}"#
    )
}

const UNUSED_MUT_SOURCE: &str = "fn f() -> Int {\n  let mut x = 1\n  x\n}\n";
const FIXED_SOURCE: &str = "fn f() -> Int {\n  let x = 1\n  x\n}\n";

#[test]
fn fix_applies_proposed_fixes_and_lists_them() {
    // Given a file whose check proposes removing an unused `mut`,
    let (dir, moonc) = project(&[
        ("a.mbt", UNUSED_MUT_SOURCE),
        ("a.mbt.diagnostics", &unused_mut("*", 2)),
    ]);
    // when running `moon fix`, the fix is written and listed on stdout. The
    // second round's check reports nothing, so nothing remains.
    fix_cmd(&dir, &moonc)
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
a.mbt:2:3: Remove `mut`

"#]])
        .stderr_eq(snapbox::str![[r#"
Applied 1 fix to 1 file

"#]]);
    assert_eq!(read_file(&dir, "a.mbt"), FIXED_SOURCE);

    // Running again finds nothing to fix.
    fix_cmd(&dir, &moonc)
        .assert()
        .success()
        .stdout_eq(snapbox::str![""])
        .stderr_eq(snapbox::str![[r#"
No fixes to apply

"#]]);
}

#[test]
fn fix_diff_prints_the_changes_without_writing_them() {
    let (dir, moonc) = project(&[
        ("a.mbt", UNUSED_MUT_SOURCE),
        ("a.mbt.diagnostics", &unused_mut("*", 2)),
    ]);
    fix_cmd(&dir, &moonc)
        .arg("--diff")
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
--- a/a.mbt
+++ b/a.mbt
@@ -1,4 +1,4 @@
 fn f() -> Int {
-  let mut x = 1
+  let x = 1
   x
 }

"#]])
        .stderr_eq(snapbox::str![[r#"
Would apply 1 fix to 1 file

"#]]);
    assert_eq!(read_file(&dir, "a.mbt"), UNUSED_MUT_SOURCE);
}

#[test]
fn fix_skips_fixes_that_do_not_match_the_files() {
    // Given fixes that fail the driver checks: one whose range no longer
    // reads its expected text, one that would join `return` and `C`, and one
    // in a file with a lone CR line break, where moonc's lines may differ.
    let stale = r#"*||{"$message_type":"diagnostic","level":"warning","error_code":1,"path":"$FILE","loc":"1:1-1:2","message":"stale","fixes":[{"title":"Stale","edits":[{"path":"$FILE","loc":"1:1-1:4","expect":"let","new_text":"var"}]}]}"#;
    let joins = r#"*||{"$message_type":"diagnostic","level":"warning","error_code":1,"path":"$FILE","loc":"2:3-2:15","message":"qualified","fixes":[{"title":"Drop the package","edits":[{"path":"$FILE","loc":"2:9-2:14","expect":"@pkg.","new_text":""}]}]}"#;
    let (dir, moonc) = project(&[
        ("stale.mbt", "fn s() -> Unit { () }\n"),
        ("stale.mbt.diagnostics", stale),
        ("joins.mbt", "fn j() -> Unit {\n  return@pkg.C\n}\n"),
        ("joins.mbt.diagnostics", joins),
        ("cr.mbt", "fn f() -> Int {\r  let mut x = 1\n  x\n}\n"),
        ("cr.mbt.diagnostics", &unused_mut("*", 2)),
    ]);
    // When running `moon fix --verbose`, no file changes and each skip says
    // why.
    fix_cmd(&dir, &moonc)
        .arg("--verbose")
        .assert()
        .success()
        .stdout_eq(snapbox::str![""])
        .stderr_eq(snapbox::str![[r#"
...
skipped fix `Drop the package` at joins.mbt:2:9: the fix would join two tokens
skipped fix `Remove `mut`` at cr.mbt:2:3: the file has a line break other than LF or CRLF, or a line too long
skipped fix `Stale` at stale.mbt:1:1: the file does not read as the fix expects
No fixes to apply
Skipped 3 fixes
...
"#]]);
    assert_eq!(read_file(&dir, "stale.mbt"), "fn s() -> Unit { () }\n");
    assert_eq!(
        read_file(&dir, "joins.mbt"),
        "fn j() -> Unit {\n  return@pkg.C\n}\n"
    );
    assert_eq!(
        read_file(&dir, "cr.mbt"),
        "fn f() -> Int {\r  let mut x = 1\n  x\n}\n"
    );
}

#[test]
fn fix_applies_a_fix_only_if_every_backend_compiling_the_file_proposes_it() {
    // Given a fix that only the js check proposes, in a file every backend
    // compiles, and another in a file only js compiles,
    let (dir, moonc) = project(&[
        ("a.mbt", UNUSED_MUT_SOURCE),
        ("a.mbt.diagnostics", &unused_mut("js", 2)),
        ("b.js.mbt", UNUSED_MUT_SOURCE.replace("f()", "g()").as_str()),
        ("b.js.mbt.diagnostics", &unused_mut("js", 2)),
    ]);
    // checking js and wasm-gc applies only the second: wasm-gc compiles
    // `a.mbt` without proposing its fix, and does not compile `b.js.mbt`.
    fix_cmd(&dir, &moonc)
        .args(["--target", "js,wasm-gc", "--verbose"])
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
b.js.mbt:2:3: Remove `mut`

"#]])
        .stderr_eq(snapbox::str![[r#"
...
skipped fix `Remove `mut`` at a.mbt:2:3: not proposed by every target backend that compiles the file
Applied 1 fix to 1 file
Skipped 1 fix
...
"#]]);
    assert_eq!(read_file(&dir, "a.mbt"), UNUSED_MUT_SOURCE);

    // Checking js alone applies the first as well.
    fix_cmd(&dir, &moonc)
        .args(["--target", "js"])
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
a.mbt:2:3: Remove `mut`

"#]]);
    assert_eq!(read_file(&dir, "a.mbt"), FIXED_SOURCE);
}

#[test]
fn fix_runs_rounds_until_no_fix_applies() {
    // Given a file whose check proposes replacing `var x` by `let mut x`,
    // which then has an unused `mut`: the second round's check, which sees
    // the first fix, proposes removing it.
    let var = r#"*|var x|{"$message_type":"diagnostic","level":"warning","error_code":1,"path":"$FILE","loc":"2:3-2:6","message":"var","fixes":[{"title":"Replace `var` with `let mut`","edits":[{"path":"$FILE","loc":"2:3-2:7","expect":"var ","new_text":"let mut "}]}]}"#;
    let (dir, moonc) = project(&[
        ("a.mbt", "fn f() -> Int {\n  var x = 1\n  x\n}\n"),
        (
            "a.mbt.diagnostics",
            &format!("{var}\n{}\n", unused_mut("*", 2)),
        ),
    ]);
    fix_cmd(&dir, &moonc)
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
a.mbt:2:3: Replace `var` with `let mut`
a.mbt:2:3: Remove `mut`

"#]])
        .stderr_eq(snapbox::str![[r#"
Applied 2 fixes to 1 file

"#]]);
    assert_eq!(read_file(&dir, "a.mbt"), FIXED_SOURCE);
}
