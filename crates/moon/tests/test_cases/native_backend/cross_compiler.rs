use crate::{TestDir, get_err_stderr_with_envs, get_stdout_with_envs};

/// Prebuilt objects and the flags that require them. They are built for the
/// host, so a compiler for another target must not receive them.
const HOST_ONLY_INPUTS: [&str; 6] = [
    "libmoonbitrun.o",
    "moonbit_simdutf.o",
    "simdutf.o",
    "libbacktrace.a",
    "MOONBIT_USE_SIMDUTF",
    "MOONBIT_ALLOW_STACKTRACE",
];

fn cross_envs(dir: &TestDir) -> Vec<(&'static str, String)> {
    vec![
        ("MOONBIT_NEW_NATIVE", "1".to_string()),
        ("MOON_CC", "fake-cross-gcc".to_string()),
        ("PATH", dir.join("fake-toolchain/bin").display().to_string()),
    ]
}

#[test]
fn test_cross_compiler_does_not_receive_host_only_objects() {
    let dir = TestDir::new("native_backend/cross_compiler");

    for args in [
        &["build", "--target", "native", "--dry-run"][..],
        &["build", "--target", "native", "--release", "--dry-run"],
        &["test", "--target", "native", "--dry-run"],
        &["test", "--target", "native", "--release", "--dry-run"],
    ] {
        let output = get_stdout_with_envs(&dir, args, cross_envs(&dir));
        for input in HOST_ONLY_INPUTS {
            assert!(
                !output.contains(input),
                "`moon {}` passed host-only `{input}` to a cross compiler:\n{output}",
                args.join(" ")
            );
        }
        // Direct object output is emitted for the host.
        assert!(
            !output.contains("__moonbit_link_core__"),
            "`moon {}` used the direct object backend with a cross compiler:\n{output}",
            args.join(" ")
        );
    }
}

#[test]
fn test_cross_compiler_rejects_mimalloc() {
    let dir = TestDir::new("native_backend/cross_compiler");
    let mut envs = cross_envs(&dir);
    envs.push(("MOONBIT_ALLOCATOR", "mimalloc".to_string()));

    let error = get_err_stderr_with_envs(&dir, ["build", "--target", "native", "--dry-run"], envs);
    assert!(
        error.contains("MOONBIT_ALLOCATOR=mimalloc is unavailable"),
        "unexpected allocator error: {error}"
    );
}
