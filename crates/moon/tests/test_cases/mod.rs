// moon: The build system and package manager for MoonBit.
// Copyright (C) 2024 International Digital Economy Academy
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

use std::io::Write;

use crate::build_graph::compare_graphs;
use expect_test::expect_file;

use super::*;
use expect_test::expect;
use moonutil::{
    common::{
        BUILD_DIR, CargoPathExt, DEP_PATH, MBTI_GENERATED, MOON_MOD_JSON, StringExt, TargetBackend,
        get_cargo_pkg_version,
    },
    module::MoonModJSON,
};
use walkdir::WalkDir;

mod backend;
mod backend_config;
mod bench2;
mod blackbox;
mod check_fmt;
mod circle_pkg_ab_001_test;
mod clean;
mod cond_comp;
mod debug_flag_test;
mod dedup_diag;
mod dep_order;
mod design;
mod diagnostics_format;
mod diamond_pkg;
mod docs_examples;
mod dummy_core;
mod extra_flags;
mod fancy_import;
mod filter_by_path;
mod fmt;
mod fmt_ignore;
mod fmt_moon_pkg;
mod fmt_path;
mod fuzzy_matching;
mod hello;
mod indirect_dep;
mod inline_test;
mod js_test_build_only;
mod mbti;
mod moon_bench;
mod moon_build_package;
mod moon_bundle;
mod moon_commands;
mod moon_coverage;
mod moon_info_001;
mod moon_info_002;
mod moon_info_compare_backends;
mod moon_new;
mod moon_test;
mod moon_version;
mod native_backend;
mod native_stub_stability;
mod no_export_when_test;
mod output_format;
mod packages;
mod prebuild;
mod prebuild_config_script;
mod query_symbol;
mod run_doc_test;
mod run_md_test;
mod simple_pkg;
mod snapshot_testing;
mod specify_source_dir_001;
mod specify_source_dir_002;
mod symlink_file_discovery;
mod target_backend;
mod targets;
mod test_dot_source;
mod test_driver_dependencies;
mod test_error_report;
mod test_exclude_001;
mod test_exclude_002;
mod test_expect_test;
mod test_filter;
mod test_include_001;
mod test_include_002;
mod test_include_003;
mod test_moon_info;
mod test_outline;
mod test_moonbitlang_x;
mod test_release;
mod third_party;
mod value_tracing;
mod virtual_pkg;
mod virtual_pkg2;
mod virtual_pkg_dep;
mod virtual_pkg_test;
mod warns;
mod wbtest_coverage;
mod whitespace_test;

#[test]
fn test_moon_pkg() {
    let dir = TestDir::new("moon_pkg.in");
    check(
        get_stdout(&dir, ["check", "--dry-run"]),
        expect![[r#"
            cat ./pkg/pkg.mbt '>' ./pkg/gen.txt
            moonc check ./pkg/pkg.mbt -w -unused_value-todo -o ./_build/wasm-gc/release/check/pkg/pkg.mi -pkg user/mod/pkg -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources user/mod/pkg:./pkg -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check ./pkg/pkg_test.mbt -doctest-only ./pkg/pkg.mbt -include-doctests -w -unused_value-todo -o ./_build/wasm-gc/release/check/pkg/pkg.blackbox_test.mi -pkg user/mod/pkg_blackbox_test -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/pkg/pkg.mi:pkg -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources user/mod/pkg_blackbox_test:./pkg -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check ./main/main.mbt -o ./_build/wasm-gc/release/check/main/main.mi -pkg user/mod/main -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/pkg/pkg.mi:lib -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources user/mod/main:./main -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check -doctest-only ./main/main.mbt -include-doctests -o ./_build/wasm-gc/release/check/main/main.blackbox_test.mi -pkg user/mod/main_blackbox_test -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/pkg/pkg.mi:lib -i ./_build/wasm-gc/release/check/main/main.mi:main -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources user/mod/main_blackbox_test:./main -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
        "#]],
    );
    check(
        get_stdout(&dir, ["build", "--dry-run"]),
        expect![[r#"
            cat ./pkg/pkg.mbt '>' ./pkg/gen.txt
            moonc build-package ./pkg/pkg.mbt -w -unused_value-todo -o ./_build/wasm-gc/release/build/pkg/pkg.core -pkg user/mod/pkg -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources user/mod/pkg:./pkg -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/release/build/pkg/pkg.core -main user/mod/pkg -o ./_build/wasm-gc/release/build/pkg/pkg.wasm -pkg-config-path ./pkg/moon.pkg -pkg-sources user/mod/pkg:./pkg -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm-gc
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/release/build/main/main.core -pkg user/mod/main -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/build/pkg/pkg.mi:lib -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources user/mod/main:./main -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/release/build/pkg/pkg.core ./_build/wasm-gc/release/build/main/main.core -main user/mod/main -o ./_build/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg -pkg-sources user/mod/pkg:./pkg -pkg-sources user/mod/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm-gc
        "#]],
    );
}

#[test]
fn test_need_link() {
    let dir = TestDir::new("need_link.in");
    check(
        get_stdout(&dir, ["build", "--dry-run", "--nostd", "--sort-input"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/release/build/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -i ./_build/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc link-core ./_build/wasm-gc/release/build/lib/lib.core ./_build/wasm-gc/release/build/main/main.core -main username/hello/main -o ./_build/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -target wasm-gc
            moonc link-core ./_build/wasm-gc/release/build/lib/lib.core -main username/hello/lib -o ./_build/wasm-gc/release/build/lib/lib.wasm -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -target wasm-gc
        "#]],
    );
}

#[test]
fn test_no_work_to_do() {
    let dir = TestDir::new("moon_new/plain");
    let out = get_stderr(&dir, ["check"]);
    assert!(out.contains("now up to date"));

    let out = get_stderr(&dir, ["check"]);
    assert!(out.contains("moon: no work to do"));

    let out = get_stderr(&dir, ["build"]);
    assert!(out.contains("now up to date"));
    let out = get_stderr(&dir, ["build"]);
    assert!(out.contains("moon: no work to do"));
}

#[test]
fn test_backtrace() {
    let dir = TestDir::new("backtrace.in");

    let out = get_err_stderr(&dir, ["run", "main"]);
    assert!(!out.contains("main.foo"));
    assert!(!out.contains("main.bar"));

    let out = get_err_stderr(&dir, ["run", "main", "--debug"]);
    assert!(out.contains("4main3foo"));
    assert!(out.contains("4main3bar"));
}

#[test]
fn test_export_memory_name() {
    let dir = TestDir::new("export_memory.in");

    // Check the commands
    // build wasm-gc/wasm should have this flag
    check(
        get_stdout(&dir, ["build", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/release/build/lib/lib.core -pkg username/hello/lib -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/build/lib/lib.mi:lib -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main:./main -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/release/build/lib/lib.core ./_build/wasm-gc/release/build/main/main.core -main username/hello/main -o ./_build/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm-gc -export-memory-name awesome_memory
        "#]],
    );

    check(
        get_stdout(
            &dir,
            ["build", "--dry-run", "--sort-input", "--target", "wasm"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm/release/build/lib/lib.core -pkg username/hello/lib -std-path '$MOON_HOME/lib/core/target/wasm/release/bundle' -i '$MOON_HOME/lib/core/target/wasm/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./lib -target wasm -workspace-path . -all-pkgs ./_build/wasm/release/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm/release/build/main/main.core -pkg username/hello/main -is-main -std-path '$MOON_HOME/lib/core/target/wasm/release/bundle' -i ./_build/wasm/release/build/lib/lib.mi:lib -i '$MOON_HOME/lib/core/target/wasm/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main:./main -target wasm -workspace-path . -all-pkgs ./_build/wasm/release/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/target/wasm/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/wasm/release/bundle/core.core' ./_build/wasm/release/build/lib/lib.core ./_build/wasm/release/build/main/main.core -main username/hello/main -o ./_build/wasm/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm -export-memory-name awesome_memory
        "#]],
    );

    // js is not wasm so should not have export-memory-name flag
    check(
        get_stdout(
            &dir,
            ["build", "--dry-run", "--sort-input", "--target", "js"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/js/release/build/lib/lib.core -pkg username/hello/lib -std-path '$MOON_HOME/lib/core/target/js/release/bundle' -i '$MOON_HOME/lib/core/target/js/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./lib -target js -workspace-path . -all-pkgs ./_build/js/release/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/js/release/build/main/main.core -pkg username/hello/main -is-main -std-path '$MOON_HOME/lib/core/target/js/release/bundle' -i ./_build/js/release/build/lib/lib.mi:lib -i '$MOON_HOME/lib/core/target/js/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main:./main -target js -workspace-path . -all-pkgs ./_build/js/release/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/target/js/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/js/release/bundle/core.core' ./_build/js/release/build/lib/lib.core ./_build/js/release/build/main/main.core -main username/hello/main -o ./_build/js/release/build/main/main.js -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target js
        "#]],
    );

    // Check the results
    let _ = get_stdout(&dir, ["build", "--target", "wasm-gc", "--output-wat"]);
    let content = std::fs::read_to_string(
        dir.join(BUILD_DIR)
            .join("wasm-gc")
            .join("release")
            .join("build")
            .join("main")
            .join("main.wat"),
    )
    .unwrap();
    assert!(content.contains("awesome_memory"));
}

#[test]
fn test_no_block_params() {
    let dir = TestDir::new("no_block_params.in");
    check(
        get_stdout(&dir, ["build", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/release/build/lib/lib.core -pkg username/hello/lib -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/build/lib/lib.mi:lib -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main:./main -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/release/build/lib/lib.core ./_build/wasm-gc/release/build/main/main.core -main username/hello/main -o ./_build/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm-gc -no-block-params
        "#]],
    );

    check(
        get_stdout(
            &dir,
            ["build", "--dry-run", "--sort-input", "--target", "wasm"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm/release/build/lib/lib.core -pkg username/hello/lib -std-path '$MOON_HOME/lib/core/target/wasm/release/bundle' -i '$MOON_HOME/lib/core/target/wasm/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./lib -target wasm -workspace-path . -all-pkgs ./_build/wasm/release/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm/release/build/main/main.core -pkg username/hello/main -is-main -std-path '$MOON_HOME/lib/core/target/wasm/release/bundle' -i ./_build/wasm/release/build/lib/lib.mi:lib -i '$MOON_HOME/lib/core/target/wasm/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main:./main -target wasm -workspace-path . -all-pkgs ./_build/wasm/release/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/target/wasm/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/wasm/release/bundle/core.core' ./_build/wasm/release/build/lib/lib.core ./_build/wasm/release/build/main/main.core -main username/hello/main -o ./_build/wasm/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm -no-block-params
        "#]],
    );

    check(
        get_stdout(
            &dir,
            ["build", "--dry-run", "--sort-input", "--target", "js"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/js/release/build/lib/lib.core -pkg username/hello/lib -std-path '$MOON_HOME/lib/core/target/js/release/bundle' -i '$MOON_HOME/lib/core/target/js/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./lib -target js -workspace-path . -all-pkgs ./_build/js/release/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/js/release/build/main/main.core -pkg username/hello/main -is-main -std-path '$MOON_HOME/lib/core/target/js/release/bundle' -i ./_build/js/release/build/lib/lib.mi:lib -i '$MOON_HOME/lib/core/target/js/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main:./main -target js -workspace-path . -all-pkgs ./_build/js/release/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/target/js/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/js/release/bundle/core.core' ./_build/js/release/build/lib/lib.core ./_build/js/release/build/main/main.core -main username/hello/main -o ./_build/js/release/build/main/main.js -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target js
        "#]],
    );
}

#[test]
fn test_panic() {
    let dir = TestDir::new("panic.in");
    let data = snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["test"])
        .assert()
        .failure()
        .get_output()
        .stdout
        .to_owned();
    let out = String::from_utf8_lossy(&data).to_string();
    check(
        &out,
        expect![[r#"
            [username/hello] test lib/hello_wbtest.mbt:3 ("panic") failed: panic is expected
            Total tests: 2, passed: 1, failed: 1.
        "#]],
    );
}

#[test]
fn test_validate_import() {
    let dir = TestDir::new("validate_import.in");
    check(
        get_err_stderr(&dir, ["check"]),
        expect![[r#"
            error: Failed to calculate build plan

            Caused by:
                0: Failed to solve package relationship
                1: Cannot find import 'mbt/core/set' in username/hello/main@0.1.0
        "#]],
    );
    check(
        get_err_stderr(&dir, ["build"]),
        expect![[r#"
            error: Failed to solve package relationship

            Caused by:
                Cannot find import 'mbt/core/set' in username/hello/main@0.1.0
        "#]],
    );
    check(
        get_err_stderr(&dir, ["test"]),
        expect![[r#"
            error: Failed to solve package relationship

            Caused by:
                Cannot find import 'mbt/core/set' in username/hello/main@0.1.0
        "#]],
    );
    check(
        get_err_stderr(&dir, ["bundle"]),
        expect![[r#"
            error: Failed to solve package relationship

            Caused by:
                Cannot find import 'mbt/core/set' in username/hello/main@0.1.0
        "#]],
    );
}

#[test]
fn test_multi_process() {
    use std::process::Command;
    use std::thread;

    let dir = TestDir::new("test_multi_process");
    let path: PathBuf = dir.as_ref().into();

    let (num_threads, inner_loop) = (16, 10);
    let mut container = vec![];

    let success = std::sync::Arc::new(std::sync::atomic::AtomicI32::new(0));

    for _ in 0..num_threads {
        let path = path.clone();
        let success = success.clone();
        let work = thread::spawn(move || {
            for _ in 0..inner_loop {
                let _ = std::fs::OpenOptions::new()
                    .append(true)
                    .open(path.join("lib/hello.mbt"))
                    .unwrap()
                    .write(b"\n")
                    .unwrap();

                let output = Command::new(moon_bin())
                    .arg("check")
                    .current_dir(path.clone())
                    .output()
                    .expect("Failed to execute command");

                if output.status.success() {
                    success.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    let out = String::from_utf8(output.stderr).unwrap();
                    assert!(out.contains("no work to do") || out.contains("now up to date"));
                } else {
                    println!("moon output: {:?}", String::from_utf8(output.stdout));
                    let error_message = String::from_utf8_lossy(&output.stderr);
                    println!("{error_message}");
                }
            }
        });
        container.push(work);
    }

    for i in container {
        i.join().unwrap();
    }

    assert_eq!(
        success.load(std::sync::atomic::Ordering::SeqCst),
        num_threads * inner_loop
    );
}

#[test]
fn test_internal_package() {
    let dir = TestDir::new("internal_package.in");
    let output = get_err_stderr(&dir, ["check", "--sort-input"]);

    // Might need a better way
    assert!(
        output
            .to_lowercase()
            .contains("cannot import internal package")
    );
}

#[test]
fn test_nonexistent_package() {
    let dir = TestDir::new("nonexistent_package.in");
    check(
        get_err_stderr(&dir, ["check", "--sort-input"]),
        expect![[r#"
            error: Failed to calculate build plan

            Caused by:
                0: Failed to solve package relationship
                1: Cannot find import 'username/hello/lib/b' in username/hello/main@0.1.0
        "#]],
    );
}

#[test]
fn mooncakes_io_smoke_test() {
    if std::env::var("CI").is_err() {
        return;
    }
    let dir = TestDir::new("hello");
    let _ = get_stdout(&dir, ["update"]);
    let _ = get_stdout(&dir, ["add", "lijunchen/hello2@0.1.0"]);
    check(
        std::fs::read_to_string(dir.join("moon.mod.json")).unwrap(),
        expect![[r#"
            {
              "name": "hello",
              "deps": {
                "lijunchen/hello2": "0.1.0"
              }
            }"#]],
    );
    let _ = get_stdout(&dir, ["remove", "lijunchen/hello2"]);
    check(
        std::fs::read_to_string(dir.join("moon.mod.json")).unwrap(),
        expect![[r#"
            {
              "name": "hello",
              "deps": {}
            }"#]],
    );
    let _ = get_stdout(&dir, ["add", "lijunchen/hello2@0.1.0"]);
    std::fs::write(
        dir.join("main/main.mbt"),
        r#"fn main {
  println(@lib.hello2())
}
"#,
    )
    .unwrap();

    assert!(
        dir.join(DEP_PATH)
            .join("lijunchen")
            .join("hello")
            .join(MOON_MOD_JSON)
            .exists()
    );

    std::fs::remove_dir_all(dir.join(DEP_PATH)).unwrap();
    let out = get_stdout(&dir, ["install"]);
    let mut lines = out.lines().collect::<Vec<_>>();
    lines.sort();
    check(
        lines.join("\n"),
        expect![[r#"
            Using cached lijunchen/hello2@0.1.0
            Using cached lijunchen/hello@0.1.0"#]],
    );

    std::fs::write(
        dir.join("main/moon.pkg.json"),
        r#"{
          "is-main": true,
          "import": [
            "lijunchen/hello2/lib"
          ]
        }
    "#,
    )
    .unwrap();

    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
            Hello, world!Hello, world2!
        "#]],
    );
}

#[test]
#[ignore = "where to download mooncake?"]
fn mooncake_cli_smoke_test() {
    let dir = TestDir::new("hello.in");
    let out = std::process::Command::new(moon_bin())
        .env("RUST_BACKTRACE", "0")
        .current_dir(&dir)
        .args(["publish"])
        .output()
        .unwrap();
    let s = std::str::from_utf8(&out.stderr).unwrap().to_string();
    assert!(s.contains("failed to open credentials file"));
}

#[test]
fn bench2_test() {
    let dir = TestDir::new("bench2_test.in");
    snapbox::cmd::Command::new(snapbox::cargo_bin!("moon"))
        .current_dir(&dir)
        .args(["run", "main"])
        .assert()
        .success()
        .stdout_eq("ok[..]");
}

#[test]
fn cakenew_test() {
    let dir = TestDir::new("cakenew_test.in");
    snapbox::cmd::Command::new(snapbox::cargo_bin!("moon"))
        .current_dir(&dir)
        .args(["run", "main"])
        .assert()
        .success()
        .stdout_eq("Hello,[..]");
}

#[test]
fn capture_abort_test() {
    let dir = super::TestDir::new("capture_abort_test.in");
    snapbox::cmd::Command::new(snapbox::cargo_bin!("moon"))
        .current_dir(&dir)
        .args(["run", "main", "--nostd"])
        .assert()
        .failure();
}

#[test]
fn test_check_failed_should_write_pkg_json() {
    let dir = TestDir::new("check_failed_should_write_pkg_json.in");
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["check"])
        .assert()
        .failure();

    let pkg_json = dir.join("target/packages.json");
    assert!(pkg_json.exists());
}

#[test]
fn test_moon_run_with_cli_args() {
    let dir = TestDir::new("moon_run_with_cli_args.in");

    check(
        get_stdout(&dir, ["run", "main", "--dry-run"]),
        expect![[r#"
            moonc build-package ./main/main_wasm.mbt -o ./_build/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main:./main -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/release/build/main/main.core -main username/hello/main -o ./_build/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm-gc
            moonrun ./_build/wasm-gc/release/build/main/main.wasm --
        "#]],
    );

    let run_graph = dir.join("run_graph.jsonl");
    snap_dry_run_graph(
        &dir,
        [
            "run",
            "main",
            "--dry-run",
            "--",
            "中文",
            "😄👍",
            "hello",
            "1242",
        ],
        &run_graph,
    );
    compare_graphs(
        &run_graph,
        expect_file!["./moon_run_with_cli_args_graph.jsonl"],
    );

    let s = get_stdout(
        &dir,
        [
            "run", "main", "--", "中文", "😄👍", "hello", "1242", "--flag",
        ],
    );
    assert!(s.contains("\"中文\", \"😄👍\", \"hello\", \"1242\", \"--flag\""));

    let s = get_stdout(
        &dir,
        [
            "run", "main", "--target", "js", "--", "中文", "😄👍", "hello", "1242", "--flag",
        ],
    );
    assert!(s.contains("\"中文\", \"😄👍\", \"hello\", \"1242\", \"--flag\""));
}

#[test]
fn test_import_memory_and_heap_start() {
    let dir = TestDir::new("import_memory.in");
    check(
        get_stdout(
            &dir,
            [
                "build",
                "--target",
                "wasm",
                "--dry-run",
                "--sort-input",
                "--nostd",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm/release/build/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target wasm -workspace-path . -all-pkgs ./_build/wasm/release/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm/release/build/main/main.core -pkg username/hello/main -is-main -i ./_build/wasm/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm -workspace-path . -all-pkgs ./_build/wasm/release/build/all_pkgs.json
            moonc link-core ./_build/wasm/release/build/lib/lib.core ./_build/wasm/release/build/main/main.core -main username/hello/main -o ./_build/wasm/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -target wasm -import-memory-module xxx -import-memory-name yyy -heap-start-address 65536
        "#]],
    );

    let dir = TestDir::new("import_memory.in");
    check(
        get_stdout(
            &dir,
            [
                "build",
                "--target",
                "wasm-gc",
                "--dry-run",
                "--sort-input",
                "--nostd",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/release/build/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -i ./_build/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc link-core ./_build/wasm-gc/release/build/lib/lib.core ./_build/wasm-gc/release/build/main/main.core -main username/hello/main -o ./_build/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -target wasm-gc -import-memory-module xxx -import-memory-name yyy
        "#]],
    );
}

#[test]
fn test_import_shared_memory() {
    let dir = TestDir::new("import_shared_memory.in");
    check(
        get_stdout(
            &dir,
            [
                "build",
                "--target",
                "wasm",
                "--dry-run",
                "--sort-input",
                "--nostd",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm/release/build/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target wasm -workspace-path . -all-pkgs ./_build/wasm/release/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm/release/build/main/main.core -pkg username/hello/main -is-main -i ./_build/wasm/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm -workspace-path . -all-pkgs ./_build/wasm/release/build/all_pkgs.json
            moonc link-core ./_build/wasm/release/build/lib/lib.core ./_build/wasm/release/build/main/main.core -main username/hello/main -o ./_build/wasm/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -target wasm -import-memory-module xxx -import-memory-name yyy -memory-limits-min 1 -memory-limits-max 65536 -shared-memory -heap-start-address 65536
        "#]],
    );

    let dir = TestDir::new("import_shared_memory.in");
    check(
        get_stdout(
            &dir,
            [
                "build",
                "--target",
                "wasm-gc",
                "--dry-run",
                "--sort-input",
                "--nostd",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/release/build/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -i ./_build/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc link-core ./_build/wasm-gc/release/build/lib/lib.core ./_build/wasm-gc/release/build/main/main.core -main username/hello/main -o ./_build/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -target wasm-gc -import-memory-module xxx -import-memory-name yyy -memory-limits-min 1 -memory-limits-max 65535 -shared-memory
        "#]],
    );
}

#[cfg(unix)]
#[test]
fn test_moon_run_single_file_dry_run() {
    let dir = TestDir::new("run_single_mbt_file.in");

    let output = get_stdout(
        &dir,
        ["run", "a/b/single.mbt", "--target", "native", "--dry-run"],
    );
    check(
        &output,
        expect![[r#"
            moonc build-package ./single.mbt -o ./_build/native/debug/build/single/single.core -pkg moon/test/single -is-main -std-path '$MOON_HOME/lib/core/target/native/release/bundle' -i '$MOON_HOME/lib/core/target/native/release/bundle/abort/abort.mi:abort' -i '$MOON_HOME/lib/core/target/native/release/bundle/array/array.mi:array' -i '$MOON_HOME/lib/core/target/native/release/bundle/bench/bench.mi:bench' -i '$MOON_HOME/lib/core/target/native/release/bundle/bigint/bigint.mi:bigint' -i '$MOON_HOME/lib/core/target/native/release/bundle/bool/bool.mi:bool' -i '$MOON_HOME/lib/core/target/native/release/bundle/buffer/buffer.mi:buffer' -i '$MOON_HOME/lib/core/target/native/release/bundle/builtin/builtin.mi:builtin' -i '$MOON_HOME/lib/core/target/native/release/bundle/byte/byte.mi:byte' -i '$MOON_HOME/lib/core/target/native/release/bundle/bytes/bytes.mi:bytes' -i '$MOON_HOME/lib/core/target/native/release/bundle/char/char.mi:char' -i '$MOON_HOME/lib/core/target/native/release/bundle/cmp/cmp.mi:cmp' -i '$MOON_HOME/lib/core/target/native/release/bundle/coverage/coverage.mi:coverage' -i '$MOON_HOME/lib/core/target/native/release/bundle/debug/debug.mi:debug' -i '$MOON_HOME/lib/core/target/native/release/bundle/deque/deque.mi:deque' -i '$MOON_HOME/lib/core/target/native/release/bundle/double/double.mi:double' -i '$MOON_HOME/lib/core/target/native/release/bundle/env/env.mi:env' -i '$MOON_HOME/lib/core/target/native/release/bundle/error/error.mi:error' -i '$MOON_HOME/lib/core/target/native/release/bundle/float/float.mi:float' -i '$MOON_HOME/lib/core/target/native/release/bundle/hashmap/hashmap.mi:hashmap' -i '$MOON_HOME/lib/core/target/native/release/bundle/hashset/hashset.mi:hashset' -i '$MOON_HOME/lib/core/target/native/release/bundle/immut/array/array.mi:immut/array' -i '$MOON_HOME/lib/core/target/native/release/bundle/immut/hashmap/hashmap.mi:immut/hashmap' -i '$MOON_HOME/lib/core/target/native/release/bundle/immut/hashset/hashset.mi:immut/hashset' -i '$MOON_HOME/lib/core/target/native/release/bundle/immut/priority_queue/priority_queue.mi:immut/priority_queue' -i '$MOON_HOME/lib/core/target/native/release/bundle/immut/sorted_map/sorted_map.mi:immut/sorted_map' -i '$MOON_HOME/lib/core/target/native/release/bundle/immut/sorted_set/sorted_set.mi:immut/sorted_set' -i '$MOON_HOME/lib/core/target/native/release/bundle/int/int.mi:int' -i '$MOON_HOME/lib/core/target/native/release/bundle/int16/int16.mi:int16' -i '$MOON_HOME/lib/core/target/native/release/bundle/int64/int64.mi:int64' -i '$MOON_HOME/lib/core/target/native/release/bundle/json/json.mi:json' -i '$MOON_HOME/lib/core/target/native/release/bundle/list/list.mi:list' -i '$MOON_HOME/lib/core/target/native/release/bundle/math/math.mi:math' -i '$MOON_HOME/lib/core/target/native/release/bundle/option/option.mi:option' -i '$MOON_HOME/lib/core/target/native/release/bundle/prelude/prelude.mi:prelude' -i '$MOON_HOME/lib/core/target/native/release/bundle/priority_queue/priority_queue.mi:priority_queue' -i '$MOON_HOME/lib/core/target/native/release/bundle/queue/queue.mi:queue' -i '$MOON_HOME/lib/core/target/native/release/bundle/quickcheck/quickcheck.mi:quickcheck' -i '$MOON_HOME/lib/core/target/native/release/bundle/random/random.mi:random' -i '$MOON_HOME/lib/core/target/native/release/bundle/ref/ref.mi:ref' -i '$MOON_HOME/lib/core/target/native/release/bundle/result/result.mi:result' -i '$MOON_HOME/lib/core/target/native/release/bundle/set/set.mi:set' -i '$MOON_HOME/lib/core/target/native/release/bundle/sorted_map/sorted_map.mi:sorted_map' -i '$MOON_HOME/lib/core/target/native/release/bundle/sorted_set/sorted_set.mi:sorted_set' -i '$MOON_HOME/lib/core/target/native/release/bundle/quickcheck/splitmix/splitmix.mi:splitmix' -i '$MOON_HOME/lib/core/target/native/release/bundle/strconv/strconv.mi:strconv' -i '$MOON_HOME/lib/core/target/native/release/bundle/string/string.mi:string' -i '$MOON_HOME/lib/core/target/native/release/bundle/test/test.mi:test' -i '$MOON_HOME/lib/core/target/native/release/bundle/tuple/tuple.mi:tuple' -i '$MOON_HOME/lib/core/target/native/release/bundle/uint/uint.mi:uint' -i '$MOON_HOME/lib/core/target/native/release/bundle/uint16/uint16.mi:uint16' -i '$MOON_HOME/lib/core/target/native/release/bundle/uint64/uint64.mi:uint64' -i '$MOON_HOME/lib/core/target/native/release/bundle/unit/unit.mi:unit' -i '$MOON_HOME/lib/core/target/native/release/bundle/encoding/utf16/utf16.mi:utf16' -i '$MOON_HOME/lib/core/target/native/release/bundle/encoding/utf8/utf8.mi:utf8' -pkg-sources moon/test/single:. -target native -O0 -workspace-path . -all-pkgs ./_build/native/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/native/release/bundle/core.core' ./_build/native/debug/build/single/single.core -main moon/test/single -o ./_build/native/debug/build/single/single.c -pkg-config-path ./moon.pkg.json -pkg-sources moon/test/single:. -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target native -O0
            cc -o ./_build/native/debug/build/runtime.o '-I$MOON_HOME/include' -g -c -fwrapv -fno-strict-aliasing -O2 '$MOON_HOME/lib/runtime.c'
            cc -o ./_build/native/debug/build/single/single.exe '-I$MOON_HOME/include' -fwrapv -fno-strict-aliasing '$MOON_HOME/lib/libmoonbitrun.o' ./_build/native/debug/build/single/single.c ./_build/native/debug/build/runtime.o -lm '$MOON_HOME/lib/libbacktrace.a'
            ./_build/native/debug/build/single/single.exe
        "#]],
    );

    let output = get_stdout(
        &dir,
        [
            "run",
            "a/b/single.mbt",
            "--target",
            "native",
            "--dry-run",
            "--release",
        ],
    );
    check(
        &output,
        expect![[r#"
            moonc build-package ./single.mbt -o ./_build/native/release/build/single/single.core -pkg moon/test/single -is-main -std-path '$MOON_HOME/lib/core/target/native/release/bundle' -i '$MOON_HOME/lib/core/target/native/release/bundle/abort/abort.mi:abort' -i '$MOON_HOME/lib/core/target/native/release/bundle/array/array.mi:array' -i '$MOON_HOME/lib/core/target/native/release/bundle/bench/bench.mi:bench' -i '$MOON_HOME/lib/core/target/native/release/bundle/bigint/bigint.mi:bigint' -i '$MOON_HOME/lib/core/target/native/release/bundle/bool/bool.mi:bool' -i '$MOON_HOME/lib/core/target/native/release/bundle/buffer/buffer.mi:buffer' -i '$MOON_HOME/lib/core/target/native/release/bundle/builtin/builtin.mi:builtin' -i '$MOON_HOME/lib/core/target/native/release/bundle/byte/byte.mi:byte' -i '$MOON_HOME/lib/core/target/native/release/bundle/bytes/bytes.mi:bytes' -i '$MOON_HOME/lib/core/target/native/release/bundle/char/char.mi:char' -i '$MOON_HOME/lib/core/target/native/release/bundle/cmp/cmp.mi:cmp' -i '$MOON_HOME/lib/core/target/native/release/bundle/coverage/coverage.mi:coverage' -i '$MOON_HOME/lib/core/target/native/release/bundle/debug/debug.mi:debug' -i '$MOON_HOME/lib/core/target/native/release/bundle/deque/deque.mi:deque' -i '$MOON_HOME/lib/core/target/native/release/bundle/double/double.mi:double' -i '$MOON_HOME/lib/core/target/native/release/bundle/env/env.mi:env' -i '$MOON_HOME/lib/core/target/native/release/bundle/error/error.mi:error' -i '$MOON_HOME/lib/core/target/native/release/bundle/float/float.mi:float' -i '$MOON_HOME/lib/core/target/native/release/bundle/hashmap/hashmap.mi:hashmap' -i '$MOON_HOME/lib/core/target/native/release/bundle/hashset/hashset.mi:hashset' -i '$MOON_HOME/lib/core/target/native/release/bundle/immut/array/array.mi:immut/array' -i '$MOON_HOME/lib/core/target/native/release/bundle/immut/hashmap/hashmap.mi:immut/hashmap' -i '$MOON_HOME/lib/core/target/native/release/bundle/immut/hashset/hashset.mi:immut/hashset' -i '$MOON_HOME/lib/core/target/native/release/bundle/immut/priority_queue/priority_queue.mi:immut/priority_queue' -i '$MOON_HOME/lib/core/target/native/release/bundle/immut/sorted_map/sorted_map.mi:immut/sorted_map' -i '$MOON_HOME/lib/core/target/native/release/bundle/immut/sorted_set/sorted_set.mi:immut/sorted_set' -i '$MOON_HOME/lib/core/target/native/release/bundle/int/int.mi:int' -i '$MOON_HOME/lib/core/target/native/release/bundle/int16/int16.mi:int16' -i '$MOON_HOME/lib/core/target/native/release/bundle/int64/int64.mi:int64' -i '$MOON_HOME/lib/core/target/native/release/bundle/json/json.mi:json' -i '$MOON_HOME/lib/core/target/native/release/bundle/list/list.mi:list' -i '$MOON_HOME/lib/core/target/native/release/bundle/math/math.mi:math' -i '$MOON_HOME/lib/core/target/native/release/bundle/option/option.mi:option' -i '$MOON_HOME/lib/core/target/native/release/bundle/prelude/prelude.mi:prelude' -i '$MOON_HOME/lib/core/target/native/release/bundle/priority_queue/priority_queue.mi:priority_queue' -i '$MOON_HOME/lib/core/target/native/release/bundle/queue/queue.mi:queue' -i '$MOON_HOME/lib/core/target/native/release/bundle/quickcheck/quickcheck.mi:quickcheck' -i '$MOON_HOME/lib/core/target/native/release/bundle/random/random.mi:random' -i '$MOON_HOME/lib/core/target/native/release/bundle/ref/ref.mi:ref' -i '$MOON_HOME/lib/core/target/native/release/bundle/result/result.mi:result' -i '$MOON_HOME/lib/core/target/native/release/bundle/set/set.mi:set' -i '$MOON_HOME/lib/core/target/native/release/bundle/sorted_map/sorted_map.mi:sorted_map' -i '$MOON_HOME/lib/core/target/native/release/bundle/sorted_set/sorted_set.mi:sorted_set' -i '$MOON_HOME/lib/core/target/native/release/bundle/quickcheck/splitmix/splitmix.mi:splitmix' -i '$MOON_HOME/lib/core/target/native/release/bundle/strconv/strconv.mi:strconv' -i '$MOON_HOME/lib/core/target/native/release/bundle/string/string.mi:string' -i '$MOON_HOME/lib/core/target/native/release/bundle/test/test.mi:test' -i '$MOON_HOME/lib/core/target/native/release/bundle/tuple/tuple.mi:tuple' -i '$MOON_HOME/lib/core/target/native/release/bundle/uint/uint.mi:uint' -i '$MOON_HOME/lib/core/target/native/release/bundle/uint16/uint16.mi:uint16' -i '$MOON_HOME/lib/core/target/native/release/bundle/uint64/uint64.mi:uint64' -i '$MOON_HOME/lib/core/target/native/release/bundle/unit/unit.mi:unit' -i '$MOON_HOME/lib/core/target/native/release/bundle/encoding/utf16/utf16.mi:utf16' -i '$MOON_HOME/lib/core/target/native/release/bundle/encoding/utf8/utf8.mi:utf8' -pkg-sources moon/test/single:. -target native -workspace-path . -all-pkgs ./_build/native/release/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/native/release/bundle/core.core' ./_build/native/release/build/single/single.core -main moon/test/single -o ./_build/native/release/build/single/single.c -pkg-config-path ./moon.pkg.json -pkg-sources moon/test/single:. -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target native
            cc -o ./_build/native/release/build/runtime.o '-I$MOON_HOME/include' -g -c -fwrapv -fno-strict-aliasing -O2 '$MOON_HOME/lib/runtime.c'
            cc -o ./_build/native/release/build/single/single.exe '-I$MOON_HOME/include' -fwrapv -fno-strict-aliasing '$MOON_HOME/lib/libmoonbitrun.o' ./_build/native/release/build/single/single.c ./_build/native/release/build/runtime.o -lm '$MOON_HOME/lib/libbacktrace.a'
            ./_build/native/release/build/single/single.exe
        "#]],
    );

    let output = get_stdout(
        &dir,
        [
            "run",
            "a/b/single.mbt",
            "--target",
            "js",
            "--build-only",
            "--dry-run",
        ],
    );
    check(
        &output,
        expect![[r#"
            moonc build-package ./single.mbt -o ./_build/js/debug/build/single/single.core -pkg moon/test/single -is-main -std-path '$MOON_HOME/lib/core/target/js/release/bundle' -i '$MOON_HOME/lib/core/target/js/release/bundle/abort/abort.mi:abort' -i '$MOON_HOME/lib/core/target/js/release/bundle/array/array.mi:array' -i '$MOON_HOME/lib/core/target/js/release/bundle/bench/bench.mi:bench' -i '$MOON_HOME/lib/core/target/js/release/bundle/bigint/bigint.mi:bigint' -i '$MOON_HOME/lib/core/target/js/release/bundle/bool/bool.mi:bool' -i '$MOON_HOME/lib/core/target/js/release/bundle/buffer/buffer.mi:buffer' -i '$MOON_HOME/lib/core/target/js/release/bundle/builtin/builtin.mi:builtin' -i '$MOON_HOME/lib/core/target/js/release/bundle/byte/byte.mi:byte' -i '$MOON_HOME/lib/core/target/js/release/bundle/bytes/bytes.mi:bytes' -i '$MOON_HOME/lib/core/target/js/release/bundle/char/char.mi:char' -i '$MOON_HOME/lib/core/target/js/release/bundle/cmp/cmp.mi:cmp' -i '$MOON_HOME/lib/core/target/js/release/bundle/coverage/coverage.mi:coverage' -i '$MOON_HOME/lib/core/target/js/release/bundle/debug/debug.mi:debug' -i '$MOON_HOME/lib/core/target/js/release/bundle/deque/deque.mi:deque' -i '$MOON_HOME/lib/core/target/js/release/bundle/double/double.mi:double' -i '$MOON_HOME/lib/core/target/js/release/bundle/env/env.mi:env' -i '$MOON_HOME/lib/core/target/js/release/bundle/error/error.mi:error' -i '$MOON_HOME/lib/core/target/js/release/bundle/float/float.mi:float' -i '$MOON_HOME/lib/core/target/js/release/bundle/hashmap/hashmap.mi:hashmap' -i '$MOON_HOME/lib/core/target/js/release/bundle/hashset/hashset.mi:hashset' -i '$MOON_HOME/lib/core/target/js/release/bundle/immut/array/array.mi:immut/array' -i '$MOON_HOME/lib/core/target/js/release/bundle/immut/hashmap/hashmap.mi:immut/hashmap' -i '$MOON_HOME/lib/core/target/js/release/bundle/immut/hashset/hashset.mi:immut/hashset' -i '$MOON_HOME/lib/core/target/js/release/bundle/immut/priority_queue/priority_queue.mi:immut/priority_queue' -i '$MOON_HOME/lib/core/target/js/release/bundle/immut/sorted_map/sorted_map.mi:immut/sorted_map' -i '$MOON_HOME/lib/core/target/js/release/bundle/immut/sorted_set/sorted_set.mi:immut/sorted_set' -i '$MOON_HOME/lib/core/target/js/release/bundle/int/int.mi:int' -i '$MOON_HOME/lib/core/target/js/release/bundle/int16/int16.mi:int16' -i '$MOON_HOME/lib/core/target/js/release/bundle/int64/int64.mi:int64' -i '$MOON_HOME/lib/core/target/js/release/bundle/json/json.mi:json' -i '$MOON_HOME/lib/core/target/js/release/bundle/list/list.mi:list' -i '$MOON_HOME/lib/core/target/js/release/bundle/math/math.mi:math' -i '$MOON_HOME/lib/core/target/js/release/bundle/option/option.mi:option' -i '$MOON_HOME/lib/core/target/js/release/bundle/prelude/prelude.mi:prelude' -i '$MOON_HOME/lib/core/target/js/release/bundle/priority_queue/priority_queue.mi:priority_queue' -i '$MOON_HOME/lib/core/target/js/release/bundle/queue/queue.mi:queue' -i '$MOON_HOME/lib/core/target/js/release/bundle/quickcheck/quickcheck.mi:quickcheck' -i '$MOON_HOME/lib/core/target/js/release/bundle/random/random.mi:random' -i '$MOON_HOME/lib/core/target/js/release/bundle/ref/ref.mi:ref' -i '$MOON_HOME/lib/core/target/js/release/bundle/result/result.mi:result' -i '$MOON_HOME/lib/core/target/js/release/bundle/set/set.mi:set' -i '$MOON_HOME/lib/core/target/js/release/bundle/sorted_map/sorted_map.mi:sorted_map' -i '$MOON_HOME/lib/core/target/js/release/bundle/sorted_set/sorted_set.mi:sorted_set' -i '$MOON_HOME/lib/core/target/js/release/bundle/quickcheck/splitmix/splitmix.mi:splitmix' -i '$MOON_HOME/lib/core/target/js/release/bundle/strconv/strconv.mi:strconv' -i '$MOON_HOME/lib/core/target/js/release/bundle/string/string.mi:string' -i '$MOON_HOME/lib/core/target/js/release/bundle/test/test.mi:test' -i '$MOON_HOME/lib/core/target/js/release/bundle/tuple/tuple.mi:tuple' -i '$MOON_HOME/lib/core/target/js/release/bundle/uint/uint.mi:uint' -i '$MOON_HOME/lib/core/target/js/release/bundle/uint16/uint16.mi:uint16' -i '$MOON_HOME/lib/core/target/js/release/bundle/uint64/uint64.mi:uint64' -i '$MOON_HOME/lib/core/target/js/release/bundle/unit/unit.mi:unit' -i '$MOON_HOME/lib/core/target/js/release/bundle/encoding/utf16/utf16.mi:utf16' -i '$MOON_HOME/lib/core/target/js/release/bundle/encoding/utf8/utf8.mi:utf8' -pkg-sources moon/test/single:. -target js -O0 -workspace-path . -all-pkgs ./_build/js/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/target/js/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/js/release/bundle/core.core' ./_build/js/debug/build/single/single.core -main moon/test/single -o ./_build/js/debug/build/single/single.js -pkg-config-path ./moon.pkg.json -pkg-sources moon/test/single:. -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target js -O0
            node ./_build/js/debug/build/single/single.js
        "#]],
    );

    let output = get_stdout(&dir, ["run", "a/b/single.mbt", "--dry-run"]);
    check(
        &output,
        expect![[r#"
            moonc build-package ./single.mbt -o ./_build/wasm-gc/debug/build/single/single.core -pkg moon/test/single -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.mi:abort' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/array/array.mi:array' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/bench/bench.mi:bench' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/bigint/bigint.mi:bigint' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/bool/bool.mi:bool' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/buffer/buffer.mi:buffer' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/builtin/builtin.mi:builtin' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/byte/byte.mi:byte' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/bytes/bytes.mi:bytes' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/char/char.mi:char' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/cmp/cmp.mi:cmp' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/coverage/coverage.mi:coverage' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/debug/debug.mi:debug' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/deque/deque.mi:deque' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/double/double.mi:double' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/env/env.mi:env' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/error/error.mi:error' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/float/float.mi:float' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/hashmap/hashmap.mi:hashmap' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/hashset/hashset.mi:hashset' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/immut/array/array.mi:immut/array' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/immut/hashmap/hashmap.mi:immut/hashmap' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/immut/hashset/hashset.mi:immut/hashset' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/immut/priority_queue/priority_queue.mi:immut/priority_queue' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/immut/sorted_map/sorted_map.mi:immut/sorted_map' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/immut/sorted_set/sorted_set.mi:immut/sorted_set' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/int/int.mi:int' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/int16/int16.mi:int16' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/int64/int64.mi:int64' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/json/json.mi:json' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/list/list.mi:list' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/math/math.mi:math' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/option/option.mi:option' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/priority_queue/priority_queue.mi:priority_queue' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/queue/queue.mi:queue' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/quickcheck/quickcheck.mi:quickcheck' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/random/random.mi:random' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/ref/ref.mi:ref' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/result/result.mi:result' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/set/set.mi:set' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/sorted_map/sorted_map.mi:sorted_map' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/sorted_set/sorted_set.mi:sorted_set' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/quickcheck/splitmix/splitmix.mi:splitmix' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/strconv/strconv.mi:strconv' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/string/string.mi:string' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/test/test.mi:test' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/tuple/tuple.mi:tuple' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/uint/uint.mi:uint' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/uint16/uint16.mi:uint16' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/uint64/uint64.mi:uint64' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/unit/unit.mi:unit' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/encoding/utf16/utf16.mi:utf16' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/encoding/utf8/utf8.mi:utf8' -pkg-sources moon/test/single:. -target wasm-gc -O0 -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/build/single/single.core -main moon/test/single -o ./_build/wasm-gc/debug/build/single/single.wasm -pkg-config-path ./moon.pkg.json -pkg-sources moon/test/single:. -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm-gc -O0
            moonrun ./_build/wasm-gc/debug/build/single/single.wasm --
        "#]],
    );

    let output = get_stdout(
        &dir,
        ["run", "a/b/single.mbt", "--target", "js", "--dry-run"],
    );
    check(
        &output,
        expect![[r#"
            moonc build-package ./single.mbt -o ./_build/js/debug/build/single/single.core -pkg moon/test/single -is-main -std-path '$MOON_HOME/lib/core/target/js/release/bundle' -i '$MOON_HOME/lib/core/target/js/release/bundle/abort/abort.mi:abort' -i '$MOON_HOME/lib/core/target/js/release/bundle/array/array.mi:array' -i '$MOON_HOME/lib/core/target/js/release/bundle/bench/bench.mi:bench' -i '$MOON_HOME/lib/core/target/js/release/bundle/bigint/bigint.mi:bigint' -i '$MOON_HOME/lib/core/target/js/release/bundle/bool/bool.mi:bool' -i '$MOON_HOME/lib/core/target/js/release/bundle/buffer/buffer.mi:buffer' -i '$MOON_HOME/lib/core/target/js/release/bundle/builtin/builtin.mi:builtin' -i '$MOON_HOME/lib/core/target/js/release/bundle/byte/byte.mi:byte' -i '$MOON_HOME/lib/core/target/js/release/bundle/bytes/bytes.mi:bytes' -i '$MOON_HOME/lib/core/target/js/release/bundle/char/char.mi:char' -i '$MOON_HOME/lib/core/target/js/release/bundle/cmp/cmp.mi:cmp' -i '$MOON_HOME/lib/core/target/js/release/bundle/coverage/coverage.mi:coverage' -i '$MOON_HOME/lib/core/target/js/release/bundle/debug/debug.mi:debug' -i '$MOON_HOME/lib/core/target/js/release/bundle/deque/deque.mi:deque' -i '$MOON_HOME/lib/core/target/js/release/bundle/double/double.mi:double' -i '$MOON_HOME/lib/core/target/js/release/bundle/env/env.mi:env' -i '$MOON_HOME/lib/core/target/js/release/bundle/error/error.mi:error' -i '$MOON_HOME/lib/core/target/js/release/bundle/float/float.mi:float' -i '$MOON_HOME/lib/core/target/js/release/bundle/hashmap/hashmap.mi:hashmap' -i '$MOON_HOME/lib/core/target/js/release/bundle/hashset/hashset.mi:hashset' -i '$MOON_HOME/lib/core/target/js/release/bundle/immut/array/array.mi:immut/array' -i '$MOON_HOME/lib/core/target/js/release/bundle/immut/hashmap/hashmap.mi:immut/hashmap' -i '$MOON_HOME/lib/core/target/js/release/bundle/immut/hashset/hashset.mi:immut/hashset' -i '$MOON_HOME/lib/core/target/js/release/bundle/immut/priority_queue/priority_queue.mi:immut/priority_queue' -i '$MOON_HOME/lib/core/target/js/release/bundle/immut/sorted_map/sorted_map.mi:immut/sorted_map' -i '$MOON_HOME/lib/core/target/js/release/bundle/immut/sorted_set/sorted_set.mi:immut/sorted_set' -i '$MOON_HOME/lib/core/target/js/release/bundle/int/int.mi:int' -i '$MOON_HOME/lib/core/target/js/release/bundle/int16/int16.mi:int16' -i '$MOON_HOME/lib/core/target/js/release/bundle/int64/int64.mi:int64' -i '$MOON_HOME/lib/core/target/js/release/bundle/json/json.mi:json' -i '$MOON_HOME/lib/core/target/js/release/bundle/list/list.mi:list' -i '$MOON_HOME/lib/core/target/js/release/bundle/math/math.mi:math' -i '$MOON_HOME/lib/core/target/js/release/bundle/option/option.mi:option' -i '$MOON_HOME/lib/core/target/js/release/bundle/prelude/prelude.mi:prelude' -i '$MOON_HOME/lib/core/target/js/release/bundle/priority_queue/priority_queue.mi:priority_queue' -i '$MOON_HOME/lib/core/target/js/release/bundle/queue/queue.mi:queue' -i '$MOON_HOME/lib/core/target/js/release/bundle/quickcheck/quickcheck.mi:quickcheck' -i '$MOON_HOME/lib/core/target/js/release/bundle/random/random.mi:random' -i '$MOON_HOME/lib/core/target/js/release/bundle/ref/ref.mi:ref' -i '$MOON_HOME/lib/core/target/js/release/bundle/result/result.mi:result' -i '$MOON_HOME/lib/core/target/js/release/bundle/set/set.mi:set' -i '$MOON_HOME/lib/core/target/js/release/bundle/sorted_map/sorted_map.mi:sorted_map' -i '$MOON_HOME/lib/core/target/js/release/bundle/sorted_set/sorted_set.mi:sorted_set' -i '$MOON_HOME/lib/core/target/js/release/bundle/quickcheck/splitmix/splitmix.mi:splitmix' -i '$MOON_HOME/lib/core/target/js/release/bundle/strconv/strconv.mi:strconv' -i '$MOON_HOME/lib/core/target/js/release/bundle/string/string.mi:string' -i '$MOON_HOME/lib/core/target/js/release/bundle/test/test.mi:test' -i '$MOON_HOME/lib/core/target/js/release/bundle/tuple/tuple.mi:tuple' -i '$MOON_HOME/lib/core/target/js/release/bundle/uint/uint.mi:uint' -i '$MOON_HOME/lib/core/target/js/release/bundle/uint16/uint16.mi:uint16' -i '$MOON_HOME/lib/core/target/js/release/bundle/uint64/uint64.mi:uint64' -i '$MOON_HOME/lib/core/target/js/release/bundle/unit/unit.mi:unit' -i '$MOON_HOME/lib/core/target/js/release/bundle/encoding/utf16/utf16.mi:utf16' -i '$MOON_HOME/lib/core/target/js/release/bundle/encoding/utf8/utf8.mi:utf8' -pkg-sources moon/test/single:. -target js -O0 -workspace-path . -all-pkgs ./_build/js/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/target/js/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/js/release/bundle/core.core' ./_build/js/debug/build/single/single.core -main moon/test/single -o ./_build/js/debug/build/single/single.js -pkg-config-path ./moon.pkg.json -pkg-sources moon/test/single:. -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target js -O0
            node ./_build/js/debug/build/single/single.js
        "#]],
    );

    let output = get_stdout(
        &dir,
        ["run", "a/b/single.mbt", "--target", "js", "--build-only"],
    );
    check(
        &output,
        expect![[r#"
            {"artifacts_path":["$ROOT/a/b/_build/js/debug/build/single/single.js"]}
        "#]],
    );
    assert!(
        dir.join("a/b/target/js/debug/build/single/single.js")
            .exists()
    );
}

#[test]
fn test_moon_run_single_mbt_file() {
    let dir = TestDir::new("run_single_mbt_file.in");

    let output = get_stdout(&dir, ["run", "a/b/single.mbt"]);
    check(
        &output,
        expect![[r#"
        I am OK
    "#]],
    );

    let output = get_stdout(&dir.join("a").join("b").join("c"), ["run", "../single.mbt"]);
    check(
        &output,
        expect![[r#"
            I am OK
            "#]],
    );

    let output = get_stdout(
        &dir.join("a").join("b"),
        ["run", "single.mbt", "--target", "js"],
    );
    check(
        &output,
        expect![[r#"
        I am OK
        "#]],
    );

    let output = get_stdout(
        &dir.join("a").join("b"),
        ["run", "single.mbt", "--target", "native"],
    );
    // cl have other output
    assert!(output.contains("I am OK"));
}

#[test]
fn test_moon_run_single_mbt_file_inside_a_pkg() {
    let dir = TestDir::new("run_single_mbt_file_inside_pkg.in");

    let output = get_stdout(&dir, ["run", "main/main.mbt"]);
    check(
        &output,
        expect![[r#"
            Hello, world!!!
            root main
        "#]],
    );
    let output = get_stdout(&dir, ["run", "lib/main_in_lib/main.mbt"]);
    check(
        &output,
        expect![[r#"
            Hello, world!!!
            main in lib
        "#]],
    );

    let output = get_stdout(&dir.join("lib"), ["run", "../main/main.mbt"]);
    check(
        &output,
        expect![[r#"
            Hello, world!!!
            root main
        "#]],
    );
    let output = get_stdout(&dir.join("lib"), ["run", "main_in_lib/main.mbt"]);
    check(
        &output,
        expect![[r#"
            Hello, world!!!
            main in lib
        "#]],
    );

    let output = get_stdout(
        &dir.join("lib").join("main_in_lib"),
        ["run", "../../main/main.mbt"],
    );
    check(
        &output,
        expect![[r#"
            Hello, world!!!
            root main
        "#]],
    );
    let output = get_stdout(&dir.join("lib").join("main_in_lib"), ["run", "main.mbt"]);
    check(
        &output,
        expect![[r#"
            Hello, world!!!
            main in lib
        "#]],
    );

    let output = get_stdout(
        &dir.join("lib").join("main_in_lib"),
        ["run", "../../main/main.mbt", "--target", "native"],
    );
    check(
        &output,
        expect![[r#"
            Hello, world!!!
            root main
        "#]],
    );
    let output = get_stdout(
        &dir.join("lib").join("main_in_lib"),
        ["run", "main.mbt", "--target", "native"],
    );
    check(
        &output,
        expect![[r#"
            Hello, world!!!
            main in lib
        "#]],
    );
}

#[test]
fn test_specify_source_dir_003() {
    let dir = TestDir::new("specify_source_dir_003_empty_string.in");
    check(
        get_stderr(&dir, ["check"]),
        expect![[r#"
            Finished. moon: ran 2 tasks, now up to date
        "#]],
    );
}

#[test]
fn test_specify_source_dir_004() {
    let dir = TestDir::new("specify_source_dir_004.in");
    check(
        get_stderr(&dir, ["check"]),
        expect![[r#"
            Finished. moon: ran 4 tasks, now up to date
        "#]],
    );

    get_stdout(&dir, ["clean"]);
    check(
        get_stdout(
            &dir,
            ["run", "nes/t/ed/src/main", "--target", "js", "--build-only"],
        ),
        expect![[r#"
            {"artifacts_path":["$ROOT/_build/js/release/build/main/main.js"]}
        "#]],
    );
    assert!(dir.join("target/js/release/build/main/main.js").exists());

    check(
        get_stdout(&dir, ["run", "nes/t/ed/src/main"]),
        expect![[r#"
            Hello, world!
        "#]],
    );
}

#[test]
fn test_specify_source_dir_005() {
    let dir = TestDir::new("specify_source_dir_005_bad.in");
    let check_stderr = get_err_stderr(&dir, ["check"]);
    assert!(check_stderr.contains("`source` not a subdirectory of the parent directory"));
}

#[test]
fn test_specify_source_dir_with_deps() {
    let dir = TestDir::new("specify_source_dir_with_deps_001.in");
    let check_graph = dir.join("check_graph.jsonl");
    snap_dry_run_graph(&dir, ["check", "--dry-run", "--sort-input"], &check_graph);
    compare_graphs(
        &check_graph,
        expect_file!["./specify_source_dir_with_deps_001.in/check_graph.jsonl.snap"],
    );
    let test_graph = dir.join("test_graph.jsonl");
    snap_dry_run_graph(&dir, ["test", "--dry-run", "--sort-input"], &test_graph);
    compare_graphs(
        &test_graph,
        expect_file!["./specify_source_dir_with_deps_001.in/test_graph.jsonl.snap"],
    );

    check(
        get_stderr(&dir, ["check"]),
        expect![[r#"
            Finished. moon: ran 6 tasks, now up to date
        "#]],
    );
    check(
        get_stderr(&dir, ["build"]),
        expect![[r#"
            Finished. moon: ran 5 tasks, now up to date
        "#]],
    );
    check(
        get_stdout(&dir, ["test"]),
        expect![[r#"
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    check(
        get_stdout(&dir, ["run", "./anyhow/main"]),
        expect![[r#"
            Hello, world!
            hello
            world
        "#]],
    );
}

#[test]
fn test_specify_source_dir_with_deps_002() {
    let dir = TestDir::new("specify_source_dir_with_deps_002.in");
    check(
        get_stderr(&dir, ["check"]),
        expect![[r#"
             WARN Duplicate alias `lib` at "$ROOT/deps/hello004/lib/moon.pkg.json". "test-import" will automatically add "import" and current package as dependency so you don't need to add it manually. If you're test-importing a dependency with the same default alias as your current package, considering give it a different alias than the current package. Violating import: `just/hello003/lib`
             WARN Duplicate alias `lib` at "$ROOT/deps/hello003/source003/lib/moon.pkg.json". "test-import" will automatically add "import" and current package as dependency so you don't need to add it manually. If you're test-importing a dependency with the same default alias as your current package, considering give it a different alias than the current package. Violating import: `just/hello002/lib`
             WARN Duplicate alias `lib` at "$ROOT/deps/hello002/lib/moon.pkg.json". "test-import" will automatically add "import" and current package as dependency so you don't need to add it manually. If you're test-importing a dependency with the same default alias as your current package, considering give it a different alias than the current package. Violating import: `just/hello001/lib`
            Finished. moon: ran 10 tasks, now up to date
        "#]],
    );
    check(
        get_stderr(&dir, ["build"]),
        expect![[r#"
             WARN Duplicate alias `lib` at "$ROOT/deps/hello004/lib/moon.pkg.json". "test-import" will automatically add "import" and current package as dependency so you don't need to add it manually. If you're test-importing a dependency with the same default alias as your current package, considering give it a different alias than the current package. Violating import: `just/hello003/lib`
             WARN Duplicate alias `lib` at "$ROOT/deps/hello003/source003/lib/moon.pkg.json". "test-import" will automatically add "import" and current package as dependency so you don't need to add it manually. If you're test-importing a dependency with the same default alias as your current package, considering give it a different alias than the current package. Violating import: `just/hello002/lib`
             WARN Duplicate alias `lib` at "$ROOT/deps/hello002/lib/moon.pkg.json". "test-import" will automatically add "import" and current package as dependency so you don't need to add it manually. If you're test-importing a dependency with the same default alias as your current package, considering give it a different alias than the current package. Violating import: `just/hello001/lib`
            Finished. moon: ran 10 tasks, now up to date
        "#]],
    );
    check(
        get_stdout(&dir, ["test"]),
        expect![[r#"
        Total tests: 0, passed: 0, failed: 0.
    "#]],
    );
    check(
        get_stdout(&dir, ["run", "./anyhow"]),
        expect![[r#"
            a!b!c!d!
            one!two!three!four!
        "#]],
    );
}

#[test]
fn moon_test_with_failure_json() {
    let dir = TestDir::new("test_with_failure_json");

    let output = get_err_stdout(&dir, ["test", "--test-failure-json"]);
    check(
        &output,
        // should keep in this format, it's used in ide test explorer
        expect![[r#"
            {"package":"username/hello/lib1","filename":"hello.mbt","index":"0","test_name":"test_1","message":"lib1/hello.mbt:7:3-7:24@username/hello FAILED: test_1 failed"}
            Total tests: 2, passed: 1, failed: 1.
        "#]],
    );
}

#[test]
fn test_js() {
    let dir = TestDir::new("test_filter/test_filter");

    let output = get_stdout(
        &dir,
        [
            "test",
            "-p",
            "username/hello/lib",
            "--target",
            "js",
            "--sort-input",
            "--no-parallelize",
        ],
    );
    check(
        &output,
        expect![[r#"
            test A
            test hello_0
            test hello_1
            Total tests: 3, passed: 3, failed: 0.
        "#]],
    );

    let output = get_stdout(
        &dir,
        [
            "test",
            "-p",
            "username/hello/lib",
            "-f",
            "hello_wbtest.mbt",
            "-i",
            "1",
            "--target",
            "js",
            "--sort-input",
            "--no-parallelize",
        ],
    );
    check(
        &output,
        expect![[r#"
            test hello_1
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_doc_dry_run() {
    let dir = TestDir::new("moon_doc.in");
    check(
        get_stdout(&dir, ["doc", "--dry-run"]),
        expect![[r#"
            moonc check ./src/lib/hello.mbt -o ./_build/wasm-gc/release/check/lib/lib.mi -pkg username/hello/lib -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./src/lib -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check ./src/main/main.mbt -o ./_build/wasm-gc/release/check/main/main.mi -pkg username/hello/main -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/lib/lib.mi:lib -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main:./src/main -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moondoc . -o ./_build/doc -std-path '$MOON_HOME/lib/core' -packages-json ./_build/packages.json
        "#]],
    );
}

#[test]
fn test_moon_doc() {
    let dir = TestDir::new("moon_doc.in");
    let _ = get_stderr(&dir, ["doc"]);
    check(
        read(dir.join("target/doc/username/hello/lib/members.md")),
        expect![[r#"
            # Documentation
            |Value|description|
            |---|---|
            |[hello](#hello)||

            ## hello

            ```moonbit
            :::source,username/hello/lib/hello.mbt,1:::fn hello() -> String
            ```

        "#]],
    );
    check(
        read(dir.join("target/doc/username/hello/main/members.md")),
        expect!["# Documentation"],
    );
    check(
        read(dir.join("target/doc/username/hello/_sidebar.md")),
        expect![[r#"
            - [username/hello](username/hello/)
            - **In this module**
              - [lib](username/hello/lib/members)
              - [main](username/hello/main/members)
            - **Dependencies**
              - [moonbitlang/core](moonbitlang/core/)"#]],
    );
}

#[test]
fn test_failed_to_fill_whole_buffer() {
    // TODO: Do we really need to test about database corruption?!

    let dir = TestDir::new("hello");
    check(
        get_stderr(&dir, ["check"]),
        expect![[r#"
            Finished. moon: ran 2 tasks, now up to date
        "#]],
    );

    // corrupt the DB intentionally
    let moon_db_path = dir.join("./target/wasm-gc/release/check/check.moon_db");
    if moon_db_path.exists() {
        std::fs::remove_file(&moon_db_path).unwrap();
    }
    std::fs::write(&moon_db_path, "").unwrap();

    let stderr = get_err_stderr(&dir, ["check"]);
    println!("stderr: {}", stderr);
    assert!(stderr.contains("failed to fill whole buffer"));
}

#[test]
fn test_moon_update_failed() {
    if std::env::var("CI").is_err() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let moon_home = dir;
    let out = std::process::Command::new(moon_bin())
        .current_dir(dir)
        .env("MOON_HOME", moon_home)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .args(["update"])
        .output()
        .unwrap();
    let out = String::from_utf8(out.stderr).unwrap();
    check(
        &out,
        expect![[r#"
            Registry index cloned successfully
            Symbols updated successfully
        "#]],
    );

    let _ = std::process::Command::new("git")
        .args([
            "-C",
            dir.join("registry").join("index").to_str().unwrap(),
            "remote",
            "set-url",
            "origin",
            "whatever",
        ])
        .output()
        .unwrap();

    let out = std::process::Command::new(moon_bin())
        .current_dir(dir)
        .env("MOON_HOME", moon_home)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .args(["update"])
        .output()
        .unwrap();
    let out = String::from_utf8(out.stderr).unwrap();
    check(
        &out,
        expect![[r#"
            Registry index is not cloned from the same URL, re-cloning
            Registry index re-cloned successfully
            Symbols updated successfully
        "#]],
    );
}

#[test]
fn test_trace_001() {
    let dir = TestDir::new("hello");
    let _ = get_stdout(&dir, ["build", "--trace"]);
    assert!(dir.join("trace.json").exists());
}

#[test]
fn no_main_just_init() {
    let dir = TestDir::new("no_main_just_init.in");
    get_stdout(&dir, ["build"]);
    let file = dir.join("target/wasm-gc/release/build/lib/lib.wasm");
    assert!(file.exists());

    let out = snapbox::cmd::Command::new("moonrun")
        .current_dir(&dir)
        .args(["./target/wasm-gc/release/build/lib/lib.wasm"])
        .assert()
        .success()
        .get_output()
        .stdout
        .to_owned();

    check(
        std::str::from_utf8(&out).unwrap(),
        expect![[r#"
            I am in fn init { ... }
        "#]],
    );
}

#[test]
fn test_pre_build() {
    let dir = TestDir::new("pre_build.in");

    // replace CRLF with LF on Windows
    let b_txt_path = dir.join("src/lib/b.txt");
    std::fs::write(&b_txt_path, read(&b_txt_path)).unwrap();

    // Assert that prebuilt files didn't exist
    assert!(
        !dir.join("src/lib/a.mbt").exists(),
        "Prebuilt file should not exist before execution"
    );

    get_stderr(&dir, ["check"]);
    // should successfully execute

    check(
        read(dir.join("src/lib/a.mbt")),
        expect![[r#"
            // Generated by `moon tool embed --text`, do not edit.

            ///|
            let resource : String =
              #|hello,
              #|world
              #|
        "#]],
    );
    let content = read(dir.join("src/lib/b.mbt"));
    check(
        content,
        expect![[r#"
            // Generated by `moon tool embed --binary`, do not edit.

            ///|
            let _b : Bytes = [
              0x4d, 0x6f, 0x6f, 0x6e, 0x42, 0x69, 0x74, 0x20, 0x69, 0x73, 0x20, 0x61, 
              0x6e, 0x20, 0x65, 0x6e, 0x64, 0x2d, 0x74, 0x6f, 0x2d, 0x65, 0x6e, 0x64, 
              0x20, 0x70, 0x72, 0x6f, 0x67, 0x72, 0x61, 0x6d, 0x6d, 0x69, 0x6e, 0x67, 
              0x20, 0x6c, 0x61, 0x6e, 0x67, 0x75, 0x61, 0x67, 0x65, 0x20, 0x74, 0x6f, 
              0x6f, 0x6c, 0x63, 0x68, 0x61, 0x69, 0x6e, 0x20, 0x66, 0x6f, 0x72, 0x20, 
              0x63, 0x6c, 0x6f, 0x75, 0x64, 0x20, 0x61, 0x6e, 0x64, 0x20, 0x65, 0x64, 
              0x67, 0x65, 0x0a, 0x63, 0x6f, 0x6d, 0x70, 0x75, 0x74, 0x69, 0x6e, 0x67, 
              0x20, 0x75, 0x73, 0x69, 0x6e, 0x67, 0x20, 0x57, 0x65, 0x62, 0x41, 0x73, 
              0x73, 0x65, 0x6d, 0x62, 0x6c, 0x79, 0x2e, 0x20, 0x54, 0x68, 0x65, 0x20, 
              0x49, 0x44, 0x45, 0x20, 0x65, 0x6e, 0x76, 0x69, 0x72, 0x6f, 0x6e, 0x6d, 
              0x65, 0x6e, 0x74, 0x20, 0x69, 0x73, 0x20, 0x61, 0x76, 0x61, 0x69, 0x6c, 
              0x61, 0x62, 0x6c, 0x65, 0x20, 0x61, 0x74, 0x0a, 0x68, 0x74, 0x74, 0x70, 
              0x73, 0x3a, 0x2f, 0x2f, 0x74, 0x72, 0x79, 0x2e, 0x6d, 0x6f, 0x6f, 0x6e, 
              0x62, 0x69, 0x74, 0x6c, 0x61, 0x6e, 0x67, 0x2e, 0x63, 0x6f, 0x6d, 0x20, 
              0x77, 0x69, 0x74, 0x68, 0x6f, 0x75, 0x74, 0x20, 0x61, 0x6e, 0x79, 0x20, 
              0x69, 0x6e, 0x73, 0x74, 0x61, 0x6c, 0x6c, 0x61, 0x74, 0x69, 0x6f, 0x6e, 
              0x3b, 0x20, 0x69, 0x74, 0x20, 0x64, 0x6f, 0x65, 0x73, 0x20, 0x6e, 0x6f, 
              0x74, 0x20, 0x72, 0x65, 0x6c, 0x79, 0x20, 0x6f, 0x6e, 0x20, 0x61, 0x6e, 
              0x79, 0x0a, 0x73, 0x65, 0x72, 0x76, 0x65, 0x72, 0x20, 0x65, 0x69, 0x74, 
              0x68, 0x65, 0x72, 0x2e, 
            ]
        "#]],
    );
    check(
        read(dir.join("src/lib/c.mbt")),
        expect![[r#"
            // Generated by `moon tool embed --text`, do not edit.

            ///|
            let _c : String =
              #|hello,
              #|world
              #|
        "#]],
    );
}

#[test]
fn test_bad_version() {
    let dir = TestDir::new("general.in");
    let content = std::fs::read_to_string(dir.join("moon.mod.json")).unwrap();
    let mut moon_mod: MoonModJSON = serde_json::from_str(&content).unwrap();
    moon_mod.version = Some("0.0".to_string());
    std::fs::write(
        dir.join("moon.mod.json"),
        serde_json::to_string(&moon_mod).unwrap(),
    )
    .unwrap();

    let check_stderr = get_err_stderr(&dir, ["check"]);
    println!("{}", check_stderr);
    assert!(check_stderr.contains("`version` bad format"));
}

#[test]
fn test_moonfmt() {
    let dir = TestDir::new("general.in");
    let oneline = r#"pub fn hello() -> String { "Hello, world!" }"#;

    std::fs::write(dir.join("src/lib/hello.mbt"), oneline).unwrap();

    let out = std::process::Command::new("moonfmt")
        .args(["./src/lib/hello.mbt"])
        .current_dir(&dir)
        .output()
        .unwrap();
    let out = String::from_utf8(out.stdout).unwrap().replace_crlf_to_lf();
    check(
        &out,
        expect![[r#"
            ///|
            pub fn hello() -> String {
              "Hello, world!"
            }
        "#]],
    );

    check(
        read(dir.join("src/lib/hello.mbt")),
        expect![[r#"pub fn hello() -> String { "Hello, world!" }"#]],
    );

    let out = std::process::Command::new("moonfmt")
        .args(["-i", "./src/lib/hello.mbt"])
        .current_dir(&dir)
        .output()
        .unwrap();
    let _ = String::from_utf8(out.stdout).unwrap();
    check(
        read(dir.join("src/lib/hello.mbt")),
        expect![[r#"
            ///|
            pub fn hello() -> String {
              "Hello, world!"
            }
        "#]],
    );

    std::fs::write(dir.join("src/lib/hello.mbt"), oneline).unwrap();
    check(
        read(dir.join("src/lib/hello.mbt")),
        expect![[r#"pub fn hello() -> String { "Hello, world!" }"#]],
    );

    let out = std::process::Command::new("moonfmt")
        .args(["-i", "./src/lib/hello.mbt", "-o", "./src/lib/hello.txt"])
        .current_dir(&dir)
        .output()
        .unwrap();
    let _ = String::from_utf8(out.stdout).unwrap();
    check(
        read(dir.join("src/lib/hello.mbt")),
        expect![[r#"
            ///|
            pub fn hello() -> String {
              "Hello, world!"
            }
        "#]],
    );
    check(
        read(dir.join("src/lib/hello.txt")),
        expect![[r#"
            ///|
            pub fn hello() -> String {
              "Hello, world!"
            }
        "#]],
    );
}

#[test]
fn test_moon_package_list() {
    let dir = TestDir::new("test_publish.in");
    check(
        get_stderr(&dir, ["package", "--list"]),
        expect![[r#"
            Running moon check ...
            Finished. moon: ran 4 tasks, now up to date
            Check passed
            README.md
            moon.mod.json
            src
            src/lib
            src/lib/hello.mbt
            src/lib/hello_test.mbt
            src/lib/moon.pkg.json
            src/main
            src/main/main.mbt
            src/main/moon.pkg.json
            Package to $ROOT/_build/publish/username-hello-0.1.0.zip
        "#]],
    );
}

#[test]
fn test_moon_check_filter_package() {
    let dir = TestDir::new("test_check_filter.in");

    check(
        get_stdout(&dir, ["check", "-p", "A", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc check ./A/hello.mbt ./A/test.mbt ./A/hello_wbtest.mbt -o ./_build/wasm-gc/release/check/A/A.whitebox_test.mi -pkg username/hello/A -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/A:./A -target wasm-gc -whitebox-test -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check ./A/hello.mbt ./A/test.mbt -o ./_build/wasm-gc/release/check/A/A.mi -pkg username/hello/A -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/A:./A -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check ./A/hello_test.mbt -doctest-only ./A/hello.mbt -doctest-only ./A/test.mbt -include-doctests -o ./_build/wasm-gc/release/check/A/A.blackbox_test.mi -pkg username/hello/A_blackbox_test -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/A/A.mi:A -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/A_blackbox_test:./A -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
        "#]],
    );

    check(
        get_stdout(&dir, ["check", "-p", "main", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc check ./lib2/lib.mbt -o ./_build/wasm-gc/release/check/lib2/lib2.mi -pkg username/hello/lib2 -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib2:./lib2 -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check ./lib/hello.mbt -o ./_build/wasm-gc/release/check/lib/lib.mi -pkg username/hello/lib -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/lib2/lib2.mi:lib2 -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check ./main/main.mbt -o ./_build/wasm-gc/release/check/main/main.mi -pkg username/hello/main -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/lib/lib.mi:lib -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main:./main -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check -doctest-only ./main/main.mbt -include-doctests -o ./_build/wasm-gc/release/check/main/main.blackbox_test.mi -pkg username/hello/main_blackbox_test -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/lib/lib.mi:lib -i ./_build/wasm-gc/release/check/main/main.mi:main -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main_blackbox_test:./main -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
        "#]],
    );

    check(
        get_stdout(&dir, ["check", "-p", "lib", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc check ./lib2/lib.mbt -o ./_build/wasm-gc/release/check/lib2/lib2.mi -pkg username/hello/lib2 -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib2:./lib2 -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check ./lib/hello.mbt -o ./_build/wasm-gc/release/check/lib/lib.mi -pkg username/hello/lib -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/lib2/lib2.mi:lib2 -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check -doctest-only ./lib/hello.mbt -include-doctests -o ./_build/wasm-gc/release/check/lib/lib.blackbox_test.mi -pkg username/hello/lib_blackbox_test -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/lib/lib.mi:lib -i ./_build/wasm-gc/release/check/lib2/lib2.mi:lib2 -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib_blackbox_test:./lib -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
        "#]],
    );
}

#[test]
fn test_moon_check_package_with_patch() {
    let dir = TestDir::new("test_check_filter.in");

    // A has no deps
    check(
        get_stdout(
            &dir,
            [
                "check",
                "-p",
                "A",
                "--patch-file",
                "/path/to/patch.json",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc check ./A/hello.mbt ./A/test.mbt ./A/hello_wbtest.mbt -o ./_build/wasm-gc/release/check/A/A.whitebox_test.mi -pkg username/hello/A -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/A:./A -target wasm-gc -whitebox-test -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check -patch-file /path/to/patch.json ./A/hello.mbt ./A/test.mbt -o ./_build/wasm-gc/release/check/A/A.mi -pkg username/hello/A -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/A:./A -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check ./A/hello_test.mbt -doctest-only ./A/hello.mbt -doctest-only ./A/test.mbt -include-doctests -o ./_build/wasm-gc/release/check/A/A.blackbox_test.mi -pkg username/hello/A_blackbox_test -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/A/A.mi:A -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/A_blackbox_test:./A -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "check",
                "-p",
                "A",
                "--patch-file",
                "/path/to/patch_wbtest.json",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc check -patch-file /path/to/patch_wbtest.json ./A/hello.mbt ./A/test.mbt ./A/hello_wbtest.mbt -o ./_build/wasm-gc/release/check/A/A.whitebox_test.mi -pkg username/hello/A -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/A:./A -target wasm-gc -whitebox-test -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check ./A/hello.mbt ./A/test.mbt -o ./_build/wasm-gc/release/check/A/A.mi -pkg username/hello/A -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/A:./A -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check ./A/hello_test.mbt -doctest-only ./A/hello.mbt -doctest-only ./A/test.mbt -include-doctests -o ./_build/wasm-gc/release/check/A/A.blackbox_test.mi -pkg username/hello/A_blackbox_test -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/A/A.mi:A -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/A_blackbox_test:./A -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "check",
                "-p",
                "A",
                "--patch-file",
                "/path/to/patch_test.json",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc check ./A/hello.mbt ./A/test.mbt ./A/hello_wbtest.mbt -o ./_build/wasm-gc/release/check/A/A.whitebox_test.mi -pkg username/hello/A -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/A:./A -target wasm-gc -whitebox-test -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check ./A/hello.mbt ./A/test.mbt -o ./_build/wasm-gc/release/check/A/A.mi -pkg username/hello/A -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/A:./A -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check -patch-file /path/to/patch_test.json ./A/hello_test.mbt -doctest-only ./A/hello.mbt -doctest-only ./A/test.mbt -include-doctests -o ./_build/wasm-gc/release/check/A/A.blackbox_test.mi -pkg username/hello/A_blackbox_test -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/A/A.mi:A -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/A_blackbox_test:./A -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
        "#]],
    );

    // lib has dep lib2
    check(
        get_stdout(
            &dir,
            [
                "check",
                "-p",
                "lib",
                "--patch-file",
                "/path/to/patch.json",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc check ./lib2/lib.mbt -o ./_build/wasm-gc/release/check/lib2/lib2.mi -pkg username/hello/lib2 -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib2:./lib2 -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check -patch-file /path/to/patch.json ./lib/hello.mbt -o ./_build/wasm-gc/release/check/lib/lib.mi -pkg username/hello/lib -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/lib2/lib2.mi:lib2 -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check -doctest-only ./lib/hello.mbt -include-doctests -o ./_build/wasm-gc/release/check/lib/lib.blackbox_test.mi -pkg username/hello/lib_blackbox_test -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/lib/lib.mi:lib -i ./_build/wasm-gc/release/check/lib2/lib2.mi:lib2 -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib_blackbox_test:./lib -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "check",
                "-p",
                "lib",
                "--patch-file",
                "/path/to/patch_test.json",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc check ./lib2/lib.mbt -o ./_build/wasm-gc/release/check/lib2/lib2.mi -pkg username/hello/lib2 -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib2:./lib2 -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check ./lib/hello.mbt -o ./_build/wasm-gc/release/check/lib/lib.mi -pkg username/hello/lib -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/lib2/lib2.mi:lib2 -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check -patch-file /path/to/patch_test.json -doctest-only ./lib/hello.mbt -include-doctests -o ./_build/wasm-gc/release/check/lib/lib.blackbox_test.mi -pkg username/hello/lib_blackbox_test -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/lib/lib.mi:lib -i ./_build/wasm-gc/release/check/lib2/lib2.mi:lib2 -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib_blackbox_test:./lib -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
        "#]],
    );

    // main has dep lib
    check(
        get_stdout(
            &dir,
            [
                "check",
                "-p",
                "main",
                "--patch-file",
                "/path/to/patch.json",
                "--no-mi",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc check ./lib2/lib.mbt -o ./_build/wasm-gc/release/check/lib2/lib2.mi -pkg username/hello/lib2 -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib2:./lib2 -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check ./lib/hello.mbt -o ./_build/wasm-gc/release/check/lib/lib.mi -pkg username/hello/lib -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/lib2/lib2.mi:lib2 -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check -patch-file /path/to/patch.json -no-mi ./main/main.mbt -pkg username/hello/main -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/lib/lib.mi:lib -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main:./main -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check -no-mi -doctest-only ./main/main.mbt -include-doctests -pkg username/hello/main_blackbox_test -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/lib/lib.mi:lib -i ./_build/wasm-gc/release/check/main/main.mi:main -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main_blackbox_test:./main -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
        "#]],
    );
}

#[test]
fn test_no_mi_for_test_pkg() {
    let dir = TestDir::new("test_check_filter.in");

    get_stdout(&dir, ["test", "-p", "username/hello/A"]);

    // .mi should not be generated for test package
    let mi_path = dir.join("target/wasm-gc/debug/test/A/A.internal_test.mi");
    assert!(!mi_path.exists());

    // .core should be generated for test package
    let core_path = dir.join("target/wasm-gc/debug/test/A/A.internal_test.core");
    assert!(core_path.exists());
}

#[test]
fn test_render_diagnostic_in_patch_file() {
    let dir = TestDir::new("moon_test/patch");
    check(
        get_stderr(
            &dir,
            ["check", "-p", "lib", "--patch-file", "./patch_test.json"],
        ),
        expect![[r#"
            Warning: [0002]
               ╭─[ hello_2_test.mbt:2:6 ]
               │
             2 │  let unused_in_patch_test_json = 1;
               │      ────────────┬────────────  
               │                  ╰────────────── Warning (unused_value): Unused variable 'unused_in_patch_test_json'
            ───╯
            Finished. moon: ran 3 tasks, now up to date (1 warnings, 0 errors)
        "#]],
    );
    check(
        get_stderr(
            &dir,
            ["check", "-p", "lib", "--patch-file", "./patch_wbtest.json"],
        ),
        expect![[r#"
            Warning: [0002]
               ╭─[ hello_1_wbtest.mbt:2:6 ]
               │
             2 │  let unused_in_patch_wbtest_json = 1;
               │      ─────────────┬─────────────  
               │                   ╰─────────────── Warning (unused_value): Unused variable 'unused_in_patch_wbtest_json'
            ───╯
            Finished. moon: ran 2 tasks, now up to date (1 warnings, 0 errors)
        "#]],
    );
    check(
        get_stderr(&dir, ["check", "-p", "lib", "--patch-file", "./patch.json"]),
        expect![[r#"
            Warning: [0002]
               ╭─[ hello_0.mbt:2:6 ]
               │
             2 │  let unused_in_patch_json = 1;
               │      ──────────┬─────────  
               │                ╰─────────── Warning (unused_value): Unused variable 'unused_in_patch_json'
            ───╯
            Finished. moon: ran 2 tasks, now up to date (1 warnings, 0 errors)
        "#]],
    );

    // check --explain
    check(
        get_stderr(
            &dir,
            [
                "check",
                "-p",
                "lib",
                "--patch-file",
                "./patch_test.json",
                "--explain",
            ],
        ),
        expect![[r#"
            Warning: 
               ╭─[ hello_2_test.mbt:2:6 ]
               │
             2 │  let unused_in_patch_test_json = 1;
               │      ────────────┬────────────  
               │                  ╰────────────── Warning (unused_value): Unused variable 'unused_in_patch_test_json'
               │ 
               │ Help: # E0002
               │       
               │       Warning name: `unused_value`
               │       
               │       Unused variable.
               │       
               │       This variable is unused by any other part of your code, nor marked with `pub`
               │       visibility.
               │       
               │       Note that this warning might uncover other bugs in your code. For example, if
               │       there are two variables in your codebase that has similar name, you might just
               │       use the other variable by mistake.
               │       
               │       Specifically, if the variable is at the toplevel, and the body of the module
               │       contains side effects, the side effects will not happen.
               │       
               │       ## Erroneous example
               │       
               │       ```moonbit
               │       ///|
               │       let p : Int = {
               │         side_effect.val = 42
               │         42
               │       }
               │       
               │       ///|
               │       let side_effect : Ref[Int] = { val: 0 }
               │       
               │       ///|
               │       test {
               │         let x = 42
               │       
               │       }
               │       ```
               │       
               │       ## Suggestion
               │       
               │       There are multiple ways to fix this warning:
               │       
               │       - If the variable is indeed useless, you can remove the definition of the
               │         variable.
               │       - If this variable is at the toplevel (i.e., not local), and is part of the
               │         public API of your module, you can add the `pub` keyword to the variable.
               │         ```moonbit
               │       
               │         ///|
               │         pub let p = 42
               │         ```
               │       - If you made a typo in the variable name, you can rename the variable to the
               │         correct name at the use site.
               │       - If your code depends on the side-effect of the variable, you can wrap the
               │         side-effect in a `fn init` block.
               │         ```moonbit
               │       
               │         ///|
               │         let side_effect : Ref[Int] = { val: 0 }
               │       
               │         ///|
               │         fn init {
               │           side_effect.val = 42
               │         }
               │         ```
               │       
               │       There are some cases where you might want to keep the variable private and
               │       unused at the same time. In this case, you can call `ignore()` on the variable
               │       to force the use of it.
               │       
               │       ```moonbit
               │       
               │       ///|
               │       let p_unused : Int = 42
               │       
               │       ///|
               │       test {
               │         ignore(p_unused)
               │       }
               │       
               │       ///|
               │       fn main {
               │         let x = 42
               │         ignore(x)
               │       }
               │       ```
            ───╯
            Finished. moon: ran 2 tasks, now up to date (1 warnings, 0 errors)
        "#]],
    );
}

#[test]
fn test_add_mi_if_self_not_set_in_test_imports() {
    let dir = TestDir::new("self-pkg-in-test-import.in");

    check(
        get_stdout(&dir, ["check", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc check ./lib/hello.mbt -o ./_build/wasm-gc/release/check/lib/lib.mi -pkg username/hello/lib -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check ./main/main.mbt -o ./_build/wasm-gc/release/check/main/main.mi -pkg username/hello/main -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/lib/lib.mi:lib -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main:./main -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check -doctest-only ./main/main.mbt -include-doctests -o ./_build/wasm-gc/release/check/main/main.blackbox_test.mi -pkg username/hello/main_blackbox_test -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/lib/lib.mi:lib -i ./_build/wasm-gc/release/check/main/main.mi:main -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main_blackbox_test:./main -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check ./lib3/hello.mbt -o ./_build/wasm-gc/release/check/lib3/lib3.mi -pkg username/hello/lib3 -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib3:./lib3 -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check ./lib3/hello_test.mbt -doctest-only ./lib3/hello.mbt -include-doctests -o ./_build/wasm-gc/release/check/lib3/lib3.blackbox_test.mi -pkg username/hello/lib3_blackbox_test -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/lib3/lib3.mi:lib3 -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib3_blackbox_test:./lib3 -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check ./lib2/hello.mbt -o ./_build/wasm-gc/release/check/lib2/lib2.mi -pkg username/hello/lib2 -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib2:./lib2 -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check ./lib2/hello_test.mbt -doctest-only ./lib2/hello.mbt -include-doctests -o ./_build/wasm-gc/release/check/lib2/lib2.blackbox_test.mi -pkg username/hello/lib2_blackbox_test -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/lib2/lib2.mi:lib2 -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib2_blackbox_test:./lib2 -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check ./lib/hello_test.mbt -doctest-only ./lib/hello.mbt -include-doctests -o ./_build/wasm-gc/release/check/lib/lib.blackbox_test.mi -pkg username/hello/lib_blackbox_test -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/lib/lib.mi:lll -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib_blackbox_test:./lib -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
        "#]],
    );

    check(get_stdout(&dir, ["check"]), expect![""]);
    get_stdout(&dir, ["clean"]);
    check(
        get_stderr(&dir, ["check"]),
        expect![[r#"
            Finished. moon: ran 8 tasks, now up to date
        "#]],
    );

    check(
        get_stdout(&dir, ["test", "--no-parallelize", "--sort-input"]),
        expect![[r#"
            Hello, world! lib
            Hello, world! lib2
            Hello, world! lib3
            Total tests: 3, passed: 3, failed: 0.
        "#]],
    );
}

#[cfg(unix)]
#[test]
#[ignore = "moon query may no support anymore"]
fn test_moon_query() {
    let dir = TestDir::new("test_filter_pkg_with_test_imports.in");

    check(
        get_stdout(&dir, ["build", "--show-artifacts"]),
        // need topological order
        expect![[r#"
            [["username/hello/lib6","$ROOT/target/wasm-gc/release/build/lib6/lib6.mi","$ROOT/target/wasm-gc/release/build/lib6/lib6.core"],["username/hello/lib7","$ROOT/target/wasm-gc/release/build/lib7/lib7.mi","$ROOT/target/wasm-gc/release/build/lib7/lib7.core"],["username/hello/lib3","$ROOT/target/wasm-gc/release/build/lib3/lib3.mi","$ROOT/target/wasm-gc/release/build/lib3/lib3.core"],["username/hello/lib1","$ROOT/target/wasm-gc/release/build/lib1/lib1.mi","$ROOT/target/wasm-gc/release/build/lib1/lib1.core"],["username/hello/lib5","$ROOT/target/wasm-gc/release/build/lib5/lib5.mi","$ROOT/target/wasm-gc/release/build/lib5/lib5.core"],["username/hello/lib4","$ROOT/target/wasm-gc/release/build/lib4/lib4.mi","$ROOT/target/wasm-gc/release/build/lib4/lib4.core"],["username/hello/lib2","$ROOT/target/wasm-gc/release/build/lib2/lib2.mi","$ROOT/target/wasm-gc/release/build/lib2/lib2.core"],["username/hello/lib","$ROOT/target/wasm-gc/release/build/lib/lib.mi","$ROOT/target/wasm-gc/release/build/lib/lib.core"],["username/hello/main","$ROOT/target/wasm-gc/release/build/main/main.mi","$ROOT/target/wasm-gc/release/build/main/main.core"]]
        "#]],
    );

    get_stdout(&dir, ["query", "moonbitlang/x"]);
}

#[test]
#[allow(clippy::just_underscores_and_digits)]
fn test_moon_install_bin() {
    let top_dir = TestDir::new("moon_install_bin.in");
    let dir = top_dir.join("user.in");

    let _1;
    let _2;
    let _3;
    let _4;
    let _5;

    #[cfg(unix)]
    {
        _1 = top_dir.join("author2.in").join("author2-native");
        _2 = top_dir.join("author2.in").join("author2-js");
        _3 = top_dir.join("author2.in").join("author2-wasm");
        _4 = top_dir.join("author1.in").join("this-is-wasm");
        _5 = top_dir.join("author1.in").join("main-js");
    }

    #[cfg(target_os = "windows")]
    {
        _1 = top_dir.join("author2.in").join("author2-native.ps1");
        _2 = top_dir.join("author2.in").join("author2-js.ps1");
        _3 = top_dir.join("author2.in").join("author2-wasm.ps1");
        _4 = top_dir.join("author1.in").join("this-is-wasm.ps1");
        _5 = top_dir.join("author1.in").join("main-js.ps1");
    }

    // moon check should auto install bin deps
    get_stdout(&dir, ["check"]);
    assert!(_1.exists());
    assert!(_2.exists());
    assert!(_3.exists());
    assert!(_4.exists());
    assert!(_5.exists());

    {
        // delete all bin files
        std::fs::remove_file(&_1).unwrap();
        std::fs::remove_file(&_2).unwrap();
        std::fs::remove_file(&_3).unwrap();
        std::fs::remove_file(&_4).unwrap();
        std::fs::remove_file(&_5).unwrap();
        assert!(!_1.exists());
        assert!(!_2.exists());
        assert!(!_3.exists());
        assert!(!_4.exists());
        assert!(!_5.exists());
    }

    // moon install should install bin deps
    get_stdout(&dir, ["install"]);

    assert!(_1.exists());
    assert!(_2.exists());
    assert!(_3.exists());
    assert!(_4.exists());
    assert!(_5.exists());

    let content = get_stderr(&dir, ["build", "--sort-input"]);

    // Ensure the prebuild tasks' outputs are shown
    assert!(content.contains("main-js"));
    assert!(content.contains("lib Hello, world!"));
    assert!(content.contains("()"));
}

#[test]
#[ignore = "platform-dependent behavior"]
fn test_strip_debug() {
    let dir = TestDir::new("strip_debug.in");

    check(
        get_stdout(&dir, ["build", "--dry-run"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target wasm-gc
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main moon_new/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc
        "#]],
    );
    check(
        get_stdout(&dir, ["build", "--no-strip", "--dry-run"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -g
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target wasm-gc -g
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main moon_new/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc -g
        "#]],
    );
    check(
        get_stdout(&dir, ["build", "--debug", "--strip", "--dry-run"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/build/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -O0 -source-map
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/build/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target wasm-gc -O0 -source-map
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/build/lib/lib.core ./target/wasm-gc/debug/build/main/main.core -main moon_new/main -o ./target/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc -O0 -source-map
        "#]],
    );
    check(
        get_stdout(&dir, ["build", "--debug", "--no-strip", "--dry-run"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/build/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -source-map
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/build/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target wasm-gc -g -O0 -source-map
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/build/lib/lib.core ./target/wasm-gc/debug/build/main/main.core -main moon_new/main -o ./target/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc -g -O0 -source-map
        "#]],
    );
    check(
        get_stdout(&dir, ["build", "--debug", "--dry-run"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/build/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -source-map
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/build/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target wasm-gc -g -O0 -source-map
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/build/lib/lib.core ./target/wasm-gc/debug/build/main/main.core -main moon_new/main -o ./target/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc -g -O0 -source-map
        "#]],
    );

    check(
        get_stdout(&dir, ["test", "--dry-run"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/test/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/main/__blackbox_test_info.json --doctest-only ./main/main.mbt --target wasm-gc --pkg-name moon_new/main --driver-kind blackbox
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/test/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target wasm-gc -g -O0 -source-map
            moonc build-package ./target/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt -doctest-only ./main/main.mbt -o ./target/wasm-gc/debug/test/main/main.blackbox_test.core -pkg moon_new/main_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/main/main.mi:main -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources moon_new/main_blackbox_test:./main -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.core ./target/wasm-gc/debug/test/main/main.core ./target/wasm-gc/debug/test/main/main.blackbox_test.core -main moon_new/main_blackbox_test -o ./target/wasm-gc/debug/test/main/main.blackbox_test.wasm -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moon_new/main_blackbox_test:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target wasm-gc --pkg-name moon_new/lib --driver-kind blackbox
            moonc build-package ./target/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -pkg moon_new/lib_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources moon_new/lib_blackbox_test:./lib -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.core ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -main moon_new/lib_blackbox_test -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/lib_blackbox_test:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__internal_test_info.json ./lib/hello.mbt --target wasm-gc --pkg-name moon_new/lib --driver-kind internal
            moonc build-package ./lib/hello.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.internal_test.core -pkg moon_new/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -source-map -no-mi -test-mode
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.internal_test.core -main moon_new/lib -o ./target/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
        "#]],
    );
    check(
        get_stdout(&dir, ["test", "--debug", "--dry-run"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/test/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/main/__blackbox_test_info.json --doctest-only ./main/main.mbt --target wasm-gc --pkg-name moon_new/main --driver-kind blackbox
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/test/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target wasm-gc -g -O0 -source-map
            moonc build-package ./target/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt -doctest-only ./main/main.mbt -o ./target/wasm-gc/debug/test/main/main.blackbox_test.core -pkg moon_new/main_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/main/main.mi:main -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources moon_new/main_blackbox_test:./main -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.core ./target/wasm-gc/debug/test/main/main.core ./target/wasm-gc/debug/test/main/main.blackbox_test.core -main moon_new/main_blackbox_test -o ./target/wasm-gc/debug/test/main/main.blackbox_test.wasm -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moon_new/main_blackbox_test:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target wasm-gc --pkg-name moon_new/lib --driver-kind blackbox
            moonc build-package ./target/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -pkg moon_new/lib_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources moon_new/lib_blackbox_test:./lib -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.core ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -main moon_new/lib_blackbox_test -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/lib_blackbox_test:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__internal_test_info.json ./lib/hello.mbt --target wasm-gc --pkg-name moon_new/lib --driver-kind internal
            moonc build-package ./lib/hello.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.internal_test.core -pkg moon_new/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -source-map -no-mi -test-mode
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.internal_test.core -main moon_new/lib -o ./target/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
        "#]],
    );
    check(
        get_stdout(&dir, ["test", "--release", "--dry-run"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/test/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/release/test/main/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/release/test/main/__blackbox_test_info.json --doctest-only ./main/main.mbt --target wasm-gc --pkg-name moon_new/main --driver-kind blackbox
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/test/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/test/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target wasm-gc -source-map
            moonc build-package ./target/wasm-gc/release/test/main/__generated_driver_for_blackbox_test.mbt -doctest-only ./main/main.mbt -o ./target/wasm-gc/release/test/main/main.blackbox_test.core -pkg moon_new/main_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/test/main/main.mi:main -i ./target/wasm-gc/release/test/lib/lib.mi:lib -pkg-sources moon_new/main_blackbox_test:./main -target wasm-gc -source-map -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/test/lib/lib.core ./target/wasm-gc/release/test/main/main.core ./target/wasm-gc/release/test/main/main.blackbox_test.core -main moon_new/main_blackbox_test -o ./target/wasm-gc/release/test/main/main.blackbox_test.wasm -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moon_new/main_blackbox_test:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/release/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/release/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target wasm-gc --pkg-name moon_new/lib --driver-kind blackbox
            moonc build-package ./target/wasm-gc/release/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./target/wasm-gc/release/test/lib/lib.blackbox_test.core -pkg moon_new/lib_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/test/lib/lib.mi:lib -pkg-sources moon_new/lib_blackbox_test:./lib -target wasm-gc -source-map -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/test/lib/lib.core ./target/wasm-gc/release/test/lib/lib.blackbox_test.core -main moon_new/lib_blackbox_test -o ./target/wasm-gc/release/test/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/lib_blackbox_test:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/release/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/release/test/lib/__internal_test_info.json ./lib/hello.mbt --target wasm-gc --pkg-name moon_new/lib --driver-kind internal
            moonc build-package ./lib/hello.mbt ./target/wasm-gc/release/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/release/test/lib/lib.internal_test.core -pkg moon_new/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -source-map -no-mi -test-mode
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/test/lib/lib.internal_test.core -main moon_new/lib -o ./target/wasm-gc/release/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -source-map
        "#]],
    );
    check(
        get_stdout(&dir, ["test", "--release", "--no-strip", "--dry-run"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/test/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -g -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/release/test/main/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/release/test/main/__blackbox_test_info.json --doctest-only ./main/main.mbt --target wasm-gc --pkg-name moon_new/main --driver-kind blackbox
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/test/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/test/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target wasm-gc -g -source-map
            moonc build-package ./target/wasm-gc/release/test/main/__generated_driver_for_blackbox_test.mbt -doctest-only ./main/main.mbt -o ./target/wasm-gc/release/test/main/main.blackbox_test.core -pkg moon_new/main_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/test/main/main.mi:main -i ./target/wasm-gc/release/test/lib/lib.mi:lib -pkg-sources moon_new/main_blackbox_test:./main -target wasm-gc -g -source-map -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/test/lib/lib.core ./target/wasm-gc/release/test/main/main.core ./target/wasm-gc/release/test/main/main.blackbox_test.core -main moon_new/main_blackbox_test -o ./target/wasm-gc/release/test/main/main.blackbox_test.wasm -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moon_new/main_blackbox_test:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/release/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/release/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target wasm-gc --pkg-name moon_new/lib --driver-kind blackbox
            moonc build-package ./target/wasm-gc/release/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./target/wasm-gc/release/test/lib/lib.blackbox_test.core -pkg moon_new/lib_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/test/lib/lib.mi:lib -pkg-sources moon_new/lib_blackbox_test:./lib -target wasm-gc -g -source-map -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/test/lib/lib.core ./target/wasm-gc/release/test/lib/lib.blackbox_test.core -main moon_new/lib_blackbox_test -o ./target/wasm-gc/release/test/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/lib_blackbox_test:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/release/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/release/test/lib/__internal_test_info.json ./lib/hello.mbt --target wasm-gc --pkg-name moon_new/lib --driver-kind internal
            moonc build-package ./lib/hello.mbt ./target/wasm-gc/release/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/release/test/lib/lib.internal_test.core -pkg moon_new/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -g -source-map -no-mi -test-mode
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/test/lib/lib.internal_test.core -main moon_new/lib -o ./target/wasm-gc/release/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -source-map
        "#]],
    );
    check(
        get_stdout(&dir, ["test", "--debug", "--no-strip", "--dry-run"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/test/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/main/__blackbox_test_info.json --doctest-only ./main/main.mbt --target wasm-gc --pkg-name moon_new/main --driver-kind blackbox
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/test/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target wasm-gc -g -O0 -source-map
            moonc build-package ./target/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt -doctest-only ./main/main.mbt -o ./target/wasm-gc/debug/test/main/main.blackbox_test.core -pkg moon_new/main_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/main/main.mi:main -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources moon_new/main_blackbox_test:./main -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.core ./target/wasm-gc/debug/test/main/main.core ./target/wasm-gc/debug/test/main/main.blackbox_test.core -main moon_new/main_blackbox_test -o ./target/wasm-gc/debug/test/main/main.blackbox_test.wasm -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moon_new/main_blackbox_test:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target wasm-gc --pkg-name moon_new/lib --driver-kind blackbox
            moonc build-package ./target/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -pkg moon_new/lib_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources moon_new/lib_blackbox_test:./lib -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.core ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -main moon_new/lib_blackbox_test -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/lib_blackbox_test:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__internal_test_info.json ./lib/hello.mbt --target wasm-gc --pkg-name moon_new/lib --driver-kind internal
            moonc build-package ./lib/hello.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.internal_test.core -pkg moon_new/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -source-map -no-mi -test-mode
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.internal_test.core -main moon_new/lib -o ./target/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
        "#]],
    );
}

#[test]
fn test_diff_mbti() {
    let dir = TestDir::new("diff_mbti.in");
    let content = get_err_stdout(&dir, ["info", "--target", "all"]);
    assert!(content.contains("$ROOT/_build/wasm-gc/release/check/lib/lib.mbti"));
    assert!(content.contains("$ROOT/_build/js/release/check/lib/lib.mbti"));
    assert!(content.contains("-pub fn aaa() -> String"));
    assert!(content.contains("+pub fn a() -> String"));
}

#[test]
fn moon_info_specific_package() {
    let dir = TestDir::new("moon_new/plain");

    // exact match
    get_stdout(&dir, ["info", "--package", "moon_new/main"]);
    assert!(dir.join("main/").join(MBTI_GENERATED).exists());
    assert!(!dir.join("lib/").join(MBTI_GENERATED).exists());

    // fuzzy match
    get_stdout(&dir, ["info", "--package", "lib"]);
    assert!(dir.join("lib/").join(MBTI_GENERATED).exists());

    let content = get_err_stderr(&dir, ["info", "--package", "moon_new/does_not_exist"]);
    assert!(content.contains("package `moon_new/does_not_exist` not found, make sure you have spelled it correctly, e.g. `moonbitlang/core/hashmap`(exact match) or `hashmap`(fuzzy match)"));
}

#[test]
fn test_exports_in_native_backend() {
    let dir = TestDir::new("native_exports.in");
    let _ = get_stdout(&dir, ["build", "--target", "native"]);
    assert!(
        !dir.join(BUILD_DIR)
            .join("native")
            .join("release")
            .join("build")
            .join("lib")
            .join("lib.c")
            .exists()
    );
    let lib2_c = read(
        dir.join(BUILD_DIR)
            .join("native")
            .join("release")
            .join("build")
            .join("lib2")
            .join("lib2.c"),
    );
    assert!(lib2_c.contains("_M0FP38username5hello4lib25hello()"));

    // alias not works
    let lib3_c = read(
        dir.join(BUILD_DIR)
            .join("native")
            .join("release")
            .join("build")
            .join("lib3")
            .join("lib3.c"),
    );
    assert!(lib3_c.contains("_M0FP38username5hello4lib35hello()"));
}

#[test]
#[ignore = "moonyacc is not updated for a long time, and this test case is broken"]
fn test_diag_loc_map() {
    let dir = TestDir::new("diag_loc_map.in");
    check(
        get_err_stderr(&dir, ["check"]),
        expect![[r#"
            Error: [4014]
                 ╭─[ $ROOT/parser.mbt:129:13 ]
                 │
             129 │       lhs + "x" + rhs
                 │             ─┬─  
                 │              ╰─── Expr Type Mismatch
                    has type : String
                    wanted   : Int
            ─────╯
            Failed with 0 warnings, 1 errors.
            error: failed when checking project
        "#]],
    );
}

#[test]
fn test_dont_link_third_party() {
    let dir = TestDir::new("dont_link_third_party.in");

    check(
        get_stdout(&dir, ["build", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/release/build/main/main.core -pkg hello/main -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources hello/main:./main -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/release/build/main/main.core -main hello/main -o ./_build/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm-gc
        "#]],
    );
}

#[test]
#[ignore = "Supported backend check is extremely slow, temporarily disable it"]
fn test_supported_backends_in_pkg_json() {
    let dir = TestDir::new("supported_backends_in_pkg_json");
    let pkg1 = dir.join("pkg1.in");
    // let pkg2 = dir.join("pkg2.in");
    let pkg3 = dir.join("pkg3.in");

    check(
        get_err_stderr(&pkg1, ["build"]),
        expect![[r#"
            error: cannot find a common supported backend for the deps chain: "username/hello1/main: [js, wasm-gc] -> username/hello1/lib: [native]"
        "#]],
    );

    // check(
    //     get_err_stderr(&pkg2, ["build"]),
    //     expect![[r#"
    //         error: deps chain: "username/hello2/main: [js, wasm-gc] -> username/hello2/lib: [js]" supports backends `[js]`, while the current target backend is wasm-gc
    //     "#]],
    // );

    check(
        get_err_stderr(&pkg3, ["check"]),
        expect![[r#"
            error: cannot find a common supported backend for the deps chain: "username/hello/main: [wasm-gc] -> username/hello/lib: [js, llvm, native, wasm, wasm-gc] -> username/hello/lib1: [wasm-gc] -> username/hello/lib3: [wasm-gc] -> username/hello/lib7: [js, wasm]"
        "#]],
    );
}

#[test]
fn test_update_expect_failed() {
    let dir = TestDir::new("test_expect_with_escape.in");
    let _ = get_stdout(&dir, ["test", "-u"]);
    check(
        read(dir.join("src").join("lib").join("hello.mbt")),
        expect![[r#"
            ///|
            test {
              inspect("\x0b", content=(#|
              ))
              inspect("a\x0b", content=(#|a
              ))
              inspect("a\x00b", content=(#|a b
              ))
              inspect("a\x00b\x19", content=(#|a b
              ))
              inspect("\na\n\x00\nb\n\x19", content=(
                #|
                #|a
                #| 
                #|b
                #|
              ))
              inspect("\n\"a\n\x00\nb\"\n\x19", content=(
                #|
                #|"a
                #| 
                #|b"
                #|
              ))
            }

            ///|
            test {
              inspect("\"abc\"", content=(#|"abc"
              ))
              inspect("\"a\nb\nc\"", content=(
                #|"a
                #|b
                #|c"
              ))
              inspect("\x0b\"a\nb\nc\"", content=(
                #|"a
                #|b
                #|c"
              ))
            }
        "#]],
    );
}

#[test]
fn test_update_expect_failed_with_multiline_string() {
    let dir = TestDir::new("test_expect_with_multiline_string_content.in");
    let _ = get_stdout(&dir, ["test", "-u"]);
    check(
        read(dir.join("src").join("lib").join("hello.mbt")),
        expect![[r#"
            ///|
            test {
              inspect("\"abc\"", content=(#|"abc"
              ))
              inspect("\"abc\"", 
                content=(
                  #|"abc"
            )
              )
              inspect("\"abc\"", content=(
                #|"abc"

              ))
              inspect(
                "\"a\nb\nc\"",
                content=(
                  #|"a
                  #|b
                  #|c"

                ),
              )
            }
        "#]],
    );
}

#[test]
fn test_native_stub_in_pkg_json() {
    let dir = TestDir::new("native_stub.in");

    let native_1 = dir.join("native_1.in");
    let native_2 = dir.join("native_2.in");
    let native_3 = dir.join("native_3.in");

    check(
        get_stdout(&native_1, ["test", "--target", "native", "--sort-input"]),
        expect![[r#"
            Hello world from native_1/lib/stub.c!!!
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    check(
        get_stdout(&native_1, ["run", "main", "--target", "native"]),
        expect![[r#"
            Hello world from native_1/lib/stub.c!!!
        "#]],
    );

    check(
        get_stdout(&native_2, ["test", "--target", "native", "--sort-input"]),
        expect![[r#"
            Hello world from native_1/lib/stub.c!!!
            Hello world from native_2/libb/stub.c!!!
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    check(
        get_stdout(&native_2, ["run", "main", "--target", "native"]),
        expect![[r#"
            Hello world from native_2/libb/stub.c!!!
        "#]],
    );

    check(
        get_stdout(&native_3, ["test", "--target", "native", "--sort-input"]),
        expect![[r#"
            Hello world from native_1/lib/stub.c!!!
            Hello world from native_2/libb/stub.c!!!
            Hello world from native_3/libbb/stub.c!!!
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    check(
        get_stdout(&native_3, ["run", "main", "--target", "native"]),
        expect![[r#"
            Hello world from native_3/libbb/stub.c!!!
        "#]],
    );
}

#[test]
#[cfg(unix)]
fn test_pre_build_dirty() {
    let dir = TestDir::new("pre_build_dirty.in");

    // Assert prebuild runs by checking file existence
    let file = dir.join("src/lib/a.mbt");

    assert!(!file.exists(), "prebuild.txt should not exist yet");
    let _first_prebuild = get_stderr(&dir, ["check"]);
    assert!(file.exists(), "prebuild.txt should exist after prebuild");
    let mtime = file.metadata().unwrap().modified().unwrap();

    check(
        get_stderr(&dir, ["check"]),
        expect![[r#"
            Finished. moon: no work to do
        "#]],
    );
    check(
        get_stderr(&dir, ["check"]),
        expect![[r#"
            Finished. moon: no work to do
        "#]],
    );

    let mtime_end = file.metadata().unwrap().modified().unwrap();
    assert_eq!(mtime, mtime_end, "file should not be modified");
}

#[test]
fn test_upgrade() -> anyhow::Result<()> {
    if std::env::var("CI").is_err() {
        return Ok(());
    }
    let tmp_dir = tempfile::TempDir::new()?;
    let _ = std::process::Command::new(moon_bin())
        .env("MOON_HOME", tmp_dir.path().to_str().unwrap())
        .arg("upgrade")
        .arg("--force")
        .arg("--non-interactive")
        .arg("--base-url")
        .arg("https://cli.moonbitlang.com")
        .output()?;
    #[cfg(unix)]
    let xs = [
        tmp_dir.path().join("bin").join("moon").exists(),
        tmp_dir.path().join("bin").join("moonc").exists(),
    ];
    #[cfg(windows)]
    let xs = [
        tmp_dir.path().join("bin").join("moon.exe").exists(),
        tmp_dir.path().join("bin").join("moonc.exe").exists(),
    ];
    check(format!("{xs:?}"), expect!["[true, true]"]);
    Ok(())
}

#[test]
fn test_no_warn_deps() {
    let dir = TestDir::new("no_warn_deps.in");
    let dir = dir.join("user.in");

    check(
        get_stderr(&dir, ["check"]),
        expect![[r#"
            Finished. moon: ran 6 tasks, now up to date
        "#]],
    );
    check(
        get_stderr(&dir, ["check", "--deny-warn"]),
        expect![[r#"
            Finished. moon: ran 6 tasks, now up to date
        "#]],
    );
    check(
        get_stderr(&dir, ["build"]),
        expect![[r#"
            Finished. moon: ran 3 tasks, now up to date
        "#]],
    );
}

#[test]
fn test_postadd_script() {
    if std::env::var("CI").is_err() {
        return;
    }
    let dir = TestDir::new("test_postadd_script.in");
    let output = get_stdout(&dir, ["add", "lijunchen/test_postadd"]);
    assert!(output.contains(".mooncakes/lijunchen/test_postadd"));

    let _ = get_stdout(&dir, ["remove", "lijunchen/test_postadd"]);

    let out = std::process::Command::new(moon_bin())
        .current_dir(&dir)
        .env("MOON_IGNORE_POSTADD", "1")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .args(["add", "lijunchen/test_postadd"])
        .output()
        .unwrap();
    let out = String::from_utf8(out.stderr).unwrap();
    assert!(!out.contains(".mooncakes/lijunchen/test_postadd"));
}

#[test]
fn test_ambiguous_pkg() {
    let dir = TestDir::new("ambiguous_pkg.in");

    // FIXME: Improve error message
    let stderr = get_err_stderr(&dir, ["build"]);
    println!("{}", stderr);
    assert!(
        stderr.contains("Ambiguous package name") || stderr.contains("Duplicated package name")
    );
}

#[test]
fn moon_check_and_test_single_file() {
    let dir = TestDir::new("moon_test_single_file.in");
    let single_mbt = dir.join("single.mbt").display().to_string();
    let single_mbt_md = dir.join("111.mbt.md").display().to_string();

    // .mbt
    {
        // rel path
        check(
            get_stdout(&dir, ["test", "single.mbt", "-i", "0"]),
            expect![[r#"
                ------------------ 11111111 ------------------
                Total tests: 1, passed: 1, failed: 0.
            "#]],
        );
        check(
            get_err_stdout(&dir, ["test", "single.mbt", "-i", "1"]),
            expect![[r#"
                [moon/test] test single/single.mbt:12 (#1) failed
                expect test failed at $ROOT/single.mbt:13:3-13:18
                Diff: (- expected, + actual)
                ----
                +234523
                ----

                Total tests: 1, passed: 0, failed: 1.
            "#]],
        );
        // abs path
        check(
            get_stdout(&dir, ["test", &single_mbt, "-i", "0"]),
            expect![[r#"
                ------------------ 11111111 ------------------
                Total tests: 1, passed: 1, failed: 0.
            "#]],
        );
        let s = get_stdout(&dir, ["test", &single_mbt, "-i", "1", "-u"]);
        let exp = r#"
------------------ 22222222 ------------------
Total tests: 1, passed: 1, failed: 0.
"#
        .trim();
        assert!(
            s.contains(exp),
            "output did not contain expected updated test output"
        ); // FIXME: this is because different versions have different output during update expect

        check(
            get_stderr(&dir, ["check", "single.mbt"]),
            expect![[r#"
                Warning: [0002]
                   ╭─[ $ROOT/single.mbt:8:7 ]
                   │
                 8 │   let single_mbt = 1
                   │       ─────┬────  
                   │            ╰────── Warning (unused_value): Unused variable 'single_mbt'
                ───╯
                Finished. moon: ran 2 tasks, now up to date (1 warnings, 0 errors)
            "#]],
        );
        // abs path
        check(
            get_stderr(&dir, ["check", &single_mbt]),
            expect![[r#"
                Warning: [0002]
                   ╭─[ $ROOT/single.mbt:8:7 ]
                   │
                 8 │   let single_mbt = 1
                   │       ─────┬────  
                   │            ╰────── Warning (unused_value): Unused variable 'single_mbt'
                ───╯
                Finished. moon: ran 1 task, now up to date (1 warnings, 0 errors)
            "#]],
        );
    }

    // .mbt.md
    {
        check(
            get_stdout(&dir, ["test", "222.mbt.md"]),
            expect![[r#"
                222
                Total tests: 1, passed: 1, failed: 0.
            "#]],
        );

        // rel path
        let s = get_stdout(&dir, ["test", "111.mbt.md", "-i", "0"]);
        assert!(
            s.contains("111"),
            "output did not contain expected test output"
        );

        check(
            get_err_stdout(&dir, ["test", "111.mbt.md", "-i", "1"]),
            expect![[r#"
                [moon/test] test single/111.mbt.md:27 (#1) failed
                expect test failed at $ROOT/111.mbt.md:34:5-34:20
                Diff: (- expected, + actual)
                ----
                +234523
                ----

                Total tests: 1, passed: 0, failed: 1.
            "#]],
        );
        // abs path
        check(
            get_stdout(&dir, ["test", &single_mbt_md, "-i", "0"]),
            expect![[r#"
                111
                Total tests: 1, passed: 1, failed: 0.
            "#]],
        );

        let s = get_stdout(&dir, ["test", &single_mbt_md, "-i", "1", "-u"]);
        assert!(
            s.contains("222"),
            "output did not contain expected updated test output"
        );
        assert!(
            s.contains("Total tests: 1, passed: 1, failed: 0."),
            "output did not contain expected updated test output"
        );

        // rel path
        snapbox::assert_data_eq!(
            get_stderr(&dir, ["check", "111.mbt.md"]),
            snapbox::str!(
                r#"
Warning: [0002]
    ╭─[ $ROOT/111.mbt.md:28:9 ]
    │
 28 │     let single_mbt_md = 1
    │         ──────┬──────  
    │               ╰──────── Warning (unused_value): Unused variable 'single_mbt_md'
────╯
..."#
            )
        );

        // abs path
        check(
            get_stderr(&dir, ["check", &single_mbt_md]),
            expect![[r#"
                Warning: [0002]
                    ╭─[ $ROOT/111.mbt.md:28:9 ]
                    │
                 28 │     let single_mbt_md = 1
                    │         ──────┬──────  
                    │               ╰──────── Warning (unused_value): Unused variable 'single_mbt_md'
                ────╯
                Finished. moon: ran 1 task, now up to date (1 warnings, 0 errors)
            "#]],
        );
    }

    // check single file (with or without main func)
    {
        let with_main = dir.join("with_main.mbt").display().to_string();
        snapbox::assert_data_eq!(
            get_stderr(&dir, ["check", &with_main]),
            snapbox::str![[r#"
Warning: [0002]
   ╭─[ $ROOT/with_main.mbt:2:7 ]
   │
 2 │   let with_main = 1
   │       ────┬────  
   │           ╰────── Warning (unused_value): Unused variable 'with_main'
───╯
...
"#]],
        );
        let without_main = dir.join("without_main.mbt").display().to_string();
        snapbox::assert_data_eq!(
            get_stderr(&dir, ["check", &without_main]),
            snapbox::str![[r#"
Warning: [0001]
   ╭─[ $ROOT/without_main.mbt:1:4 ]
   │
 1 │ fn func() -> Unit {
   │    ──┬─  
   │      ╰─── Warning (unused_value): Unused function 'func'
───╯
Warning: [0002]
   ╭─[ $ROOT/without_main.mbt:2:7 ]
   │
 2 │   let without_main = 1
   │       ──────┬─────  
   │             ╰─────── Warning (unused_value): Unused variable 'without_main'
───╯
...
"#]],
        );
    }
}

/// Test that single-file commands properly report errors for non-existent files
/// instead of panicking (issue #1192)
#[test]
fn test_single_file_nonexistent_path_error() {
    // Use temp_dir for cross-platform compatibility
    let nonexistent_path = std::env::temp_dir()
        .join("nonexistent_file_12345.mbt")
        .display()
        .to_string();

    // Test moon check with non-existent file outside any project
    // Should fail gracefully (exit != 101 which is Rust panic code)
    let check_result = snapbox::cmd::Command::new(moon_bin())
        .current_dir(std::env::temp_dir())
        .args(["check", &nonexistent_path])
        .assert()
        .failure();
    // Verify it's not a panic (exit code 101)
    assert_ne!(
        check_result.get_output().status.code(),
        Some(101),
        "moon check should not panic for non-existent file"
    );

    // Test moon test with non-existent file outside any project
    let test_result = snapbox::cmd::Command::new(moon_bin())
        .current_dir(std::env::temp_dir())
        .args(["test", &nonexistent_path])
        .assert()
        .failure();
    // Verify it's not a panic (exit code 101)
    assert_ne!(
        test_result.get_output().status.code(),
        Some(101),
        "moon test should not panic for non-existent file"
    );

    // Test moon run with non-existent file outside any project
    let run_result = snapbox::cmd::Command::new(moon_bin())
        .current_dir(std::env::temp_dir())
        .args(["run", &nonexistent_path])
        .assert()
        .failure();
    // Verify it's not a panic (exit code 101)
    assert_ne!(
        run_result.get_output().status.code(),
        Some(101),
        "moon run should not panic for non-existent file"
    );
}

#[test]
#[ignore = "subpackage is not fully supported yet"]
fn test_sub_package() {
    let dir = TestDir::new("test_sub_package.in");

    check(
        get_stdout(&dir, ["test", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./dep/hello.mbt -o ./target/wasm-gc/debug/test/dep/dep.core -pkg moon_new/dep -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -pkg-sources moon_new/dep:./dep -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package ./sub_pkg/111.mbt ./sub_pkg/dir/222.mbt -o ./target/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.core -pkg moon_new/sub_pkg -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/debug/test/dep/dep.mi:dep -pkg-sources moon_new/sub_pkg:./sub_pkg -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package ./dep2/hello.mbt -o ./target/wasm-gc/debug/test/dep2/dep2.core -pkg moon_new/dep2 -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/dep2:./dep2 -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package ./sub_pkg/111.mbt ./sub_pkg/hello.mbt -o ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.core -pkg moon_new/sub_pkg -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/debug/test/dep2/dep2.mi:dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -target wasm-gc -g -O0 -source-map -workspace-path .
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/test/__generated_driver_for_whitebox_test.mbt --output-metadata ./target/wasm-gc/debug/test/test/__whitebox_test_info.json ./test/hello_wbtest.mbt --target wasm-gc --pkg-name moon_new/test --driver-kind whitebox
            moonc build-package ./test/hello.mbt ./test/hello_wbtest.mbt ./target/wasm-gc/debug/test/test/__generated_driver_for_whitebox_test.mbt -o ./target/wasm-gc/debug/test/test/test.whitebox_test.core -pkg moon_new/test -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.mi:sub_pkg -pkg-sources moon_new/test:./test -target wasm-gc -g -O0 -source-map -whitebox-test -no-mi -test-mode -workspace-path .
            moonc link-core '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core' ./target/wasm-gc/debug/test/dep2/dep2.core ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.core ./target/wasm-gc/debug/test/test/test.whitebox_test.core -main moon_new/test -o ./target/wasm-gc/debug/test/test/test.whitebox_test.wasm -test-mode -pkg-config-path ./test/moon.pkg.json -pkg-sources moon_new/dep2:./dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources moon_new/test:./test -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/test/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/test/__internal_test_info.json ./test/hello.mbt --target wasm-gc --pkg-name moon_new/test --driver-kind internal
            moonc build-package ./test/hello.mbt ./target/wasm-gc/debug/test/test/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/test/test.internal_test.core -pkg moon_new/test -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/test:./test -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core' ./target/wasm-gc/debug/test/dep2/dep2.core ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.core ./target/wasm-gc/debug/test/test/test.internal_test.core -main moon_new/test -o ./target/wasm-gc/debug/test/test/test.internal_test.wasm -test-mode -pkg-config-path ./test/moon.pkg.json -pkg-sources moon_new/dep2:./dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources moon_new/test:./test -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map
            moonc build-package ./test/hello.mbt -o ./target/wasm-gc/debug/test/test/test.core -pkg moon_new/test -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/test:./test -target wasm-gc -g -O0 -source-map -workspace-path .
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/test/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/test/__blackbox_test_info.json ./test/hello_test.mbt --doctest-only ./test/hello.mbt --target wasm-gc --pkg-name moon_new/test --driver-kind blackbox
            moonc build-package ./test/hello_test.mbt ./target/wasm-gc/debug/test/test/__generated_driver_for_blackbox_test.mbt -doctest-only ./test/hello.mbt -o ./target/wasm-gc/debug/test/test/test.blackbox_test.core -pkg moon_new/test_blackbox_test -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.mi:sub_pkg -i ./target/wasm-gc/debug/test/test/test.mi:test -pkg-sources moon_new/test_blackbox_test:./test -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core' ./target/wasm-gc/debug/test/dep2/dep2.core ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.core ./target/wasm-gc/debug/test/test/test.core ./target/wasm-gc/debug/test/test/test.blackbox_test.core -main moon_new/test_blackbox_test -o ./target/wasm-gc/debug/test/test/test.blackbox_test.wasm -test-mode -pkg-config-path ./test/moon.pkg.json -pkg-sources moon_new/dep2:./dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources moon_new/test:./test -pkg-sources moon_new/test_blackbox_test:./test -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/sub_pkg_sub/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/sub_pkg_sub/__internal_test_info.json ./sub_pkg/111.mbt ./sub_pkg/dir/222.mbt --target wasm-gc --pkg-name moon_new/sub_pkg_sub --driver-kind internal
            moonc build-package ./sub_pkg/111.mbt ./sub_pkg/dir/222.mbt ./target/wasm-gc/debug/test/sub_pkg_sub/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.internal_test.core -pkg moon_new/sub_pkg -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/debug/test/dep/dep.mi:dep -pkg-sources moon_new/sub_pkg:./sub_pkg -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core' ./target/wasm-gc/debug/test/dep/dep.core ./target/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.internal_test.core -main moon_new/sub_pkg -o ./target/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.internal_test.wasm -test-mode -pkg-config-path ./sub_pkg/moon.pkg.json -pkg-sources moon_new/dep:./dep -pkg-sources moon_new/sub_pkg_sub:./sub_pkg -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/sub_pkg_sub/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/sub_pkg_sub/__blackbox_test_info.json --doctest-only ./sub_pkg/111.mbt --doctest-only ./sub_pkg/dir/222.mbt --target wasm-gc --pkg-name moon_new/sub_pkg_sub --driver-kind blackbox
            moonc build-package ./target/wasm-gc/debug/test/sub_pkg_sub/__generated_driver_for_blackbox_test.mbt -doctest-only ./sub_pkg/111.mbt -doctest-only ./sub_pkg/dir/222.mbt -o ./target/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.blackbox_test.core -pkg moon_new/sub_pkg_sub_blackbox_test -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/debug/test/dep/dep.mi:dep -i ./target/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.mi:sub_pkg_sub -pkg-sources moon_new/sub_pkg_sub_blackbox_test:./sub_pkg -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core' ./target/wasm-gc/debug/test/dep/dep.core ./target/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.core ./target/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.blackbox_test.core -main moon_new/sub_pkg_blackbox_test -o ./target/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.blackbox_test.wasm -test-mode -pkg-config-path ./sub_pkg/moon.pkg.json -pkg-sources moon_new/dep:./dep -pkg-sources moon_new/sub_pkg_sub:./sub_pkg -pkg-sources moon_new/sub_pkg_sub_blackbox_test:./sub_pkg -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/sub_pkg/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/sub_pkg/__internal_test_info.json ./sub_pkg/111.mbt ./sub_pkg/hello.mbt --target wasm-gc --pkg-name moon_new/sub_pkg --driver-kind internal
            moonc build-package ./sub_pkg/111.mbt ./sub_pkg/hello.mbt ./target/wasm-gc/debug/test/sub_pkg/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.internal_test.core -pkg moon_new/sub_pkg -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/debug/test/dep2/dep2.mi:dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core' ./target/wasm-gc/debug/test/dep2/dep2.core ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.internal_test.core -main moon_new/sub_pkg -o ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.internal_test.wasm -test-mode -pkg-config-path ./sub_pkg/moon.pkg.json -pkg-sources moon_new/dep2:./dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/sub_pkg/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/sub_pkg/__blackbox_test_info.json --doctest-only ./sub_pkg/111.mbt --doctest-only ./sub_pkg/hello.mbt --target wasm-gc --pkg-name moon_new/sub_pkg --driver-kind blackbox
            moonc build-package ./target/wasm-gc/debug/test/sub_pkg/__generated_driver_for_blackbox_test.mbt -doctest-only ./sub_pkg/111.mbt -doctest-only ./sub_pkg/hello.mbt -o ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.blackbox_test.core -pkg moon_new/sub_pkg_blackbox_test -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/debug/test/dep2/dep2.mi:dep2 -i ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.mi:sub_pkg -pkg-sources moon_new/sub_pkg_blackbox_test:./sub_pkg -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core' ./target/wasm-gc/debug/test/dep2/dep2.core ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.core ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.blackbox_test.core -main moon_new/sub_pkg_blackbox_test -o ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.blackbox_test.wasm -test-mode -pkg-config-path ./sub_pkg/moon.pkg.json -pkg-sources moon_new/dep2:./dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources moon_new/sub_pkg_blackbox_test:./sub_pkg -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/main/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/main/__internal_test_info.json ./main/main.mbt --target wasm-gc --pkg-name moon_new/main --driver-kind internal
            moonc build-package ./main/main.mbt ./target/wasm-gc/debug/test/main/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/main/main.internal_test.core -pkg moon_new/main -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/main:./main -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core' ./target/wasm-gc/debug/test/dep2/dep2.core ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.core ./target/wasm-gc/debug/test/main/main.internal_test.core -main moon_new/main -o ./target/wasm-gc/debug/test/main/main.internal_test.wasm -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/dep2:./dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources moon_new/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/test/main/main.core -pkg moon_new/main -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/main:./main -target wasm-gc -g -O0 -source-map -workspace-path .
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/main/__blackbox_test_info.json --doctest-only ./main/main.mbt --target wasm-gc --pkg-name moon_new/main --driver-kind blackbox
            moonc build-package ./target/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt -doctest-only ./main/main.mbt -o ./target/wasm-gc/debug/test/main/main.blackbox_test.core -pkg moon_new/main_blackbox_test -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/debug/test/main/main.mi:main -i ./target/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/main_blackbox_test:./main -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core' ./target/wasm-gc/debug/test/dep2/dep2.core ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.core ./target/wasm-gc/debug/test/main/main.core ./target/wasm-gc/debug/test/main/main.blackbox_test.core -main moon_new/main_blackbox_test -o ./target/wasm-gc/debug/test/main/main.blackbox_test.wasm -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/dep2:./dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources moon_new/main:./main -pkg-sources moon_new/main_blackbox_test:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__internal_test_info.json ./lib/hello.mbt --target wasm-gc --pkg-name moon_new/lib --driver-kind internal
            moonc build-package ./lib/hello.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.internal_test.core -pkg moon_new/lib -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.mi:sub_pkg -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core' ./target/wasm-gc/debug/test/dep2/dep2.core ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.core ./target/wasm-gc/debug/test/lib/lib.internal_test.core -main moon_new/lib -o ./target/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/dep2:./dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources moon_new/lib:./lib -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/test/lib/lib.core -pkg moon_new/lib -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.mi:sub_pkg -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -source-map -workspace-path .
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target wasm-gc --pkg-name moon_new/lib --driver-kind blackbox
            moonc build-package ./target/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -pkg moon_new/lib_blackbox_test -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -i ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.mi:sub_pkg -pkg-sources moon_new/lib_blackbox_test:./lib -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core' ./target/wasm-gc/debug/test/dep2/dep2.core ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.core ./target/wasm-gc/debug/test/lib/lib.core ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -main moon_new/lib_blackbox_test -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/dep2:./dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/lib_blackbox_test:./lib -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/dep2/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/dep2/__internal_test_info.json ./dep2/hello.mbt --target wasm-gc --pkg-name moon_new/dep2 --driver-kind internal
            moonc build-package ./dep2/hello.mbt ./target/wasm-gc/debug/test/dep2/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/dep2/dep2.internal_test.core -pkg moon_new/dep2 -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/dep2:./dep2 -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core' ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.core ./target/wasm-gc/debug/test/dep2/dep2.internal_test.core -main moon_new/dep2 -o ./target/wasm-gc/debug/test/dep2/dep2.internal_test.wasm -test-mode -pkg-config-path ./dep2/moon.pkg.json -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources moon_new/dep2:./dep2 -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/dep2/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/dep2/__blackbox_test_info.json --doctest-only ./dep2/hello.mbt --target wasm-gc --pkg-name moon_new/dep2 --driver-kind blackbox
            moonc build-package ./target/wasm-gc/debug/test/dep2/__generated_driver_for_blackbox_test.mbt -doctest-only ./dep2/hello.mbt -o ./target/wasm-gc/debug/test/dep2/dep2.blackbox_test.core -pkg moon_new/dep2_blackbox_test -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/debug/test/dep2/dep2.mi:dep2 -i ./target/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/dep2_blackbox_test:./dep2 -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core' ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.core ./target/wasm-gc/debug/test/dep2/dep2.core ./target/wasm-gc/debug/test/dep2/dep2.blackbox_test.core -main moon_new/dep2_blackbox_test -o ./target/wasm-gc/debug/test/dep2/dep2.blackbox_test.wasm -test-mode -pkg-config-path ./dep2/moon.pkg.json -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources moon_new/dep2:./dep2 -pkg-sources moon_new/dep2_blackbox_test:./dep2 -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/dep/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/dep/__internal_test_info.json ./dep/hello.mbt --target wasm-gc --pkg-name moon_new/dep --driver-kind internal
            moonc build-package ./dep/hello.mbt ./target/wasm-gc/debug/test/dep/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/dep/dep.internal_test.core -pkg moon_new/dep -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -pkg-sources moon_new/dep:./dep -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core' ./target/wasm-gc/debug/test/dep/dep.internal_test.core -main moon_new/dep -o ./target/wasm-gc/debug/test/dep/dep.internal_test.wasm -test-mode -pkg-config-path ./dep/moon.pkg.json -pkg-sources moon_new/dep:./dep -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/dep/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/dep/__blackbox_test_info.json --doctest-only ./dep/hello.mbt --target wasm-gc --pkg-name moon_new/dep --driver-kind blackbox
            moonc build-package ./target/wasm-gc/debug/test/dep/__generated_driver_for_blackbox_test.mbt -doctest-only ./dep/hello.mbt -o ./target/wasm-gc/debug/test/dep/dep.blackbox_test.core -pkg moon_new/dep_blackbox_test -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/debug/test/dep/dep.mi:dep -pkg-sources moon_new/dep_blackbox_test:./dep -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core' ./target/wasm-gc/debug/test/dep/dep.core ./target/wasm-gc/debug/test/dep/dep.blackbox_test.core -main moon_new/dep_blackbox_test -o ./target/wasm-gc/debug/test/dep/dep.blackbox_test.wasm -test-mode -pkg-config-path ./dep/moon.pkg.json -pkg-sources moon_new/dep:./dep -pkg-sources moon_new/dep_blackbox_test:./dep -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map
        "#]],
    );
    check(
        get_stdout(&dir, ["check", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc check ./dep/hello.mbt -o ./target/wasm-gc/release/check/dep/dep.mi -pkg moon_new/dep -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -pkg-sources moon_new/dep:./dep -target wasm-gc -workspace-path .
            moonc check ./sub_pkg/111.mbt ./sub_pkg/dir/222.mbt -o ./target/wasm-gc/release/check/sub_pkg/sub_pkg_sub.mi -pkg moon_new/sub_pkg -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/release/check/dep/dep.mi:dep -pkg-sources moon_new/sub_pkg:./sub_pkg -target wasm-gc -workspace-path .
            moonc check ./dep2/hello.mbt -o ./target/wasm-gc/release/check/dep2/dep2.mi -pkg moon_new/dep2 -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/release/check/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/dep2:./dep2 -target wasm-gc -workspace-path .
            moonc check ./sub_pkg/111.mbt ./sub_pkg/hello.mbt -o ./target/wasm-gc/release/check/sub_pkg/sub_pkg.mi -pkg moon_new/sub_pkg -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/release/check/dep2/dep2.mi:dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -target wasm-gc -workspace-path .
            moonc check ./test/hello.mbt ./test/hello_wbtest.mbt -o ./target/wasm-gc/release/check/test/test.whitebox_test.mi -pkg moon_new/test -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/release/check/sub_pkg/sub_pkg.mi:sub_pkg -pkg-sources moon_new/test:./test -target wasm-gc -whitebox-test -workspace-path .
            moonc check ./test/hello.mbt -o ./target/wasm-gc/release/check/test/test.mi -pkg moon_new/test -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/release/check/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/test:./test -target wasm-gc -workspace-path .
            moonc check ./test/hello_test.mbt -doctest-only ./test/hello.mbt -include-doctests -o ./target/wasm-gc/release/check/test/test.blackbox_test.mi -pkg moon_new/test_blackbox_test -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/release/check/sub_pkg/sub_pkg.mi:sub_pkg -i ./target/wasm-gc/release/check/test/test.mi:test -pkg-sources moon_new/test_blackbox_test:./test -target wasm-gc -blackbox-test -workspace-path .
            moonc check -doctest-only ./sub_pkg/111.mbt -doctest-only ./sub_pkg/dir/222.mbt -include-doctests -o ./target/wasm-gc/release/check/sub_pkg/sub_pkg_sub.blackbox_test.mi -pkg moon_new/sub_pkg_blackbox_test -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/release/check/dep/dep.mi:dep -i ./target/wasm-gc/release/check/sub_pkg/sub_pkg_sub.mi:sub_pkg_sub -pkg-sources moon_new/sub_pkg_blackbox_test:./sub_pkg -target wasm-gc -blackbox-test -workspace-path .
            moonc check -doctest-only ./sub_pkg/111.mbt -doctest-only ./sub_pkg/hello.mbt -include-doctests -o ./target/wasm-gc/release/check/sub_pkg/sub_pkg.blackbox_test.mi -pkg moon_new/sub_pkg_blackbox_test -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/release/check/dep2/dep2.mi:dep2 -i ./target/wasm-gc/release/check/sub_pkg/sub_pkg.mi:sub_pkg -pkg-sources moon_new/sub_pkg_blackbox_test:./sub_pkg -target wasm-gc -blackbox-test -workspace-path .
            moonc check ./main/main.mbt -o ./target/wasm-gc/release/check/main/main.mi -pkg moon_new/main -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/release/check/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/main:./main -target wasm-gc -workspace-path .
            moonc check -doctest-only ./main/main.mbt -include-doctests -o ./target/wasm-gc/release/check/main/main.blackbox_test.mi -pkg moon_new/main_blackbox_test -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/release/check/main/main.mi:main -i ./target/wasm-gc/release/check/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/main_blackbox_test:./main -target wasm-gc -blackbox-test -workspace-path .
            moonc check ./lib/hello.mbt -o ./target/wasm-gc/release/check/lib/lib.mi -pkg moon_new/lib -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/release/check/sub_pkg/sub_pkg.mi:sub_pkg -pkg-sources moon_new/lib:./lib -target wasm-gc -workspace-path .
            moonc check -doctest-only ./lib/hello.mbt -include-doctests -o ./target/wasm-gc/release/check/lib/lib.blackbox_test.mi -pkg moon_new/lib_blackbox_test -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/release/check/lib/lib.mi:lib -i ./target/wasm-gc/release/check/sub_pkg/sub_pkg.mi:sub_pkg -pkg-sources moon_new/lib_blackbox_test:./lib -target wasm-gc -blackbox-test -workspace-path .
            moonc check -doctest-only ./dep2/hello.mbt -include-doctests -o ./target/wasm-gc/release/check/dep2/dep2.blackbox_test.mi -pkg moon_new/dep2_blackbox_test -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/release/check/dep2/dep2.mi:dep2 -i ./target/wasm-gc/release/check/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/dep2_blackbox_test:./dep2 -target wasm-gc -blackbox-test -workspace-path .
            moonc check -doctest-only ./dep/hello.mbt -include-doctests -o ./target/wasm-gc/release/check/dep/dep.blackbox_test.mi -pkg moon_new/dep_blackbox_test -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/release/check/dep/dep.mi:dep -pkg-sources moon_new/dep_blackbox_test:./dep -target wasm-gc -blackbox-test -workspace-path .
        "#]],
    );
    check(
        get_stdout(&dir, ["build", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./dep/hello.mbt -o ./target/wasm-gc/release/build/dep/dep.core -pkg moon_new/dep -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -pkg-sources moon_new/dep:./dep -target wasm-gc -workspace-path .
            moonc build-package ./sub_pkg/111.mbt ./sub_pkg/dir/222.mbt -o ./target/wasm-gc/release/build/sub_pkg/sub_pkg_sub.core -pkg moon_new/sub_pkg -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/release/build/dep/dep.mi:dep -pkg-sources moon_new/sub_pkg:./sub_pkg -target wasm-gc -workspace-path .
            moonc build-package ./dep2/hello.mbt -o ./target/wasm-gc/release/build/dep2/dep2.core -pkg moon_new/dep2 -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/release/build/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/dep2:./dep2 -target wasm-gc -workspace-path .
            moonc build-package ./sub_pkg/111.mbt ./sub_pkg/hello.mbt -o ./target/wasm-gc/release/build/sub_pkg/sub_pkg.core -pkg moon_new/sub_pkg -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/release/build/dep2/dep2.mi:dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -target wasm-gc -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg moon_new/main -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/release/build/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/main:./main -target wasm-gc -workspace-path .
            moonc link-core '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core' ./target/wasm-gc/release/build/dep2/dep2.core ./target/wasm-gc/release/build/sub_pkg/sub_pkg.core ./target/wasm-gc/release/build/main/main.core -main moon_new/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/dep2:./dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources moon_new/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm-gc
        "#]],
    );
    check(
        get_stdout(&dir, ["run", "main", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./dep/hello.mbt -o ./target/wasm-gc/release/build/dep/dep.core -pkg moon_new/dep -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -pkg-sources moon_new/dep:./dep -target wasm-gc -workspace-path .
            moonc build-package ./sub_pkg/111.mbt ./sub_pkg/dir/222.mbt -o ./target/wasm-gc/release/build/sub_pkg/sub_pkg_sub.core -pkg moon_new/sub_pkg -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/release/build/dep/dep.mi:dep -pkg-sources moon_new/sub_pkg:./sub_pkg -target wasm-gc -workspace-path .
            moonc build-package ./dep2/hello.mbt -o ./target/wasm-gc/release/build/dep2/dep2.core -pkg moon_new/dep2 -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/release/build/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/dep2:./dep2 -target wasm-gc -workspace-path .
            moonc build-package ./sub_pkg/111.mbt ./sub_pkg/hello.mbt -o ./target/wasm-gc/release/build/sub_pkg/sub_pkg.core -pkg moon_new/sub_pkg -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/release/build/dep2/dep2.mi:dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -target wasm-gc -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg moon_new/main -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./target/wasm-gc/release/build/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/main:./main -target wasm-gc -workspace-path .
            moonc link-core '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core' ./target/wasm-gc/release/build/dep2/dep2.core ./target/wasm-gc/release/build/sub_pkg/sub_pkg.core ./target/wasm-gc/release/build/main/main.core -main moon_new/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/dep2:./dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources moon_new/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm-gc
            moonrun ./target/wasm-gc/release/build/main/main.wasm --
        "#]],
    );
}

#[test]
fn test_in_main_pkg() {
    let dir = TestDir::new("test_in_main_pkg.in");

    check(
        get_stderr(&dir, ["check"]),
        expect![[r#"
            Warning: [0002]
               ╭─[ $ROOT/lib/1_test.mbt:2:7 ]
               │
             2 │   let a = 1
               │       ┬  
               │       ╰── Warning (unused_value): Unused variable 'a'
            ───╯
            Finished. moon: ran 6 tasks, now up to date (1 warnings, 0 errors)
        "#]],
    );

    check(
        get_stdout(
            &dir,
            ["test", "-p", "main", "--sort-input", "--no-parallelize"],
        ),
        expect![[r#"
            hello from lib pkg
            ------------------bb test in main pkg ------------------
            hello from lib pkg
            ------------------internal test in main pkg ------------------
            hello from lib pkg
            ------------------ wb test in main pkg ------------------
            Total tests: 3, passed: 3, failed: 0.
        "#]],
    );
    check(
        get_stdout(&dir, ["test", "--sort-input", "--no-parallelize"]),
        expect![[r#"
            ------------------bb test in lib pkg ------------------
            ------------------internal test in lib pkg ------------------
            ------------------ wb test in lib pkg ------------------
            hello from lib pkg
            ------------------bb test in main pkg ------------------
            hello from lib pkg
            ------------------internal test in main pkg ------------------
            hello from lib pkg
            ------------------ wb test in main pkg ------------------
            Total tests: 6, passed: 6, failed: 0.
        "#]],
    );
}

#[test]
fn merge_doc_test_and_md_test() {
    let dir = TestDir::new("all_kind_test.in");

    let check_output = get_stderr(&dir, ["check"]);
    println!("CHECK OUTPUT:\n{}", check_output);

    assert!(check_output.contains("unused_in_lib_md_test"));
    assert!(check_output.contains("unused_in_lib_doc_test"));

    assert!(get_err_stdout(&dir, ["test"]).contains("Total tests: 9, passed: 6, failed: 3."));

    // should be ok if run with update
    get_stdout(&dir, ["test", "-u"]);

    // -f should run internal test & doc test in that file
    check(
        get_stdout(
            &dir,
            ["test", "-p", "lib", "-f", "hello.mbt", "--sort-input"],
        )
        .split("\n")
        .collect::<Vec<&str>>()
        .iter()
        .take(5)
        .next_back()
        .unwrap(),
        expect!["Total tests: 4, passed: 4, failed: 0."],
    );
    // -i should run internal test only
    check(
        get_stdout(&dir, ["test", "-p", "lib", "-f", "hello.mbt", "-i", "0"]),
        expect![[r#"
            internal test 1
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    // --doc-index should run doc test only
    check(
        get_stdout(
            &dir,
            ["test", "-p", "lib", "-f", "hello.mbt", "--doc-index", "0"],
        ),
        expect![[r#"
            doc test 1
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    // should run bb test only
    check(
        get_stdout(
            &dir,
            ["test", "-p", "lib", "-f", "hello_test.mbt", "-i", "0"],
        ),
        expect![[r#"
            blackbox test 1
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    // doc test is ignored for _test.mbt & .mbt.md
    {
        check(
            get_stdout(
                &dir,
                [
                    "test",
                    "-p",
                    "lib",
                    "-f",
                    "hello_test.mbt",
                    "--doc-index",
                    "0",
                ],
            ),
            expect![[r#"
            Total tests: 0, passed: 0, failed: 0.
        "#]],
        );
        check(
            get_stdout(
                &dir,
                [
                    "test",
                    "-p",
                    "lib",
                    "-f",
                    "README.mbt.md",
                    "--doc-index",
                    "0",
                ],
            ),
            expect![[r#"
            Total tests: 0, passed: 0, failed: 0.
        "#]],
        );
    }
}

#[test]
fn moon_test_target_js_panic_with_sourcemap() {
    let dir = TestDir::new("moon_test_target_js_panic_with_sourcemap.in");

    let output = get_err_stdout(&dir, ["test", "--target", "js"]);

    // Extract first 4 lines + the last line (Total tests) as they should be consistent across Node.js versions
    let lines: Vec<&str> = output.lines().collect();
    let first_four_lines = lines.iter().take(4).cloned().collect::<Vec<_>>().join("\n");
    let last_line = lines.last().unwrap_or(&"");
    let filtered_output = format!("{}\n{}", first_four_lines, last_line);

    check(
        &filtered_output,
        // should keep in this format, it's used in ide test explorer
        expect![[r#"
            [username/hello] test lib/hello_test.mbt:1 ("hello") failed: Error
                at $panic ($ROOT/_build/js/debug/test/lib/lib.blackbox_test.js:6:9)
                at _M0FP38username5hello19lib__blackbox__test41____test__68656c6c6f5f746573742e6d6274__0 ($ROOT/src/lib/hello_test.mbt:3:5)
                at _M0FP38username5hello19lib__blackbox__test42moonbit__test__driver__internal__js__catch ($ROOT/src/lib/__generated_driver_for_blackbox_test.mbt:351:15)
            Total tests: 1, passed: 0, failed: 1."#]],
    );
}
