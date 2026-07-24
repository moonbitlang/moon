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
        compare_graphs_with_replacements(&graph, expected, |s| {
            // Normalize clang-only warnings to keep snapshots portable across macOS/Linux.
            *s = s.replace(" -Wno-unused-value", "");
            *s = s.replace(".dylib", ".so");
            normalize_macos_sdk_path(s);
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
}
