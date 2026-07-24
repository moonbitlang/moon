use std::io::Write;

use super::*;

#[test]
fn test_build_package_tracks_dependency_core_artifact() {
    let dir = TestDir::new("build_package_dep_core/build_package_dep_core.in");

    let first_build = get_stdout(&dir, ["build", "--target", "wasm-gc"]);
    assert!(
        first_build.contains("Finished. moon: ran 3 tasks, now up to date"),
        "initial build should compile lib, main, and linked core; got:\n{first_build}"
    );

    let lib_core = dir.join("_build/wasm-gc/debug/build/lib/lib.core");
    assert!(lib_core.exists(), "lib core artifact should exist");

    // Ensure the edited dependency `.core` is newer than the downstream output
    // even on filesystems with coarse timestamp granularity.
    std::thread::sleep(std::time::Duration::from_secs(1));
    let mut lib_core_file = std::fs::OpenOptions::new()
        .append(true)
        .open(&lib_core)
        .expect("lib core should be writable");
    lib_core_file
        .write_all(b"\n")
        .expect("lib core edit should succeed");
    drop(lib_core_file);

    let second_build = get_stdout(&dir, ["build", "--target", "wasm-gc"]);
    check(
        second_build,
        expect![[r#"
            Finished. moon: ran 3 tasks, now up to date
        "#]],
    );
}
