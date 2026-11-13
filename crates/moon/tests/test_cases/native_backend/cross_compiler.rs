use crate::{TestDir, get_err_stderr_with_envs, get_stdout_with_envs, moon_cmd};

#[test]
fn package_compilers_select_compatible_payload() {
    for use_dependency_stub in [false, true] {
        let dir = TestDir::new("native_backend/cross_compiler");
        let compiler = dir.join("fake-toolchain/bin/fake-cross-gcc");
        if use_dependency_stub {
            std::fs::create_dir(dir.join("stub_dep")).unwrap();
            std::fs::write(dir.join("stub_dep/stub.c"), "int stub(void) { return 0; }").unwrap();
            std::fs::write(
                dir.join("stub_dep/moon.pkg"),
                format!(r#"options("native-stub": ["stub.c"], link: {{ "native": {{ "stub-cc": "{}" }} }})"#, compiler.display()),
            ).unwrap();
            std::fs::write(
                dir.join("main/moon.pkg"),
                "import { \"cross_compiler/stub_dep\" }\noptions(\"is-main\": true)",
            )
            .unwrap();
        } else {
            std::fs::write(
                dir.join("main/moon.pkg"),
                format!(
                    r#"options("is-main": true, link: {{ "native": {{ "cc": "{}" }} }})"#,
                    compiler.display(),
                ),
            )
            .unwrap();
        }
        // An unused package must not contribute a compiler request.
        std::fs::create_dir(dir.join("unused")).unwrap();
        std::fs::write(dir.join("unused/stub.c"), "").unwrap();
        std::fs::write(
            dir.join("unused/moon.pkg"),
            r#"options("native-stub": ["stub.c"], link: { "native": { "stub-cc": "moon-missing-gcc" }})"#,
        ).unwrap();
        moon_cmd(&dir)
            .env_remove("MOON_CC")
            .env_remove("MOON_AR")
            .env("MOONBIT_NEW_NATIVE", "1")
            .args([
                "build",
                "main",
                "--target",
                "native",
                "--debug",
                "--dry-run",
            ])
            .assert()
            .success()
            .stdout_eq(snapbox::str![[r#"
...
[..]moonc link-core[..] -o [..].c [..]
...
"#]]);
    }
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
