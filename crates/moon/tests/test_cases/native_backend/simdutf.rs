use std::path::Path;

use crate::{TestDir, get_stdout_with_envs, util::toolchain_root_for_tests};

fn simdutf_objects_exist(toolchain_root: &Path) -> bool {
    toolchain_root.join("lib/moonbit_simdutf.o").exists()
        && toolchain_root.join("lib/simdutf.o").exists()
}

fn available_non_tcc_compilers() -> Vec<(&'static str, &'static str)> {
    let mut compilers = Vec::new();

    for name in ["cc", "clang", "gcc"] {
        if which::which(name).is_ok() {
            compilers.push((name, name));
        }
    }

    compilers
}

#[test]
fn test_native_backend_simdutf_links_with_available_non_tcc_compilers() {
    let toolchain_root = toolchain_root_for_tests();
    if !simdutf_objects_exist(&toolchain_root) {
        eprintln!("skipping simdutf link test: toolchain does not provide simdutf objects");
        return;
    }

    let compilers = available_non_tcc_compilers();
    if compilers.is_empty() {
        eprintln!("skipping simdutf link test: no non-tcc C compiler found");
        return;
    }

    for (name, cc) in compilers {
        let dir = TestDir::new("native_backend/simdutf_conversion");
        let envs = vec![
            (
                "MOON_TOOLCHAIN_ROOT".to_string(),
                toolchain_root.display().to_string(),
            ),
            ("MOON_CC".to_string(), cc.to_string()),
        ];
        get_stdout_with_envs(&dir, ["clean"], envs.clone());
        let stdout = get_stdout_with_envs(
            &dir,
            ["run", "main", "--target", "native", "--release"],
            envs,
        );
        assert_eq!(
            stdout, "6\n6\nA©中😀B\n11\n11\nb'/x41'\nb'/x42'\n-2\nok\n",
            "simdutf native executable should run UTF conversion after linking with {name}"
        );
    }
}
