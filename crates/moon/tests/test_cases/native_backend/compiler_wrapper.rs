use std::os::unix::fs::PermissionsExt;

use crate::{TestDir, moon_cmd};

#[test]
fn c_wrapper_runs_for_each_translation_unit_only() {
    for (fixture, args, generated_source) in [
        (
            "native_backend/compiler_wrapper",
            vec!["build", "--release"],
            "main.c",
        ),
        (
            "native_backend/compiler_wrapper",
            vec!["run", "main"],
            "main.c",
        ),
        (
            "native_backend/compiler_wrapper",
            vec!["test"],
            "main.internal_test.c",
        ),
        (
            "native_foreign_library",
            vec!["build", "--release"],
            "lib.c",
        ),
    ] {
        let dir = TestDir::new(fixture);
        let wrapper = dir.join("cache wrapper");
        let log = dir.join("wrapper.log");
        // Reject probes, link commands, and multi-source compilation, just as
        // a compiler cache needs one translation unit per invocation.
        std::fs::write(
            &wrapper,
            r#"#!/bin/sh
compile=false
source=
for arg in "$@"; do
    case "$arg" in
        -c) compile=true ;;
        *.c)
            if [ -n "$source" ]; then exit 81; fi
            source=$arg ;;
    esac
done
if [ "$compile" != true ] || [ -z "$source" ]; then
    printf 'invalid invocation\n' >> "$WRAPPER_LOG"
    exit 82
fi
printf '%s\n' "$source" >> "$WRAPPER_LOG"
exec "$@"
"#,
        )
        .unwrap();
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
        moon_cmd(&dir)
            .args(&args)
            .args(["--target", "native"])
            .env("MOONBIT_NEW_NATIVE", "0")
            .env("MOON_CC_WRAPPER", &wrapper)
            .env("WRAPPER_LOG", &log)
            .assert()
            .success();
        let calls = std::fs::read_to_string(&log).expect("the compiler wrapper must run");
        assert!(!calls.contains("invalid invocation"), "{calls}");
        assert!(
            calls.lines().any(|line| line.ends_with(generated_source)),
            "{calls}"
        );
        assert!(
            calls.lines().any(|line| line.ends_with("stub.c")),
            "{calls}"
        );
        assert!(
            calls
                .lines()
                .any(|line| line.ends_with("runtime.c") || line.contains("/runtime/")),
            "{calls}"
        );
        if fixture == "native_backend/compiler_wrapper" && args[0] == "build" {
            std::fs::write(
                dir.join("main/stub.c"),
                "int wrapper_test_answer(void) { return 43; }\n",
            )
            .unwrap();
            moon_cmd(&dir)
                .args(["build", "--target", "native", "--release"])
                .env("MOON_CC_WRAPPER", &wrapper)
                .env("MOONBIT_NEW_NATIVE", "0")
                .env("WRAPPER_LOG", &log)
                .assert()
                .success();
            let new_calls = std::fs::read_to_string(&log).unwrap();
            let extra_calls = new_calls
                .strip_prefix(&calls)
                .unwrap()
                .lines()
                .collect::<Vec<_>>();
            assert_eq!(
                extra_calls.len(),
                1,
                "only the changed C stub should recompile: {extra_calls:?}"
            );
            assert!(extra_calls[0].ends_with("stub.c"));
            moon_cmd(&dir)
                .args(["run", "main", "--target", "native", "--release"])
                .env("MOON_CC_WRAPPER", &wrapper)
                .env("MOONBIT_NEW_NATIVE", "0")
                .env("WRAPPER_LOG", &log)
                .assert()
                .success()
                .stdout_eq("43\n");
        }
    }
}

#[test]
fn missing_c_wrapper_is_an_error_only_when_native_tools_are_needed() {
    let dir = TestDir::new("native_backend/compiler_wrapper");
    moon_cmd(&dir)
        .args(["check", "--target", "native"])
        .env("MOON_CC_WRAPPER", "missing-moon-test-wrapper")
        .assert()
        .success();
    moon_cmd(&dir)
        .args(["build", "--target", "native", "--dry-run"])
        .env("MOON_CC_WRAPPER", "missing-moon-test-wrapper")
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
...
[..]failed to resolve C compiler wrapper `missing-moon-test-wrapper` from MOON_CC_WRAPPER[..]
...
"#]]);
    moon_cmd(&dir)
        .args(["build", "--target", "native", "--dry-run"])
        .env("MOON_CC_WRAPPER", "")
        .assert()
        .success();
}
