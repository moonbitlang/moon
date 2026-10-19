mod cc_flags;
#[cfg(unix)]
mod compiler_wrapper;
#[cfg(unix)]
mod cross_compiler;
mod integer_semantics;
#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "windows", target_arch = "x86_64")
))]
mod new_native_e2e;
#[cfg(unix)]
mod no_compiler_probe;
#[cfg(windows)]
mod parallel_msvc;
#[cfg(unix)]
mod simdutf;
#[cfg(unix)]
mod system_cc_fallback;
mod test_filter;

#[cfg(unix)]
mod unix_graph {
    use expect_test::ExpectFile;
    use std::path::Path;

    use crate::{TestDir, build_graph, moon_cmd};

    #[track_caller]
    pub(super) fn assert_native_backend_graph(
        dir: &TestDir,
        args: &[&str],
        envs: &[(&str, &str)],
        expected: ExpectFile,
    ) {
        let uses_host_archiver = !envs
            .iter()
            .any(|(name, _)| matches!(*name, "MOON_CC" | "MOON_AR"));
        // The transform visits every command, input, and output in the graph.
        // Resolve the SDK once per assertion using the test process environment.
        let sdk_link_flag = macos_sdk_path().map(|root| format!("-L{root}/usr/lib"));
        build_graph::assert_with_replacements(
            moon_cmd(dir).args(args).envs(envs.iter().copied()),
            expected,
            |s| {
                // Normalize clang-only warnings to keep snapshots portable across macOS/Linux.
                *s = s.replace(" -Wno-unused-value", "");
                *s = s.replace(".dylib", ".so");
                if uses_host_archiver {
                    crate::util::normalize_host_archiver(s);
                }
                if let Some(flag) = &sdk_link_flag {
                    *s = s.replace(flag, "-L$MACOSX_SDK/usr/lib");
                }
                normalize_fake_toolchain_path(s, dir);
            },
        );
    }

    pub(super) fn prepend_to_path(path: &Path) -> String {
        std::env::join_paths(
            std::iter::once(path.to_path_buf()).chain(
                std::env::var_os("PATH")
                    .as_deref()
                    .into_iter()
                    .flat_map(std::env::split_paths),
            ),
        )
        .expect("prepend native toolchain fixture to PATH")
        .to_string_lossy()
        .into_owned()
    }

    #[cfg(target_os = "macos")]
    fn macos_sdk_path() -> Option<String> {
        let output = std::process::Command::new("xcrun")
            .args(["--sdk", "macosx", "--show-sdk-path"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }

        let sdk_root = String::from_utf8_lossy(&output.stdout);
        let sdk_root = sdk_root.lines().next()?.trim();
        (!sdk_root.is_empty()).then(|| sdk_root.to_owned())
    }

    #[cfg(not(target_os = "macos"))]
    fn macos_sdk_path() -> Option<String> {
        None
    }

    fn normalize_fake_toolchain_path(s: &mut String, dir: &TestDir) {
        let root = dir.join("fake-toolchain");
        let raw = root.to_string_lossy();
        *s = s.replace(raw.as_ref(), "$FAKE_TOOLCHAIN");
        *s = s.replace(raw.replace('\\', "/").as_str(), "$FAKE_TOOLCHAIN");
        *s = s.replace("./fake-toolchain", "$FAKE_TOOLCHAIN");
        *s = s.replace(".\\fake-toolchain", "$FAKE_TOOLCHAIN");

        if let Ok(root) = dunce::canonicalize(root) {
            let canonical = root.to_string_lossy();
            *s = s.replace(canonical.as_ref(), "$FAKE_TOOLCHAIN");
            *s = s.replace(canonical.replace('\\', "/").as_str(), "$FAKE_TOOLCHAIN");
        }
    }
}
