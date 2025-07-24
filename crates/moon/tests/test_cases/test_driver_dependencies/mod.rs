use std::path::Path;

use indexmap::IndexMap;
use moonutil::common::{FileName, MbtTestInfo, MooncGenTestInfo};

use crate::{get_stdout, TestDir};

fn total_test_count(gen_test_section: &IndexMap<FileName, Vec<MbtTestInfo>>) -> usize {
    gen_test_section.iter().map(|x| x.1.len()).sum()
}

#[test]
fn test_driver_no_args() {
    let dir = TestDir::new("test_driver_dependencies");
    let test_file = dir.join("test_no_args.mbt");

    let (output, meta) = try_gen_output(dir.path.path(), &test_file, false);
    assert!(
        !output.contains("@moonbitlang/core/test"),
        "No args test driver should not contain test dependencies. Full output:\n{output}"
    );
    assert!(
        !output.contains("@moonbitlang/core/bench"),
        "No args test driver should not contain bench dependencies. Full output:\n{output}"
    );
    assert_eq!(
        total_test_count(&meta.no_args_tests),
        1,
        "Expected 1 no args test"
    );
    assert_eq!(
        total_test_count(&meta.with_args_tests),
        0,
        "Expected 0 with args tests"
    );
    assert_eq!(
        total_test_count(&meta.with_bench_args_tests),
        0,
        "Expected 0 with bench args tests"
    );
}

#[test]
fn test_driver_with_args() {
    let dir = TestDir::new("test_driver_dependencies");
    let test_file = dir.join("test_with_args.mbt");

    let (output, meta) = try_gen_output(dir.path.path(), &test_file, false);
    assert!(
        output.contains("@moonbitlang/core/test"),
        "With args test driver should contain test dependencies. Full output:\n{output}"
    );
    assert!(
        !output.contains("@moonbitlang/core/bench"),
        "With args test driver should not contain bench dependencies. Full output:\n{output}"
    );
    assert_eq!(
        total_test_count(&meta.no_args_tests),
        0,
        "Expected 0 no args tests"
    );
    assert_eq!(
        total_test_count(&meta.with_args_tests),
        1,
        "Expected 1 with args test"
    );
    assert_eq!(
        total_test_count(&meta.with_bench_args_tests),
        0,
        "Expected 0 with bench args tests"
    );
}

#[test]
fn test_driver_with_bench_args() {
    let dir = TestDir::new("test_driver_dependencies");
    let bench_file = dir.join("bench_with_args.mbt");

    let (output, meta) = try_gen_output(dir.path.path(), &bench_file, true);
    assert!(
        !output.contains("@moonbitlang/core/test"),
        "With args bench driver should not contain test dependencies. Full output:\n{output}"
    );
    assert!(
        output.contains("@moonbitlang/core/bench"),
        "With args bench driver should contain bench dependencies. Full output:\n{output}"
    );
    assert_eq!(
        total_test_count(&meta.no_args_tests),
        0,
        "Expected 0 no args tests"
    );
    assert_eq!(
        total_test_count(&meta.with_args_tests),
        0,
        "Expected 0 with args tests"
    );
    assert_eq!(
        total_test_count(&meta.with_bench_args_tests),
        1,
        "Expected 1 with bench args test"
    );
}

#[test]
fn test_driver_with_no_args_and_bench() {
    let dir = TestDir::new("test_driver_dependencies");
    let bench_file = dir.join("no_args_and_bench.mbt");

    let (output, meta) = try_gen_output(dir.path.path(), &bench_file, false);
    assert!(
        !output.contains("@moonbitlang/core/test"),
        "With args bench driver should not contain test dependencies. Full output:\n{output}"
    );
    assert!(
        !output.contains("@moonbitlang/core/bench"),
        "With args bench driver should contain bench dependencies. Full output:\n{output}"
    );
    assert_eq!(
        total_test_count(&meta.no_args_tests),
        1,
        "Expected 0 no args tests"
    );
    assert_eq!(
        total_test_count(&meta.with_args_tests),
        0,
        "Expected 0 with args tests"
    );
    assert_eq!(
        total_test_count(&meta.with_bench_args_tests),
        1,
        "Expected 1 with bench args test"
    );
}

fn try_gen_output(dir: &Path, file: &Path, bench: bool) -> (String, MooncGenTestInfo) {
    let file_meta_output = file.with_extension("json");
    let file_driver_output = file.with_extension("driver.mbt");

    get_stdout(
        &dir,
        [
            "generate-test-driver".as_ref(),
            "--output-metadata".as_ref(),
            file_meta_output.as_os_str(),
            "--output-driver".as_ref(),
            file_driver_output.as_os_str(),
            "--target=wasm-gc".as_ref(),
            "--pkg-name=test".as_ref(),
            "--driver-kind=blackbox".as_ref(),
            file.as_os_str(),
        ]
        .into_iter()
        .chain(if bench {
            Some("--bench".as_ref())
        } else {
            None
        }),
    );

    let meta_output =
        std::fs::read_to_string(&file_meta_output).expect("Failed to read metadata output");
    let meta_output = serde_json::from_str(&meta_output).expect("Failed to deserialize metadata");

    println!("{meta_output:?}");
    let driver_output =
        std::fs::read_to_string(&file_driver_output).expect("Failed to read driver output");
    (driver_output, meta_output)
}
