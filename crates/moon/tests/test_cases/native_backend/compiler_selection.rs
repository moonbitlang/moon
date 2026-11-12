use crate::{TestDir, moon_cmd};

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
