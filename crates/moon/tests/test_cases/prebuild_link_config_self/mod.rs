use std::cell::OnceCell;

use crate::{TestDir, get_stdout_with_envs};

#[test]
fn test_prebuild_link_config_self() {
    let dir = TestDir::new("prebuild_link_config_self/prebuild_link_config_self.in");
    let cc = if cfg!(windows) { "cl" } else { "cc" };
    let build_stdout = get_stdout_with_envs(
        &dir,
        ["build", "--target", "native", "--dry-run"],
        [("MOON_CC", cc)],
    );
    println!("{}", &build_stdout);
    let lines = build_stdout.lines().collect::<Vec<_>>();

    let found_final_link = OnceCell::<()>::new();

    for line in lines {
        if line.contains("cc -o ./_build/native/debug/build/main/main") && cfg!(unix) {
            found_final_link.set(()).expect("final linking found twice");
            assert!(line.contains("-l__prebuild_self_link_flag__"));
            assert!(line.contains("-lprebuildselflib"));
            assert!(line.contains("-L/prebuild-self-path"));
        } else if line.contains("cl.exe")
            && line.contains("/Fe./_build/native/debug/build/main/main.exe")
            && cfg!(windows)
        {
            found_final_link.set(()).expect("final linking found twice");
            assert!(line.contains("-l__prebuild_self_link_flag__"));
            assert!(line.contains("prebuildselflib"));
            assert!(line.contains("/LIBPATH:/prebuild-self-path"));
        }
    }

    found_final_link.get().expect("final linking not found");

    if cfg!(unix) {
        let test_stdout = get_stdout_with_envs(
            &dir,
            ["test", "--target", "native", "--dry-run"],
            [("MOON_CC", cc)],
        );
        println!("{}", &test_stdout);
        let lines = test_stdout.lines().collect::<Vec<_>>();

        let mut found_test_links = 0;
        for line in lines {
            let is_test_link = line.contains("cc -o ./_build/native/debug/test/main/")
                && (line.contains(".exe")
                    || line.contains("libmain.so")
                    || line.contains("libmain.dylib"));
            if is_test_link {
                found_test_links += 1;
                assert!(line.contains("-l__prebuild_self_link_flag__"));
                assert!(line.contains("-lprebuildselflib"));
                assert!(line.contains("-L/prebuild-self-path"));
            }
        }

        assert!(found_test_links >= 1, "test executable linking not found");
    }
}
