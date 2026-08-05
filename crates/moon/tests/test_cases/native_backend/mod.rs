mod cc_flags;
#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
mod new_native_e2e;
#[cfg(windows)]
mod parallel_msvc;
#[cfg(unix)]
mod simdutf;
#[cfg(unix)]
mod tcc_run;
mod test_filter;

#[cfg(unix)]
mod unix_graph {
    use expect_test::ExpectFile;
    use moonbuild_debug::graph::ENV_VAR;
    use std::path::Path;

    use crate::{TestDir, build_graph::compare_graphs_with_replacements, get_stdout_with_envs};

    #[track_caller]
    pub(super) fn assert_native_backend_graph(
        dir: &TestDir,
        tmp_name: &str,
        args: &[&str],
        envs: &[(&str, &str)],
        expected: ExpectFile,
    ) {
        let graph = dir.join(tmp_name);
        let mut env_pairs: Vec<(String, String)> = envs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect();
        env_pairs.push((ENV_VAR.to_string(), graph.to_string_lossy().into_owned()));
        get_stdout_with_envs(dir, args.iter().copied(), env_pairs);
        let uses_host_archiver = !envs
            .iter()
            .any(|(name, _)| matches!(*name, "MOON_CC" | "MOON_AR"));
        compare_graphs_with_replacements(&graph, expected, |s| {
            // Normalize clang-only warnings to keep snapshots portable across macOS/Linux.
            *s = s.replace(" -Wno-unused-value", "");
            *s = s.replace(".dylib", ".so");
            if uses_host_archiver {
                crate::util::normalize_host_archiver(s);
            } else {
                crate::util::normalize_archive_fingerprints(s);
            }
            normalize_macos_sdk_path(s);
            normalize_fake_toolchain_path(s, dir);
        });
    }

    #[track_caller]
    pub(super) fn assert_native_backend_graph_no_env(
        dir: &TestDir,
        tmp_name: &str,
        args: &[&str],
        expected: ExpectFile,
    ) {
        assert_native_backend_graph(dir, tmp_name, args, &[], expected);
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
    fn normalize_macos_sdk_path(s: &mut String) {
        let Ok(output) = std::process::Command::new("xcrun")
            .args(["--sdk", "macosx", "--show-sdk-path"])
            .output()
        else {
            return;
        };
        if !output.status.success() {
            return;
        }

        let sdk_root = String::from_utf8_lossy(&output.stdout);
        let Some(sdk_root) = sdk_root.lines().next().map(str::trim) else {
            return;
        };
        if sdk_root.is_empty() {
            return;
        }

        *s = s.replace(&format!("-L{sdk_root}/usr/lib"), "-L$MACOSX_SDK/usr/lib");
    }

    #[cfg(not(target_os = "macos"))]
    fn normalize_macos_sdk_path(_s: &mut String) {}

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
