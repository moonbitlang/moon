use crate::{TestDir, moon_cmd};

fn assert_integer_semantics(compiler: &str) {
    let is_cl = compiler == "cl";
    #[cfg(windows)]
    let toolchain = moonutil::compiler_flags::resolve_windows_msvc_toolchain()
        .expect("integer semantics tests require MSVC Build Tools and the Windows SDK");
    #[cfg(windows)]
    let compiler = if compiler == "cl" {
        std::path::PathBuf::from(toolchain.cc().cc_path())
    } else {
        let Ok(path) = which::which(compiler) else {
            assert!(std::env::var_os("CI").is_none(), "{compiler} missing in CI");
            eprintln!("skipping integer semantics test: {compiler} is unavailable");
            return;
        };
        path
    };
    #[cfg(not(windows))]
    let compiler = std::path::PathBuf::from(compiler);

    #[cfg(windows)]
    let environment = toolchain
        .msvc_environment()
        .expect("discovered MSVC environment")
        .command_env();

    // Record the actual compiler version with any failure. cl prints its banner
    // and exits unsuccessfully without an input; Clang accepts --version.
    let mut version = std::process::Command::new(&compiler);
    #[cfg(windows)]
    version.envs(environment.iter().cloned());
    if !is_cl {
        version.arg("--version");
    }
    let version = version.output().expect("query C compiler version");
    eprintln!(
        "{}{}",
        String::from_utf8_lossy(&version.stdout),
        String::from_utf8_lossy(&version.stderr)
    );

    for profile in ["--debug", "--release"] {
        // Each compiler/profile gets a fresh build, without sharing native
        // artifacts. Force generated C even for the debug profile.
        let dir = TestDir::new("native_backend/integer_semantics");
        let command = moon_cmd(&dir);
        #[cfg(windows)]
        let command = command.envs(environment.iter().cloned());
        eprintln!("checking {compiler:?} {profile}");
        command
            .env("MOON_CC", &compiler)
            .env_remove("MOON_AR")
            .env("MOONBIT_NEW_NATIVE", "0")
            .args(["run", "main", "--target", "native", profile, "--"])
            .args([
                "2147483647",
                "-2147483648",
                "9223372036854775807",
                "-9223372036854775808",
            ])
            .assert()
            .success()
            .stdout_eq(snapbox::str![[r#"
-2147483648
2147483647
-2
false
false
false
-9223372036854775808
9223372036854775807
-2
false
false
false

"#]]);
    }
}

#[test]
#[cfg(not(windows))]
fn system_cc_preserves_integer_wrapping() {
    assert_integer_semantics("cc");
}

#[test]
#[cfg(windows)]
fn msvc_preserves_integer_wrapping() {
    assert_integer_semantics("cl");
}

#[test]
#[cfg(windows)]
fn clang_preserves_integer_wrapping() {
    assert_integer_semantics("clang");
}

#[test]
#[cfg(windows)]
fn clang_cl_preserves_integer_wrapping() {
    assert_integer_semantics("clang-cl");
}
