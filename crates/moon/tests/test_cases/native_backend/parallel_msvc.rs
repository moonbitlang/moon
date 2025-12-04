#[cfg(windows)]
use crate::{TestDir, get_stdout, util::copy};

/// This test is probabilistic. It ensures that MSVC builds don't share an
/// intermediate directory when run in parallel, which can cause conflicts.
#[test]
#[cfg(windows)]
fn test_parallel_msvc() {
    let dir = TestDir::new("native_backend/parallel_msvc");
    let template = dir.join("template");
    // Copy it multiple times while changing the package name
    for i in 0..30 {
        let pkg_name = format!("parallel_msvc_{}", i);
        let pkg_dir = dir.join(&pkg_name);
        copy(&template, &pkg_dir).unwrap();
    }
    // Run `moon test --release` to trigger the builds
    get_stdout(&dir, ["test", "--release"]);
}
