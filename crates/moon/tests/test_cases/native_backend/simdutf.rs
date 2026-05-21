use std::{fs, io, path::Path};

use crate::{TestDir, get_stdout_with_envs, util::toolchain_root_for_tests};

#[derive(Clone, Debug)]
struct PerfResult {
    elapsed_us: f64,
    checksum: u128,
}

fn simdutf_objects_exist(toolchain_root: &Path) -> bool {
    toolchain_root.join("lib/moonbit_simdutf.o").exists()
        && toolchain_root.join("lib/simdutf.o").exists()
}

fn in_ci() -> bool {
    std::env::var_os("CI").is_some()
}

fn available_non_tcc_compilers() -> Vec<(&'static str, &'static str)> {
    let mut compilers = Vec::new();

    for name in ["cc", "clang", "gcc"] {
        if which::which(name).is_ok() {
            compilers.push((name, name));
        }
    }

    compilers
}

fn moon_env(toolchain_root: &Path, cc: &str) -> Vec<(String, String)> {
    vec![
        (
            "MOON_TOOLCHAIN_ROOT".to_string(),
            toolchain_root.display().to_string(),
        ),
        ("MOON_CC".to_string(), cc.to_string()),
    ]
}

fn clean(dir: &TestDir, toolchain_root: &Path, cc: &str) {
    get_stdout_with_envs(dir, ["clean"], moon_env(toolchain_root, cc));
}

fn run_perf(dir: &TestDir, toolchain_root: &Path, cc: &str) -> PerfResult {
    let stdout = get_stdout_with_envs(
        dir,
        ["run", "main", "--target", "native", "--release"],
        moon_env(toolchain_root, cc),
    );
    let lines = stdout.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2, "unexpected simdutf perf output: {stdout:?}");
    let elapsed_us = lines[0]
        .parse::<f64>()
        .unwrap_or_else(|_| panic!("unexpected simdutf perf elapsed time: {stdout:?}"));
    let checksum = lines[1]
        .parse::<u128>()
        .unwrap_or_else(|_| panic!("unexpected simdutf perf checksum: {stdout:?}"));
    assert!(elapsed_us > 0.0, "perf run should report elapsed time");
    assert_ne!(checksum, 0, "perf run should report non-zero checksum");
    PerfResult {
        elapsed_us,
        checksum,
    }
}

fn best_of_three(dir: &TestDir, toolchain_root: &Path, cc: &str) -> PerfResult {
    clean(dir, toolchain_root, cc);
    (0..3)
        .map(|_| run_perf(dir, toolchain_root, cc))
        .min_by(|left, right| left.elapsed_us.total_cmp(&right.elapsed_us))
        .expect("at least one perf sample")
}

fn is_simdutf_object(root: &Path, path: &Path) -> bool {
    let Ok(relative_path) = path.strip_prefix(root) else {
        return false;
    };
    relative_path == Path::new("lib").join("moonbit_simdutf.o")
        || relative_path == Path::new("lib").join("simdutf.o")
}

fn hard_link_or_copy_file(src: &Path, dest: &Path) -> io::Result<()> {
    fs::hard_link(src, dest).or_else(|_| fs::copy(src, dest).map(|_| ()))
}

fn copy_tree_without_simdutf_objects(src: &Path, dest: &Path, root: &Path) -> io::Result<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        if is_simdutf_object(root, &src_path) {
            continue;
        }

        let dest_path = dest.join(entry.file_name());
        if fs::metadata(&src_path)?.is_dir() {
            copy_tree_without_simdutf_objects(&src_path, &dest_path, root)?;
        } else {
            hard_link_or_copy_file(&src_path, &dest_path)?;
        }
    }
    Ok(())
}

fn scalar_toolchain_root(source_root: &Path) -> tempfile::TempDir {
    let temp_dir = tempfile::TempDir::new().expect("create scalar toolchain root");
    copy_tree_without_simdutf_objects(source_root, temp_dir.path(), source_root)
        .expect("copy scalar toolchain root");
    temp_dir
}

#[test]
fn test_native_backend_simdutf_links_with_available_non_tcc_compilers() {
    let toolchain_root = toolchain_root_for_tests();
    if !simdutf_objects_exist(&toolchain_root) {
        assert!(
            !in_ci(),
            "CI toolchain should provide moonbit_simdutf.o and simdutf.o"
        );
        eprintln!("skipping simdutf link test: toolchain does not provide simdutf objects");
        return;
    }

    let compilers = available_non_tcc_compilers();
    if compilers.is_empty() {
        assert!(
            !in_ci(),
            "CI should provide at least one non-tcc C compiler"
        );
        eprintln!("skipping simdutf link test: no non-tcc C compiler found");
        return;
    }

    for (name, cc) in compilers {
        let dir = TestDir::new("native_backend/simdutf_conversion");
        clean(&dir, &toolchain_root, cc);
        let stdout = get_stdout_with_envs(
            &dir,
            ["run", "main", "--target", "native", "--release"],
            moon_env(&toolchain_root, cc),
        );
        assert_eq!(
            stdout, "6\n6\nA©中😀B\n11\n11\nb'/x41'\nb'/x42'\n-2\nok\n",
            "simdutf native executable should run UTF conversion after linking with {name}"
        );
    }
}

#[test]
#[ignore = "performance-sensitive check for optimized toolchain simdutf objects"]
fn test_native_backend_simdutf_is_faster_than_scalar_runtime() {
    let toolchain_root = toolchain_root_for_tests();
    if !simdutf_objects_exist(&toolchain_root) {
        assert!(
            !in_ci(),
            "CI toolchain should provide moonbit_simdutf.o and simdutf.o"
        );
        eprintln!("skipping simdutf performance test: toolchain does not provide simdutf objects");
        return;
    }

    let Some((name, cc)) = available_non_tcc_compilers().into_iter().next() else {
        assert!(
            !in_ci(),
            "CI should provide at least one non-tcc C compiler"
        );
        eprintln!("skipping simdutf performance test: no non-tcc C compiler found");
        return;
    };

    let scalar_toolchain = scalar_toolchain_root(&toolchain_root);
    assert!(
        !simdutf_objects_exist(scalar_toolchain.path()),
        "scalar baseline toolchain should not contain simdutf objects"
    );

    let scalar_dir = TestDir::new("native_backend/simdutf_perf");
    let simdutf_dir = TestDir::new("native_backend/simdutf_perf");
    let scalar = best_of_three(&scalar_dir, scalar_toolchain.path(), cc);
    let simdutf = best_of_three(&simdutf_dir, &toolchain_root, cc);

    assert_eq!(
        scalar.checksum, simdutf.checksum,
        "scalar and simdutf runs should do the same work"
    );
    assert!(
        simdutf.elapsed_us * 100.0 < scalar.elapsed_us * 90.0,
        "expected simdutf runtime to be at least 10% faster with {name}; scalar={}us, simdutf={}us",
        scalar.elapsed_us,
        simdutf.elapsed_us,
    );
}
