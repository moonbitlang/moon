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
fn fmt_handles_its_long_format_directory() {
    let case = LongPathCase::new();
    moon_cmd(&case.dir).arg("fmt").assert().success();
    moon_cmd(&case.dir)
        .args(["fmt", "--check"])
        .assert()
        .success();
}

#[test]
fn check_handles_its_long_output_directory() {
    let case = LongPathCase::new();
    moon_cmd(&case.dir)
        .args(["check", "--target", "wasm-gc"])
        .assert()
        .success();
}

#[test]
fn info_handles_its_long_check_directory() {
    let case = LongPathCase::new();
    moon_cmd(&case.dir)
        .args(["info", "--target", "wasm-gc"])
        .assert()
        .success();
}

#[test]
fn build_handles_its_long_output_directory() {
    let case = LongPathCase::new();
    moon_cmd(&case.dir)
        .args(["build", "--target", "wasm-gc"])
        .assert()
        .success();
    // An existing output must use the same path as the initially missing one.
    moon_cmd(&case.dir)
        .args(["build", "--target", "wasm-gc"])
        .assert()
        .success();
}

#[test]
fn test_handles_its_long_output_directory() {
    let case = LongPathCase::new();
    moon_cmd(&case.dir)
        .args(["test", "--target", "wasm-gc"])
        .assert()
        .success();
}

#[test]
fn bundle_handles_its_long_output_directory() {
    let case = LongPathCase::new();
    moon_cmd(&case.dir)
        .args(["bundle", "--target", "wasm-gc"])
        .assert()
        .success();
}

#[test]
fn doc_reaches_moondoc_with_long_artifacts() {
    let case = LongPathCase::new();
    // FIXME: moondoc must preserve verbatim paths in its options and package
    // metadata. It currently normalizes the .mi path into an unusable spelling.
    moon_cmd(&case.dir)
        .arg("doc")
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
Error: failed to build docs test/long-path@0.0.0 (local [..]) (wasm-gc): [..]moondoc[EXE] [..]
missing .mi artifact for package test/long-path/[..]: /?/[..].mi
...
"#]]);
    let interface = case
        .dir
        .join("_build/wasm-gc/debug/check")
        .join(&case.package_rel)
        .join(case.package_rel.file_name().unwrap())
        .with_extension("mi");
    assert!(interface.is_file(), "moonc must produce the interface");
}

#[test]
fn run_wasm_gc_handles_its_long_output_directory() {
    let case = LongPathCase::new();
    moon_cmd(&case.dir)
        .args(["run", case.package(), "--target", "wasm-gc"])
        .assert()
        .success()
        .stdout_eq("42\n");
}

#[test]
fn build_js_handles_its_long_output_directory() {
    let case = LongPathCase::new();
    moon_cmd(&case.dir)
        .args(["build", "--target", "js"])
        .assert()
        .success();
}

#[test]
fn js_commands_reach_node_with_long_artifacts() {
    // FIXME: Node 22 rejects verbatim entrypoints (nodejs/node#62446). Remove
    // this characterization when the supported Node versions include its fix.
    for command in ["run", "test"] {
        let case = LongPathCase::new();
        moon_cmd(&case.dir)
            .args([command, "--target", "js"])
            .args((command == "run").then_some(case.package()))
            .assert()
            .failure()
            .stderr_eq(snapbox::str![[r#"
...
Error: EISDIR: illegal operation on a directory, lstat '[..]:'
...
"#]]);
    }
}

#[test]
fn check_native_handles_its_long_output_directory() {
    let case = LongPathCase::new();
    moon_cmd(&case.dir)
        .args(["check", "--target", "native"])
        .assert()
        .success();
}

// FIXME: MSVC still rejects long generated-C paths. Keep checking that moonc
// completes before the native toolchain fails; remove this when MSVC supports
// the paths, or native execution gains a separate short-path strategy.
#[test]
fn native_commands_reach_the_c_toolchain_with_long_artifacts() {
    for command in ["build", "test", "run"] {
        let case = LongPathCase::new();
        moon_cmd(&case.dir)
            .env("MOONBIT_NEW_NATIVE", "0")
            .args([command, "--target", "native"])
            .args((command == "run").then_some(case.package()))
            .assert()
            .failure();
        let action = if command == "test" { "test" } else { "build" };
        let generated = case
            .dir
            .join("_build/native/debug")
            .join(action)
            .join(&case.package_rel);
        assert!(
            std::fs::read_dir(generated).unwrap().any(|entry| {
                entry
                    .unwrap()
                    .path()
                    .extension()
                    .is_some_and(|ext| ext == "c")
            }),
            "moonc must produce C before the native toolchain fails"
        );
    }
}
