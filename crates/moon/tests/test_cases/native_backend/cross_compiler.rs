use crate::{TestDir, get_err_stderr_with_envs, get_stdout_with_envs, moon_cmd};
use snapbox::IntoData;

// Exercise automatic Native payload selection, with fixture compilers on PATH.
fn native_debug_build_dry_run(dir: &TestDir) -> snapbox::cmd::Command {
    moon_cmd(dir)
        .env_remove("MOON_CC")
        .env_remove("MOON_AR")
        .env_remove("MOON_CC_WRAPPER")
        .env(
            "PATH",
            super::unix_graph::prepend_to_path(&dir.join("fake-toolchain/bin")),
        )
        .env("MOONBIT_NEW_NATIVE", "1")
        .args(["build", "--target", "native", "--debug", "--dry-run"])
}

// Each program uses its own compiler for both C compilation and linking.
// The first program's C stub uses a third compiler.
#[test]
fn independent_programs_and_stubs_keep_their_compiler_choices() {
    let dir = TestDir::new("native_backend/compiler_roles");
    let bin = dir.join("fake-toolchain/bin");
    for name in ["first", "second", "stub"] {
        std::fs::copy(bin.join("fake-gcc"), bin.join(format!("{name}-gcc"))).unwrap();
        std::fs::copy(bin.join("fake-ar"), bin.join(format!("{name}-ar"))).unwrap();
    }
    native_debug_build_dry_run(&dir)
        .assert()
        .success()
        .stdout_eq(
            snapbox::str![[r#"
[..]/first-gcc -o [..]/first.o [..] -c [..] [..]/first/first.c
[..]/first-gcc -o [..]/first/first.exe [..]
[..]/second-gcc -o [..]/second.o [..] -c [..] [..]/second/second.c
[..]/second-gcc -o [..]/second/second.exe [..]
[..]/stub-gcc -o [..]/first/stub.o [..] -c [..] [..]/first/stub.c
...
"#]]
            .unordered(),
        );
}

#[test]
fn package_cross_compiler_requires_generated_c() {
    let dir = TestDir::new("native_backend/cross_compiler");
    std::fs::write(
        dir.join("main/moon.pkg"),
        r#"options("is-main": true, link: { "native": { "cc": "fake-cross-gcc" }})"#,
    )
    .unwrap();

    native_debug_build_dry_run(&dir)
        .args(["main"])
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
...
[..]moonc link-core[..] -o [..]/main/main.c [..]
...
"#]]);
}

#[test]
fn dependency_stub_cross_compiler_requires_generated_c() {
    let dir = TestDir::new("native_backend/cross_compiler");
    // main has no compiler override; only the imported stub_dep does.
    std::fs::write(
        dir.join("main/moon.pkg"),
        "import { \"cross_compiler/stub_dep\" }\noptions(\"is-main\": true)",
    )
    .unwrap();

    native_debug_build_dry_run(&dir)
        .args(["main"])
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
...
[..]moonc link-core[..] -o [..]/main/main.c [..]
...
"#]]);
}

#[test]
fn unused_package_does_not_require_its_compiler() {
    // The unused package names a nonexistent stub compiler. main imports neither
    // it nor stub_dep, so this build needs only the ordinary host compiler.
    let dir = TestDir::new("native_backend/cross_compiler");
    native_debug_build_dry_run(&dir)
        .args(["main"])
        .assert()
        .success();
}

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
            !output
                .lines()
                .any(|line| line.contains("moonc link-core")
                    && line.contains("__moonbit_link_core__")),
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
