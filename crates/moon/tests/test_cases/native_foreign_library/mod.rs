//! A foreign-library package builds into a static archive on the Native
//! backend. A C host links that archive and only calls `moonbit_init` before
//! using the exports.
//!
//! `moon build` prints the archive path.
//!
//! The package and its dependency both name their C stub `stub.c`, so the
//! archive holds two `stub.o` members that every archiver must keep.

use std::path::Path;
use std::process::Command;

use crate::{TestDir, get_stdout, util::toolchain_root_for_tests};

#[cfg(unix)]
const ARCHIVE_EXTENSION: &str = ".a";
#[cfg(windows)]
const ARCHIVE_EXTENSION: &str = ".lib";

/// Link `host.c` against the archive with the host's C compiler.
#[cfg(unix)]
fn link_c_host(dir: &TestDir, archive: &Path, profile: &str) -> Command {
    let toolchain_root = toolchain_root_for_tests();
    let mut link = Command::new(which::which("cc").expect("C compiler should be available"));
    link.arg("-I")
        .arg(toolchain_root.join("include"))
        .arg(dir.join("host.c"))
        .arg(archive);
    if profile == "debug" {
        // TODO: Drop once moon reports the link requirements of a foreign
        // library; the debug runtime reports backtraces with libbacktrace.
        link.arg(toolchain_root.join("lib/libbacktrace.a"));
    }
    link.args(["-lm", "-o"])
        .arg(dir.join(format!("host-{profile}")));
    link
}

/// Link `host.c` against the archive with the MSVC toolchain moon selects,
/// using the static CRT that every archive member was compiled with.
#[cfg(windows)]
fn link_c_host(dir: &TestDir, archive: &Path, profile: &str) -> Command {
    use moonutil::compiler_flags::{
        WINDOWS_MSVC_DEFAULT_LIBS, WINDOWS_MSVC_STATIC_RUNTIME_FLAG, resolve_windows_msvc_toolchain,
    };

    let toolchain = resolve_windows_msvc_toolchain().expect("MSVC Build Tools should be available");
    let mut link = Command::new(toolchain.cc_command_path());
    if let Some(environment) = toolchain.msvc_environment() {
        link.envs(environment.command_env().iter().cloned());
    }
    // TODO: Drop the CRT flag and system libraries once moon reports the link
    // requirements of a foreign library.
    link.current_dir(dir)
        .args(["/nologo", WINDOWS_MSVC_STATIC_RUNTIME_FLAG])
        .arg(format!(
            "/I{}",
            toolchain_root_for_tests().join("include").display()
        ))
        .arg(dir.join("host.c"))
        .arg(archive)
        .arg(format!(
            "/Fe{}",
            dir.join(format!("host-{profile}.exe")).display()
        ))
        .args(["/link", "/nologo", "/subsystem:console"])
        .args(WINDOWS_MSVC_DEFAULT_LIBS);
    link
}

#[test]
fn test_native_foreign_library_links_into_c_host() {
    let dir = TestDir::new("native_foreign_library");

    // Release builds compile generated C into the payload object; debug builds
    // use direct object output where the host supports it.
    for (profile, profile_args) in [("release", &["--release"][..]), ("debug", &[])] {
        // `--quiet` hides the build summary but not the built paths.
        let stdout = get_stdout(
            &dir,
            ["build", "--target", "native", "--quiet"]
                .iter()
                .chain(profile_args.iter()),
        );
        assert_eq!(
            stdout.replace('\\', "/"),
            format!("$ROOT/_build/native/{profile}/build/lib/lib{ARCHIVE_EXTENSION}\n"),
            "moon build should print the archive path"
        );

        let package_dir = dir.join(format!("_build/native/{profile}/build/lib"));
        assert!(
            !package_dir.join("lib.exe").exists(),
            "a foreign library must not link an executable"
        );
        let archive = package_dir.join(format!("lib{ARCHIVE_EXTENSION}"));
        assert!(
            archive.is_file(),
            "the foreign library should be archived at {}",
            archive.display()
        );

        let link_output = link_c_host(&dir, &archive, profile)
            .output()
            .expect("failed to run the C compiler");
        assert!(
            link_output.status.success(),
            "linking the {profile} foreign library failed:\n{}\n{}",
            String::from_utf8_lossy(&link_output.stdout),
            String::from_utf8_lossy(&link_output.stderr)
        );

        let host = dir.join(format!("host-{profile}{}", std::env::consts::EXE_SUFFIX));
        let run_output = Command::new(&host)
            .output()
            .expect("failed to run the C host");
        assert!(
            run_output.status.success(),
            "the {profile} C host failed:\n{}",
            String::from_utf8_lossy(&run_output.stderr)
        );
        // `add` runs both C stubs and reads a value set by `moonbit_init`.
        // The Windows CRT writes CRLF when stdout is captured by a pipe.
        assert_eq!(
            String::from_utf8_lossy(&run_output.stdout).replace("\r\n", "\n"),
            "add(40, 2) = 86\n",
            "unexpected {profile} host output"
        );
    }
}
