use super::*;
use std::{
    fmt::Write as _,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
};

const LEGACY_PATH_LIMIT: usize = 260;

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
            if windows_path_len(&root.join(&next).join("moon.pkg.json")) >= LEGACY_PATH_LIMIT {
                break;
            }
            package_rel = next;
        }

        let package_dir = root.join(&package_rel);
        let package_manifest = package_dir.join("moon.pkg.json");
        let source_file = package_dir.join("lib.mbt");
        assert!(
            windows_path_len(&package_manifest) < LEGACY_PATH_LIMIT,
            "the package manifest must remain below the legacy path limit"
        );
        assert!(
            windows_path_len(&source_file) < LEGACY_PATH_LIMIT,
            "the source file must remain below the legacy path limit"
        );

        write_file(
            &root.join("moon.mod.json"),
            r#"{
  "name": "test/long-path"
}
"#,
        );
        write_file(
            &package_manifest,
            r#"{
  "is-main": true
}
"#,
        );
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

        let dry_run = moon_cmd(&dir)
            .args(["check", "--dry-run", "--sort-input"])
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

        Self { dir, package_rel }
    }
}

#[test]
fn records_toolchain_behavior_beyond_the_legacy_path_limit() {
    let case = LongPathCase::new();
    let package = case
        .package_rel
        .to_str()
        .expect("generated package path should be valid UTF-8");
    let commands = [
        ("fmt", vec!["fmt"]),
        ("check", vec!["check"]),
        ("info", vec!["info"]),
        ("build", vec!["build"]),
        ("test", vec!["test"]),
        ("bundle", vec!["bundle"]),
        ("doc", vec!["doc"]),
        ("run (wasm-gc)", vec!["run", package]),
        ("run (js)", vec!["run", package, "--target", "js"]),
        ("test (js)", vec!["test", "--target", "js"]),
        ("check (native)", vec!["check", "--target", "native"]),
        ("info (native)", vec!["info", "--target", "native"]),
        ("build (native)", vec!["build", "--target", "native"]),
        ("test (native)", vec!["test", "--target", "native"]),
        ("run (native)", vec!["run", package, "--target", "native"]),
    ];

    let mut observed = String::new();
    for (label, args) in commands {
        let target_dir = case.dir.join("_build");
        if target_dir.exists() {
            std::fs::remove_dir_all(&target_dir).unwrap();
        }

        let output = moon_process_cmd(&case.dir)
            .args(args)
            .output()
            .expect("moon command should start");
        writeln!(
            observed,
            "{label}: {}",
            if output.status.success() {
                "success"
            } else {
                "failure"
            }
        )
        .unwrap();
    }

    // This is intentionally a behavior snapshot, including known failures.
    // Follow-up fixes should change only the affected line to `success`.
    snapbox::assert_data_eq!(
        observed,
        snapbox::str![[r#"
fmt: success
check: failure
info: failure
build: failure
test: failure
bundle: failure
doc: failure
run (wasm-gc): failure
run (js): failure
test (js): failure
check (native): failure
info (native): failure
build (native): failure
test (native): failure
run (native): failure
"#]],
    );
}
