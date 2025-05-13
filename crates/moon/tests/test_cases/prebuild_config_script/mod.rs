use std::cell::OnceCell;

use crate::{get_stdout, TestDir};

// Notice the two `this-is-added-by-config-script`
#[test]
fn test_prebuild_config_js() {
    let dir = TestDir::new("prebuild_config_script/js");
    test_prebuild_config_common(dir);
}

#[test]
fn test_prebuild_config_py() {
    let dir = TestDir::new("prebuild_config_script/py");
    test_prebuild_config_common(dir);
}

fn test_prebuild_config_common(dir: TestDir) {
    let stdout = get_stdout(&dir, ["build", "--target", "native", "--dry-run"]);
    println!("{}", &stdout);
    let lines = stdout.lines().collect::<Vec<_>>();

    let found_c_flags_replacement = OnceCell::<()>::new();
    let found_link_flags = OnceCell::<()>::new();
    for line in lines {
        if line.contains("stub.c") {
            found_c_flags_replacement
                .set(())
                .expect("c stub compilation found twice");
            assert!(line.contains("-D HELLO=------this-is-added-by-config-script------"));
        }

        if line.contains("cc -o ./target/native/release/build/main/main") && cfg!(unix) {
            found_link_flags.set(()).expect("final linking found twice");
            assert!(line.contains("-l______this_is_added_by_config_script_______"));
            assert!(line.contains("-lmylib"));
            assert!(line.contains("-L/my-search-path"));
        } else if line.contains("cl.exe /Fe./target/native/release/build/main/main.exe")
            && cfg!(windows)
        {
            found_link_flags.set(()).expect("final linking found twice");
            assert!(line.contains("-l______this_is_added_by_config_script_______"));
            assert!(line.contains("mylib"));
            assert!(line.contains("/LIBPATH:/my-search-path"));
        }
    }
    found_c_flags_replacement
        .get()
        .expect("c stub compilation not found");
    found_link_flags.get().expect("link flags not found");
}
