use std::cell::OnceCell;

use crate::{TestDir, assert_success, get_err_stderr, get_stdout};

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
            assert!(line.contains("HELLO=------this-is-added-by-config-script------"));
        }

        if line.contains("cc -o ./_build/native/debug/build/main/main") && cfg!(unix) {
            found_link_flags.set(()).expect("final linking found twice");
            assert!(line.contains("-l______this_is_added_by_config_script_______"));
            assert!(line.contains("-lmylib"));
            assert!(line.contains("-L/my-search-path"));
        } else if line.contains("cl.exe") // cl.exe might be quoted
            && line.contains("/Fe./_build/native/debug/build/main/main.exe")
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

#[test]
fn test_prebuild_config_not_run_in_check() {
    let dir = TestDir::new("prebuild_config_script/check_skip_on_check");

    let build_err = get_err_stderr(&dir, ["build", "--dry-run"]);
    assert!(
        build_err.contains("prebuild script `fail.js`"),
        "expected build to execute prebuild script and fail, got:\n{build_err}"
    );

    assert_success(&dir, ["check"]);
}

#[test]
fn test_prebuild_config_in_bin_dep_runs_for_check_install() {
    let top_dir = TestDir::new("prebuild_config_script/check_skip_bin_dep.in");
    let dir = top_dir.join("user.in");
    let generated_stub = top_dir.join("author.in/src/main/generated_stub.c");
    assert!(
        !generated_stub.exists(),
        "generated stub should not exist before check"
    );
    assert_success(&dir, ["check"]);
    assert!(
        generated_stub.exists(),
        "expected bin-dep prebuild to generate required stub during check install"
    );
}
