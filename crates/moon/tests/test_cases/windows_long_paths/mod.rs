use super::*;
use std::{
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
};

const LEGACY_PATH_LIMIT: usize = 260;
const COMMAND_LINE_LIMIT: usize = 32_767;

fn windows_path_len(path: &Path) -> usize {
    path.as_os_str().encode_wide().count()
}

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

struct LongPathCase {
    dir: TestDir,
    package_rel: PathBuf,
}

impl LongPathCase {
    fn new() -> Self {
        let dir = TestDir::new_empty();
        let root = dir.as_ref();
        assert!(
            windows_path_len(&root.join("_build")) < LEGACY_PATH_LIMIT,
            "the test needs a shallow target root"
        );

        let mut package_rel = PathBuf::new();
        // Keep all inputs below the legacy limit while making the package deep
        // enough for its generated artifacts to cross it.
        for index in 0.. {
            let next = package_rel.join(format!("segment{index:02}"));
            if windows_path_len(&root.join(&next).join("moon.pkg")) >= LEGACY_PATH_LIMIT {
                break;
            }
            package_rel = next;
        }

        let package_dir = root.join(&package_rel);
        let package_manifest = package_dir.join("moon.pkg");
        let source_file = package_dir.join("lib.mbt");
        assert!(
            windows_path_len(&package_manifest) < LEGACY_PATH_LIMIT,
            "the package manifest must remain below the legacy path limit"
        );
        assert!(
            windows_path_len(&source_file) < LEGACY_PATH_LIMIT,
            "the source file must remain below the legacy path limit"
        );

        // `moon doc` has no `--target`, so pin the fixture while passing the
        // target explicitly to commands that support it.
        write_file(
            &root.join("moon.mod"),
            "name = \"test/long-path\"\npreferred_target = \"wasm-gc\"\n",
        );
        write_file(&package_manifest, "pkgtype(kind: \"executable\")\n");
        write_file(
            &source_file,
            r#"pub fn answer() -> Int { 42 }

fn main {
  println(answer())
}

test "answer" {
  inspect(answer(), content="42")
}
"#,
        );

        Self { dir, package_rel }
    }

    fn package(&self) -> &str {
        self.package_rel
            .to_str()
            .expect("generated package path should be valid UTF-8")
    }

    #[track_caller]
    fn assert_compiler_path_failure(
        &self,
        args: &[&str],
        backend: &str,
        profile: &str,
        action: &str,
    ) {
        let expected = format!(
            r#"
         --  --[..]
       /  //  / __--------_
      /  //  /_/            /
   ---      -                / __
  / X        /        ____   /   )
  *_________/__/_____/______/ `--

Oops, the compiler has encountered an unexpected situation.
This is a bug in the compiler.

A bug report containing the error description and relevant code would be
greatly appreciated. You can submit the bug report here:

  https://github.com/moonbitlang/moonbit-docs/issues/new?template=ice.md

Error: Sys_error("[..]_build[..]{backend}[..]{profile}[..]{action}[..]: No such file or directory")

Compiler args: [..]

moonc version: [..]
...
"#
        );

        moon_cmd(&self.dir)
            .env("NO_COLOR", "1")
            .args(args)
            .assert()
            .failure()
            .stderr_eq(expected);
    }
}

#[test]
fn dry_run_plans_an_artifact_beyond_the_legacy_path_limit() {
    let case = LongPathCase::new();
    let root = case.dir.as_ref();
    let dry_run = moon_cmd(&case.dir)
        .args(["check", "--target", "wasm-gc", "--dry-run", "--sort-input"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let dry_run = String::from_utf8(dry_run).expect("dry-run output should be UTF-8");
    let command_args = dry_run
        .lines()
        .find_map(|line| {
            let args = moonutil::shlex::split_native(line);
            args.windows(2).any(|pair| pair[0] == "-o").then_some(args)
        })
        .expect("dry run should expose the compiler command");
    let output_path = command_args
        .windows(2)
        .find(|pair| pair[0] == "-o")
        .map(|pair| PathBuf::from(&pair[1]))
        .expect("dry run should expose the package artifact after `-o`");
    let output_path = if output_path.is_absolute() {
        output_path
    } else {
        root.join(output_path)
    };
    assert!(
        windows_path_len(&output_path) > LEGACY_PATH_LIMIT,
        "the generated artifact must cross the legacy path limit"
    );
}

#[test]
fn build_succeeds_with_an_oversized_compiler_command() {
    let dir = TestDir::new_empty();
    let root = dir.as_ref();
    write_file(
        &root.join("moon.mod"),
        "name = \"test/long-command-line\"\n",
    );
    write_file(&root.join("moon.pkg"), "");

    let total_source_path_len = (0..400)
        .map(|index| {
            let source = root.join(format!(
                "source_{index:04}_with_a_deliberately_long_name_for_command_line_testing.mbt"
            ));
            write_file(&source, "");
            windows_path_len(&source) + 1
        })
        .sum::<usize>();
    assert!(
        total_source_path_len > COMMAND_LINE_LIMIT,
        "source paths must exceed the Windows command-line limit"
    );

    moon_cmd(&dir)
        .args(["build", "--target", "wasm-gc"])
        .assert()
        .success();
}

#[test]
fn fmt_reports_its_long_format_directory() {
    let case = LongPathCase::new();
    moon_cmd(&case.dir)
        .env("NO_COLOR", "1")
        .arg("fmt")
        .assert()
        .failure()
        .stdout_eq(snapbox::str![[r#"
Fatal error: exception Sys_error("[..]_build[..]wasm-gc[..]release[..]format[..]: No such file or directory")
...
"#]])
        .stderr_eq(snapbox::str![[r#"
"#]]);
}

#[test]
fn check_reports_its_long_output_directory() {
    LongPathCase::new().assert_compiler_path_failure(
        &["check", "--target", "wasm-gc"],
        "wasm-gc",
        "debug",
        "check",
    );
}

#[test]
fn info_reports_its_long_check_directory() {
    LongPathCase::new().assert_compiler_path_failure(
        &["info", "--target", "wasm-gc"],
        "wasm-gc",
        "debug",
        "check",
    );
}

#[test]
fn build_reports_its_long_output_directory() {
    LongPathCase::new().assert_compiler_path_failure(
        &["build", "--target", "wasm-gc"],
        "wasm-gc",
        "debug",
        "build",
    );
}

#[test]
fn test_reports_its_long_output_directory() {
    LongPathCase::new().assert_compiler_path_failure(
        &["test", "--target", "wasm-gc"],
        "wasm-gc",
        "debug",
        "test",
    );
}

#[test]
fn bundle_reports_its_long_output_directory() {
    LongPathCase::new().assert_compiler_path_failure(
        &["bundle", "--target", "wasm-gc"],
        "wasm-gc",
        "release",
        "bundle",
    );
}

// These higher-level commands currently stop in `moonc`, before moondoc, Node,
// or the native C toolchain is reached. Keep them separate so a future change
// reveals exactly which boundary becomes reachable.
#[test]
fn doc_reports_its_long_check_directory() {
    LongPathCase::new().assert_compiler_path_failure(&["doc"], "wasm-gc", "debug", "check");
}

#[test]
fn run_wasm_gc_reports_its_long_output_directory() {
    let case = LongPathCase::new();
    case.assert_compiler_path_failure(
        &["run", case.package(), "--target", "wasm-gc"],
        "wasm-gc",
        "debug",
        "build",
    );
}

#[test]
fn run_js_reports_its_long_output_directory() {
    let case = LongPathCase::new();
    case.assert_compiler_path_failure(
        &["run", case.package(), "--target", "js"],
        "js",
        "debug",
        "build",
    );
}

#[test]
fn test_js_reports_its_long_output_directory() {
    LongPathCase::new().assert_compiler_path_failure(
        &["test", "--target", "js"],
        "js",
        "debug",
        "test",
    );
}

#[test]
fn check_native_reports_its_long_output_directory() {
    LongPathCase::new().assert_compiler_path_failure(
        &["check", "--target", "native"],
        "native",
        "debug",
        "check",
    );
}

#[test]
fn info_native_reports_its_long_check_directory() {
    LongPathCase::new().assert_compiler_path_failure(
        &["info", "--target", "native"],
        "native",
        "debug",
        "check",
    );
}

#[test]
fn build_native_reports_its_long_output_directory() {
    LongPathCase::new().assert_compiler_path_failure(
        &["build", "--target", "native"],
        "native",
        "debug",
        "build",
    );
}

#[test]
fn test_native_reports_its_long_output_directory() {
    LongPathCase::new().assert_compiler_path_failure(
        &["test", "--target", "native"],
        "native",
        "debug",
        "test",
    );
}

#[test]
fn run_native_reports_its_long_output_directory() {
    let case = LongPathCase::new();
    case.assert_compiler_path_failure(
        &["run", case.package(), "--target", "native"],
        "native",
        "debug",
        "build",
    );
}
