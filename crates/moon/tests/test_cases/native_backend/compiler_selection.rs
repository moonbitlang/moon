use crate::{TestDir, moon_cmd};

#[test]
fn missing_compiler_override_is_not_replaced_by_msvc() {
    let dir = TestDir::new("native_backend/compiler_selection");
    moon_cmd(&dir)
        .env("MOON_CC", "moon-missing-gcc")
        .env("MOONBIT_NEW_NATIVE", "1")
        .args(["build", "--target", "native", "--debug", "--dry-run"])
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
...
[..]native compiler executable `moon-missing-gcc`[..]
...
"#]]);
}

#[test]
fn plain_clang_overrides_select_generated_c() {
    if which::which("clang").is_err() {
        assert!(std::env::var_os("CI").is_none(), "clang missing in CI");
        eprintln!("skipping compiler selection test: clang is unavailable");
        return;
    }
    let sdk = moonutil::compiler_flags::resolve_windows_msvc_toolchain().unwrap();
    for request in ["environment", "package", "stub"] {
        let dir = TestDir::new("native_backend/compiler_selection");
        // The existing stub recognizes Clang targeting MSVC with either CLI.
        if request != "environment" {
            let options = if request == "package" {
                r#""cc": "clang", "stub-cc": "clang""#
            } else {
                r#""stub-cc": "clang""#
            };
            std::fs::write(dir.join("main/moon.pkg"), format!(
                r#"options("is-main": true, "native-stub": ["stub.c"], link: {{ "native": {{ {options} }} }})"#,
            )).unwrap();
        }
        let command = moon_cmd(&dir)
            .envs(
                sdk.msvc_environment()
                    .unwrap()
                    .command_env()
                    .iter()
                    .cloned(),
            )
            .env_remove("MOON_CC")
            .env_remove("MOON_AR")
            .env_remove("MOON_CC_WRAPPER")
            .env("MOONBIT_NEW_NATIVE", "1");
        let command = if request == "environment" {
            command.env("MOON_CC", "clang")
        } else {
            command
        };
        command
            .args(["run", "main", "--target", "native", "--debug"])
            .assert()
            .success()
            .stdout_eq("1\n");
        assert!(dir.join("_build/native/debug/build/main/main.c").is_file());
    }
}

#[test]
fn bare_clang_cl_overrides_run_the_requested_compiler() {
    if which::which("clang-cl").is_err() {
        assert!(std::env::var_os("CI").is_none(), "clang-cl missing in CI");
        eprintln!("skipping compiler selection test: clang-cl is unavailable");
        return;
    }

    for compiler in ["clang-cl", "clang-cl.exe"] {
        for use_env_override in [true, false] {
            // Debug exercises direct object linking; release exercises generated C.
            // A C stub reports its actual compiler, so silently substituting cl.exe
            // fails even if the resulting program otherwise builds and runs.
            for profile in ["--debug", "--release"] {
                let dir = TestDir::new("native_backend/compiler_selection");
                let command = moon_cmd(&dir)
                    .env_remove("MOON_AR")
                    .env_remove("MOON_CC_WRAPPER")
                    .env("MOONBIT_NEW_NATIVE", "1");
                let command = if use_env_override {
                    command.env("MOON_CC", compiler)
                } else {
                    std::fs::write(
                        dir.join("main/moon.pkg"),
                        format!(
                            r#"options(
  "is-main": true,
  "native-stub": ["stub.c"],
  link: {{ "native": {{ "cc": "{compiler}", "stub-cc": "{compiler}" }} }},
)
"#
                        ),
                    )
                    .unwrap();
                    command.env_remove("MOON_CC")
                };
                eprintln!("checking {compiler} {profile}, env override: {use_env_override}");
                command
                    .args(["run", "main", "--target", "native", profile])
                    .assert()
                    .success()
                    .stdout_eq("1\n");
            }
        }
    }
}
