use std::cell::OnceCell;

use crate::{TestDir, get_stdout};

#[test]
fn test_prebuild_link_config_self() {
    let dir = TestDir::new("prebuild_link_config_self/prebuild_link_config_self.in");
    let build_stdout = get_stdout(&dir, ["build", "--target", "native", "--dry-run"]);
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
        let test_stdout = get_stdout(&dir, ["test", "--target", "native", "--dry-run"]);
        println!("{}", &test_stdout);
        let lines = test_stdout.lines().collect::<Vec<_>>();

        let found_stub_link = OnceCell::<()>::new();
        for line in lines {
            let is_stub_link = line.contains("libmain") && line.contains("-shared");
            if is_stub_link {
                found_stub_link.set(()).expect("c stub linking found twice");
                assert!(line.contains("-l__prebuild_self_link_flag__"));
                assert!(line.contains("-lprebuildselflib"));
                assert!(line.contains("-L/prebuild-self-path"));
            }
        }

        found_stub_link.get().expect("c stub linking not found");
    }
}
