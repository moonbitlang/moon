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

use super::*;
use expect_test::expect;
use moonutil::{
    common::{
        CargoPathExt, DEP_PATH, MBTI_GENERATED, MOON_MOD_JSON, StringExt, TargetBackend,
        get_cargo_pkg_version,
    },
    module::MoonModJSON,
};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

mod backend;
mod backend_config;
mod design;
mod diamond_pkg;
mod docs_examples;
mod dummy_core;
mod extra_flags;
mod fancy_import;
mod filter_by_path;
mod fmt;
mod fmt_ignore;
mod hello;
mod inline_test;
mod mbti;
mod moon_bench;
mod moon_bundle;
mod moon_commands;
mod moon_coverage;
mod moon_new;
mod moon_test;
mod moon_version;
mod native_stub_stability;
mod output_format;
mod packages;
mod prebuild;
mod prebuild_config_script;
mod simple_pkg;
mod target_backend;
mod targets;
mod test_driver_dependencies;
mod test_error_report;
mod test_expect_test;
mod test_filter;
mod virtual_pkg_dep;
mod virtual_pkg_test;
mod warns;

#[test]
fn test_need_link() {
    let dir = TestDir::new("need_link.in");
    check(
        get_stdout(&dir, ["build", "--dry-run", "--nostd", "--sort-input"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm-gc -workspace-path .
            moonc link-core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main username/hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/lib/lib.core -main username/hello/lib -o ./target/wasm-gc/release/build/lib/lib.wasm -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -target wasm-gc
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
fn test_moon_test_release() {
    let dir = TestDir::new("test_release.in");

    check(
        get_stdout(&dir, ["test", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/test/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./lib -target wasm-gc -g -O0 -source-map -workspace-path .
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/main/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/main/__internal_test_info.json ./main/main.mbt --target wasm-gc --pkg-name username/hello/main --driver-kind internal
            moonc build-package ./main/main.mbt ./target/wasm-gc/debug/test/main/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/main/main.internal_test.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.core ./target/wasm-gc/debug/test/main/main.internal_test.core -main username/hello/main -o ./target/wasm-gc/debug/test/main/main.internal_test.wasm -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/test/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm-gc -g -O0 -source-map -workspace-path .
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/main/__blackbox_test_info.json --doctest-only ./main/main.mbt --target wasm-gc --pkg-name username/hello/main --driver-kind blackbox
            moonc build-package ./target/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt -doctest-only ./main/main.mbt -o ./target/wasm-gc/debug/test/main/main.blackbox_test.core -pkg username/hello/main_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/main/main.mi:main -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources username/hello/main_blackbox_test:./main -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.core ./target/wasm-gc/debug/test/main/main.core ./target/wasm-gc/debug/test/main/main.blackbox_test.core -main username/hello/main_blackbox_test -o ./target/wasm-gc/debug/test/main/main.blackbox_test.wasm -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -pkg-sources username/hello/main_blackbox_test:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_whitebox_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__whitebox_test_info.json ./lib/hello_wbtest.mbt --target wasm-gc --pkg-name username/hello/lib --driver-kind whitebox
            moonc build-package ./lib/hello.mbt ./lib/hello_wbtest.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_whitebox_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.whitebox_test.core -pkg username/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./lib -target wasm-gc -g -O0 -source-map -whitebox-test -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.whitebox_test.core -main username/hello/lib -o ./target/wasm-gc/debug/test/lib/lib.whitebox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__internal_test_info.json ./lib/hello.mbt --target wasm-gc --pkg-name username/hello/lib --driver-kind internal
            moonc build-package ./lib/hello.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./lib -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target wasm-gc --pkg-name username/hello/lib --driver-kind blackbox
            moonc build-package ./target/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -pkg username/hello/lib_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources username/hello/lib_blackbox_test:./lib -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.core ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -main username/hello/lib_blackbox_test -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/lib_blackbox_test:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
        "#]],
    );

    check(
        get_stdout(&dir, ["test", "--release", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/test/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./lib -target wasm-gc -source-map -workspace-path .
            moon generate-test-driver --output-driver ./target/wasm-gc/release/test/main/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/release/test/main/__internal_test_info.json ./main/main.mbt --target wasm-gc --pkg-name username/hello/main --driver-kind internal
            moonc build-package ./main/main.mbt ./target/wasm-gc/release/test/main/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/release/test/main/main.internal_test.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/test/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm-gc -source-map -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/test/lib/lib.core ./target/wasm-gc/release/test/main/main.internal_test.core -main username/hello/main -o ./target/wasm-gc/release/test/main/main.internal_test.wasm -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -source-map
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/test/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/test/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm-gc -source-map -workspace-path .
            moon generate-test-driver --output-driver ./target/wasm-gc/release/test/main/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/release/test/main/__blackbox_test_info.json --doctest-only ./main/main.mbt --target wasm-gc --pkg-name username/hello/main --driver-kind blackbox
            moonc build-package ./target/wasm-gc/release/test/main/__generated_driver_for_blackbox_test.mbt -doctest-only ./main/main.mbt -o ./target/wasm-gc/release/test/main/main.blackbox_test.core -pkg username/hello/main_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/test/main/main.mi:main -i ./target/wasm-gc/release/test/lib/lib.mi:lib -pkg-sources username/hello/main_blackbox_test:./main -target wasm-gc -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/test/lib/lib.core ./target/wasm-gc/release/test/main/main.core ./target/wasm-gc/release/test/main/main.blackbox_test.core -main username/hello/main_blackbox_test -o ./target/wasm-gc/release/test/main/main.blackbox_test.wasm -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -pkg-sources username/hello/main_blackbox_test:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/release/test/lib/__generated_driver_for_whitebox_test.mbt --output-metadata ./target/wasm-gc/release/test/lib/__whitebox_test_info.json ./lib/hello_wbtest.mbt --target wasm-gc --pkg-name username/hello/lib --driver-kind whitebox
            moonc build-package ./lib/hello.mbt ./lib/hello_wbtest.mbt ./target/wasm-gc/release/test/lib/__generated_driver_for_whitebox_test.mbt -o ./target/wasm-gc/release/test/lib/lib.whitebox_test.core -pkg username/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./lib -target wasm-gc -source-map -whitebox-test -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/test/lib/lib.whitebox_test.core -main username/hello/lib -o ./target/wasm-gc/release/test/lib/lib.whitebox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/release/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/release/test/lib/__internal_test_info.json ./lib/hello.mbt --target wasm-gc --pkg-name username/hello/lib --driver-kind internal
            moonc build-package ./lib/hello.mbt ./target/wasm-gc/release/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/release/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./lib -target wasm-gc -source-map -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/wasm-gc/release/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/release/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/release/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target wasm-gc --pkg-name username/hello/lib --driver-kind blackbox
            moonc build-package ./target/wasm-gc/release/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./target/wasm-gc/release/test/lib/lib.blackbox_test.core -pkg username/hello/lib_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/test/lib/lib.mi:lib -pkg-sources username/hello/lib_blackbox_test:./lib -target wasm-gc -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/test/lib/lib.core ./target/wasm-gc/release/test/lib/lib.blackbox_test.core -main username/hello/lib_blackbox_test -o ./target/wasm-gc/release/test/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/lib_blackbox_test:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -source-map
        "#]],
    );

    check(
        get_stdout(
            &dir,
            ["test", "--release", "--sort-input", "--no-parallelize"],
        ),
        expect![[r#"
            test A
            test hello_0
            test hello_1
            Total tests: 3, passed: 3, failed: 0.
        "#]],
    );
}

#[test]
fn test_backtrace() {
    let dir = TestDir::new("backtrace.in");

    let out = get_err_stderr(&dir, ["run", "main"]);
    assert!(!out.contains("main.foo"));
    assert!(!out.contains("main.bar"));

    let out = get_err_stderr(&dir, ["run", "main", "--debug"]);
    assert!(out.contains("main.foo"));
    assert!(out.contains("main.bar"));
}

#[test]
fn test_export_memory_name() {
    let dir = TestDir::new("export_memory.in");
    let _ = get_stdout(&dir, ["build", "--target", "wasm-gc", "--output-wat"]);
    let content = std::fs::read_to_string(
        dir.join("target")
            .join("wasm-gc")
            .join("release")
            .join("build")
            .join("main")
            .join("main.wat"),
    )
    .unwrap();
    assert!(content.contains("awesome_memory"));

    check(
        get_stdout(&dir, ["build", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm-gc -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main username/hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc -export-memory-name awesome_memory
        "#]],
    );

    check(
        get_stdout(
            &dir,
            ["build", "--dry-run", "--sort-input", "--target", "wasm"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm/release/build/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm/release/bundle -pkg-sources username/hello/lib:./lib -target wasm -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/wasm/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm/release/bundle -i ./target/wasm/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm/release/bundle/core.core ./target/wasm/release/build/lib/lib.core ./target/wasm/release/build/main/main.core -main username/hello/main -o ./target/wasm/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm -export-memory-name awesome_memory
        "#]],
    );

    check(
        get_stdout(
            &dir,
            ["build", "--dry-run", "--sort-input", "--target", "js"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/js/release/build/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/js/release/bundle -pkg-sources username/hello/lib:./lib -target js -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/js/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/js/release/bundle -i ./target/js/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target js -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/js/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/js/release/bundle/core.core ./target/js/release/build/lib/lib.core ./target/js/release/build/main/main.core -main username/hello/main -o ./target/js/release/build/main/main.js -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target js
        "#]],
    );
}

#[test]
fn test_no_block_params() {
    let dir = TestDir::new("no_block_params.in");
    check(
        get_stdout(&dir, ["build", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm-gc -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main username/hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc -no-block-params
        "#]],
    );

    check(
        get_stdout(
            &dir,
            ["build", "--dry-run", "--sort-input", "--target", "wasm"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm/release/build/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm/release/bundle -pkg-sources username/hello/lib:./lib -target wasm -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/wasm/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm/release/bundle -i ./target/wasm/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm/release/bundle/core.core ./target/wasm/release/build/lib/lib.core ./target/wasm/release/build/main/main.core -main username/hello/main -o ./target/wasm/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm -no-block-params
        "#]],
    );

    check(
        get_stdout(
            &dir,
            ["build", "--dry-run", "--sort-input", "--target", "js"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/js/release/build/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/js/release/bundle -pkg-sources username/hello/lib:./lib -target js -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/js/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/js/release/bundle -i ./target/js/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target js -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/js/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/js/release/bundle/core.core ./target/js/release/build/lib/lib.core ./target/js/release/build/main/main.core -main username/hello/main -o ./target/js/release/build/main/main.js -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target js
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
            error: failed to read import path in "$ROOT/main/moon.pkg.json"

            Caused by:
                No matching module was found for mbt/core/set
        "#]],
    );
    check(
        get_err_stderr(&dir, ["build"]),
        expect![[r#"
            error: failed to read import path in "$ROOT/main/moon.pkg.json"

            Caused by:
                No matching module was found for mbt/core/set
        "#]],
    );
    check(
        get_err_stderr(&dir, ["test"]),
        expect![[r#"
            error: failed to read import path in "$ROOT/main/moon.pkg.json"

            Caused by:
                No matching module was found for mbt/core/set
        "#]],
    );
    check(
        get_err_stderr(&dir, ["bundle"]),
        expect![[r#"
            error: failed to read import path in "$ROOT/main/moon.pkg.json"

            Caused by:
                No matching module was found for mbt/core/set
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
    check(
        get_err_stderr(&dir, ["check", "--sort-input"]),
        expect![[r#"
            error: $ROOT/lib2/moon.pkg.json: cannot import internal package `username/hello/lib/internal` in `username/hello/lib2`
            $ROOT/lib2/moon.pkg.json: cannot import internal package `username/hello/lib/internal/b` in `username/hello/lib2`
            $ROOT/main/moon.pkg.json: cannot import internal package `username/hello/lib/internal` in `username/hello/main`
        "#]],
    );
}

#[test]
fn test_nonexistent_package() {
    let dir = TestDir::new("nonexistent_package.in");
    check(
        get_err_stderr(&dir, ["check", "--sort-input"]),
        expect![[r#"
            error: $ROOT/main/moon.pkg.json: cannot import `username/hello/lib/b` in `username/hello/main`, no such package
            $ROOT/main/moon.pkg.json: cannot import `username/hello/transient` in `username/hello/main`, no such package
            $ROOT/pkg/transient/moon.pkg.json: cannot import `username/transient/lib/b` in `username/transient`, no such package
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
    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin("moon"))
        .current_dir(&dir)
        .args(["run", "main"])
        .assert()
        .success()
        .stdout_matches("ok[..]");
}

#[test]
fn cakenew_test() {
    let dir = TestDir::new("cakenew_test.in");
    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin("moon"))
        .current_dir(&dir)
        .args(["run", "main"])
        .assert()
        .success()
        .stdout_matches("Hello,[..]");
}

#[test]
fn capture_abort_test() {
    let dir = super::TestDir::new("capture_abort_test.in");
    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin("moon"))
        .current_dir(&dir)
        .args(["run", "main", "--nostd"])
        .assert()
        .failure();
}

#[test]
fn whitespace_test() {
    let dir = TestDir::new("whitespace_test.in");
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["clean"])
        .assert()
        .success();

    // unstable test
    // check(
    //     &get_stdout_with_args(&dir, ["check", "--dry-run", "--nostd"]),
    //     expect![[r#"
    //         moonc check './main lib/hello.mbt' './main lib/hello_test.mbt' -o './target/check/main lib/main lib.underscore_test.mi' -pkg 'username/hello/main lib' -pkg-sources 'username/hello/main lib:./main lib'
    //         moonc check './main lib/hello.mbt' -o './target/check/main lib/main lib.mi' -pkg 'username/hello/main lib' -pkg-sources 'username/hello/main lib:./main lib'
    //         moonc check './main exe/main.mbt' -o './target/check/main exe/main exe.mi' -pkg 'username/hello/main exe' -is-main -i './target/check/main lib/main lib.mi:lib' -pkg-sources 'username/hello/main exe:./main exe'
    //     "#]],
    // );

    check(
        get_stdout(&dir, ["build", "--dry-run", "--nostd"]),
        expect![[r#"
            moonc build-package "./main lib/hello.mbt" -o "./target/wasm-gc/release/build/main lib/main lib.core" -pkg "username/hello/main lib" -pkg-sources "username/hello/main lib:./main lib" -target wasm-gc -workspace-path .
            moonc build-package "./main exe/main.mbt" -o "./target/wasm-gc/release/build/main exe/main exe.core" -pkg "username/hello/main exe" -is-main -i "./target/wasm-gc/release/build/main lib/main lib.mi:lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc -workspace-path .
            moonc link-core "./target/wasm-gc/release/build/main lib/main lib.core" "./target/wasm-gc/release/build/main exe/main exe.core" -main "username/hello/main exe" -o "./target/wasm-gc/release/build/main exe/main exe.wasm" -pkg-config-path "./main exe/moon.pkg.json" -pkg-sources "username/hello/main lib:./main lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc
        "#]],
    );

    check(
        get_stdout(&dir, ["build", "--dry-run", "--debug", "--nostd"]),
        expect![[r#"
            moonc build-package "./main lib/hello.mbt" -o "./target/wasm-gc/debug/build/main lib/main lib.core" -pkg "username/hello/main lib" -pkg-sources "username/hello/main lib:./main lib" -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package "./main exe/main.mbt" -o "./target/wasm-gc/debug/build/main exe/main exe.core" -pkg "username/hello/main exe" -is-main -i "./target/wasm-gc/debug/build/main lib/main lib.mi:lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc link-core "./target/wasm-gc/debug/build/main lib/main lib.core" "./target/wasm-gc/debug/build/main exe/main exe.core" -main "username/hello/main exe" -o "./target/wasm-gc/debug/build/main exe/main exe.wasm" -pkg-config-path "./main exe/moon.pkg.json" -pkg-sources "username/hello/main lib:./main lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc -g -O0 -source-map
        "#]],
    );

    check(
        get_stdout(&dir, ["run", "main exe", "--dry-run", "--nostd"]),
        expect![[r#"
            moonc build-package "./main lib/hello.mbt" -o "./target/wasm-gc/release/build/main lib/main lib.core" -pkg "username/hello/main lib" -pkg-sources "username/hello/main lib:./main lib" -target wasm-gc -workspace-path .
            moonc build-package "./main exe/main.mbt" -o "./target/wasm-gc/release/build/main exe/main exe.core" -pkg "username/hello/main exe" -is-main -i "./target/wasm-gc/release/build/main lib/main lib.mi:lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc -workspace-path .
            moonc link-core "./target/wasm-gc/release/build/main lib/main lib.core" "./target/wasm-gc/release/build/main exe/main exe.core" -main "username/hello/main exe" -o "./target/wasm-gc/release/build/main exe/main exe.wasm" -pkg-config-path "./main exe/moon.pkg.json" -pkg-sources "username/hello/main lib:./main lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc
            moonrun ./target/wasm-gc/release/build/main exe/main exe.wasm
        "#]],
    );

    check(
        get_stdout(&dir, ["run", "main exe", "--dry-run", "--debug", "--nostd"]),
        expect![[r#"
            moonc build-package "./main lib/hello.mbt" -o "./target/wasm-gc/debug/build/main lib/main lib.core" -pkg "username/hello/main lib" -pkg-sources "username/hello/main lib:./main lib" -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package "./main exe/main.mbt" -o "./target/wasm-gc/debug/build/main exe/main exe.core" -pkg "username/hello/main exe" -is-main -i "./target/wasm-gc/debug/build/main lib/main lib.mi:lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc link-core "./target/wasm-gc/debug/build/main lib/main lib.core" "./target/wasm-gc/debug/build/main exe/main exe.core" -main "username/hello/main exe" -o "./target/wasm-gc/debug/build/main exe/main exe.wasm" -pkg-config-path "./main exe/moon.pkg.json" -pkg-sources "username/hello/main lib:./main lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc -g -O0 -source-map
            moonrun ./target/wasm-gc/debug/build/main exe/main exe.wasm
        "#]],
    );
    check(
        get_stdout(
            &dir,
            ["build", "--target", "wasm-gc", "--dry-run", "--nostd"],
        ),
        expect![[r#"
            moonc build-package "./main lib/hello.mbt" -o "./target/wasm-gc/release/build/main lib/main lib.core" -pkg "username/hello/main lib" -pkg-sources "username/hello/main lib:./main lib" -target wasm-gc -workspace-path .
            moonc build-package "./main exe/main.mbt" -o "./target/wasm-gc/release/build/main exe/main exe.core" -pkg "username/hello/main exe" -is-main -i "./target/wasm-gc/release/build/main lib/main lib.mi:lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc -workspace-path .
            moonc link-core "./target/wasm-gc/release/build/main lib/main lib.core" "./target/wasm-gc/release/build/main exe/main exe.core" -main "username/hello/main exe" -o "./target/wasm-gc/release/build/main exe/main exe.wasm" -pkg-config-path "./main exe/moon.pkg.json" -pkg-sources "username/hello/main lib:./main lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "build",
                "--dry-run",
                "--target",
                "wasm-gc",
                "--debug",
                "--nostd",
            ],
        ),
        expect![[r#"
            moonc build-package "./main lib/hello.mbt" -o "./target/wasm-gc/debug/build/main lib/main lib.core" -pkg "username/hello/main lib" -pkg-sources "username/hello/main lib:./main lib" -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package "./main exe/main.mbt" -o "./target/wasm-gc/debug/build/main exe/main exe.core" -pkg "username/hello/main exe" -is-main -i "./target/wasm-gc/debug/build/main lib/main lib.mi:lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc link-core "./target/wasm-gc/debug/build/main lib/main lib.core" "./target/wasm-gc/debug/build/main exe/main exe.core" -main "username/hello/main exe" -o "./target/wasm-gc/debug/build/main exe/main exe.wasm" -pkg-config-path "./main exe/moon.pkg.json" -pkg-sources "username/hello/main lib:./main lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc -g -O0 -source-map
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "run",
                "main exe",
                "--target",
                "wasm-gc",
                "--dry-run",
                "--nostd",
            ],
        ),
        expect![[r#"
            moonc build-package "./main lib/hello.mbt" -o "./target/wasm-gc/release/build/main lib/main lib.core" -pkg "username/hello/main lib" -pkg-sources "username/hello/main lib:./main lib" -target wasm-gc -workspace-path .
            moonc build-package "./main exe/main.mbt" -o "./target/wasm-gc/release/build/main exe/main exe.core" -pkg "username/hello/main exe" -is-main -i "./target/wasm-gc/release/build/main lib/main lib.mi:lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc -workspace-path .
            moonc link-core "./target/wasm-gc/release/build/main lib/main lib.core" "./target/wasm-gc/release/build/main exe/main exe.core" -main "username/hello/main exe" -o "./target/wasm-gc/release/build/main exe/main exe.wasm" -pkg-config-path "./main exe/moon.pkg.json" -pkg-sources "username/hello/main lib:./main lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc
            moonrun ./target/wasm-gc/release/build/main exe/main exe.wasm
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "run",
                "main exe",
                "--target",
                "wasm-gc",
                "--dry-run",
                "--debug",
                "--nostd",
            ],
        ),
        expect![[r#"
            moonc build-package "./main lib/hello.mbt" -o "./target/wasm-gc/debug/build/main lib/main lib.core" -pkg "username/hello/main lib" -pkg-sources "username/hello/main lib:./main lib" -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package "./main exe/main.mbt" -o "./target/wasm-gc/debug/build/main exe/main exe.core" -pkg "username/hello/main exe" -is-main -i "./target/wasm-gc/debug/build/main lib/main lib.mi:lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc link-core "./target/wasm-gc/debug/build/main lib/main lib.core" "./target/wasm-gc/debug/build/main exe/main exe.core" -main "username/hello/main exe" -o "./target/wasm-gc/debug/build/main exe/main exe.wasm" -pkg-config-path "./main exe/moon.pkg.json" -pkg-sources "username/hello/main lib:./main lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc -g -O0 -source-map
            moonrun ./target/wasm-gc/debug/build/main exe/main exe.wasm
        "#]],
    );

    check(
        get_stdout(&dir, ["run", "main exe"]),
        expect![[r#"
            Hello, world!
        "#]],
    );

    let out = get_stderr(&dir, ["check"]);
    expect![[r#"
        Finished. moon: ran 5 tasks, now up to date
    "#]]
    .assert_eq(&out);
}

#[test]
fn test_whitespace_parent_space() -> anyhow::Result<()> {
    let tmp_dir = tempfile::TempDir::new()?;
    let path_with_space = tmp_dir.path().join("with space");
    std::fs::create_dir_all(&path_with_space)?;
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/test_cases")
        .join("whitespace_test.in");
    copy(&dir, &path_with_space)?;

    let canon = dunce::canonicalize(tmp_dir.path())?;
    let prefix = canon.as_path().display().to_string().replace('\\', "/");

    let out = get_stdout(
        &path_with_space,
        ["build", "--no-render", "--sort-input", "--dry-run"],
    );
    let out = out.replace(&prefix, ".");
    let out = out.replace(
        &moonutil::moon_dir::home()
            .to_str()
            .unwrap()
            .replace('\\', "/"),
        "$MOON_HOME",
    );

    copy(&dir, &path_with_space)?;
    check(
        &out,
        expect![[r#"
            moonc build-package "./main lib/hello.mbt" -o "./target/wasm-gc/release/build/main lib/main lib.core" -pkg "username/hello/main lib" -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources "username/hello/main lib:./main lib" -target wasm-gc -workspace-path "."
            moonc build-package "./main exe/main.mbt" -o "./target/wasm-gc/release/build/main exe/main exe.core" -pkg "username/hello/main exe" -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i "./target/wasm-gc/release/build/main lib/main lib.mi:lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc -workspace-path "."
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core "./target/wasm-gc/release/build/main lib/main lib.core" "./target/wasm-gc/release/build/main exe/main exe.core" -main "username/hello/main exe" -o "./target/wasm-gc/release/build/main exe/main exe.wasm" -pkg-config-path "./main exe/moon.pkg.json" -pkg-sources "username/hello/main lib:./main lib" -pkg-sources "username/hello/main exe:./main exe" -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc
        "#]],
    );

    let out = get_stderr(&path_with_space, ["build", "--no-render"]);
    let out = out.replace(&prefix, ".");
    let out = out.replace(
        &moonutil::moon_dir::home()
            .to_str()
            .unwrap()
            .replace('\\', "/"),
        "$MOON_HOME",
    );

    copy(&dir, &path_with_space)?;
    check(
        &out,
        expect![[r#"
            Finished. moon: ran 3 tasks, now up to date
        "#]],
    );
    Ok(())
}

#[test]
fn circle_pkg_test() {
    let dir = TestDir::new("circle_pkg_AB_001_test.in");
    let stderr = get_err_stderr(&dir, ["run", "main", "--nostd"]);
    assert!(stderr.contains("cyclic dependency"), "stderr: {stderr}");
}

#[test]
fn debug_flag_test() {
    let dir = TestDir::new("debug_flag_test.in");
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["clean"])
        .assert()
        .success();

    check(
        get_stdout(&dir, ["check", "--dry-run", "--nostd"]),
        expect![[r#"
            moonc check ./lib/hello.mbt -o ./target/wasm-gc/release/check/lib/lib.mi -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -workspace-path .
            moonc check ./main/main.mbt -o ./target/wasm-gc/release/check/main/main.mi -pkg hello/main -is-main -i ./target/wasm-gc/release/check/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -workspace-path .
            moonc check -doctest-only ./main/main.mbt -include-doctests -o ./target/wasm-gc/release/check/main/main.blackbox_test.mi -pkg hello/main_blackbox_test -i ./target/wasm-gc/release/check/main/main.mi:main -i ./target/wasm-gc/release/check/lib/lib.mi:lib -pkg-sources hello/main_blackbox_test:./main -target wasm-gc -blackbox-test -workspace-path .
            moonc check -doctest-only ./lib/hello.mbt -include-doctests -o ./target/wasm-gc/release/check/lib/lib.blackbox_test.mi -pkg hello/lib_blackbox_test -i ./target/wasm-gc/release/check/lib/lib.mi:lib -pkg-sources hello/lib_blackbox_test:./lib -target wasm-gc -blackbox-test -workspace-path .
        "#]],
    );

    check(
        get_stdout(&dir, ["build", "--dry-run", "--nostd"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -workspace-path .
            moonc link-core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc
        "#]],
    );

    check(
        get_stdout(&dir, ["build", "--dry-run", "--debug", "--nostd"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc link-core ./target/wasm-gc/debug/build/lib/lib.core ./target/wasm-gc/debug/build/main/main.core -main hello/main -o ./target/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map
        "#]],
    );

    check(
        get_stdout(&dir, ["run", "main", "--dry-run", "--nostd"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -workspace-path .
            moonc link-core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc
            moonrun ./target/wasm-gc/release/build/main/main.wasm
        "#]],
    );

    check(
        get_stdout(&dir, ["run", "main", "--dry-run", "--debug", "--nostd"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc link-core ./target/wasm-gc/debug/build/lib/lib.core ./target/wasm-gc/debug/build/main/main.core -main hello/main -o ./target/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map
            moonrun ./target/wasm-gc/debug/build/main/main.wasm
        "#]],
    );
    check(
        get_stdout(
            &dir,
            ["build", "--target", "wasm-gc", "--dry-run", "--nostd"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -workspace-path .
            moonc link-core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "build",
                "--dry-run",
                "--target",
                "wasm-gc",
                "--debug",
                "--nostd",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc link-core ./target/wasm-gc/debug/build/lib/lib.core ./target/wasm-gc/debug/build/main/main.core -main hello/main -o ./target/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map
        "#]],
    );

    check(
        get_stdout(
            &dir,
            ["run", "main", "--target", "wasm-gc", "--dry-run", "--nostd"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -workspace-path .
            moonc link-core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc
            moonrun ./target/wasm-gc/release/build/main/main.wasm
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "run",
                "main",
                "--target",
                "wasm-gc",
                "--dry-run",
                "--debug",
                "--nostd",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc link-core ./target/wasm-gc/debug/build/lib/lib.core ./target/wasm-gc/debug/build/main/main.core -main hello/main -o ./target/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map
            moonrun ./target/wasm-gc/debug/build/main/main.wasm
        "#]],
    );

    // release should conflict with debug
    #[cfg(unix)]
    {
        check(
            get_err_stderr(&dir, ["test", "--release", "--debug"]),
            expect![[r#"
                error: the argument '--release' cannot be used with '--debug'

                Usage: moon test --release [PATH]

                For more information, try '--help'.
            "#]],
        );

        check(
            get_err_stderr(&dir, ["build", "--debug", "--release"]),
            expect![[r#"
                error: the argument '--debug' cannot be used with '--release'

                Usage: moon build --debug [PATH]

                For more information, try '--help'.
            "#]],
        );

        check(
            get_err_stderr(&dir, ["check", "--release", "--debug"]),
            expect![[r#"
                error: the argument '--release' cannot be used with '--debug'

                Usage: moon check --release [PATH]

                For more information, try '--help'.
            "#]],
        );

        check(
            get_err_stderr(&dir, ["run", "main", "--debug", "--release"]),
            expect![[r#"
                error: the argument '--debug' cannot be used with '--release'

                Usage: moon run --debug <PACKAGE_OR_MBT_FILE> [ARGS]...

                For more information, try '--help'.
            "#]],
        );
    }
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
            moonc build-package ./main/main_wasm.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/main:./main -target wasm-gc -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/main/main.core -main username/hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc
            moonrun ./target/wasm-gc/release/build/main/main.wasm
        "#]],
    );

    check(
        get_stdout(
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
        ),
        expect![[r#"
            moonc build-package ./main/main_wasm.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/main:./main -target wasm-gc -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/main/main.core -main username/hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc
            moonrun ./target/wasm-gc/release/build/main/main.wasm -- 中文 😄👍 hello 1242
        "#]],
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
fn test_third_party() {
    if std::env::var("CI").is_err() {
        return;
    }
    let dir = TestDir::new("third_party.in");
    get_stdout(&dir, ["update"]);
    get_stdout(&dir, ["install"]);
    get_stdout(&dir, ["build"]);
    get_stdout(&dir, ["clean"]);

    let actual = get_stderr(&dir, ["check"]);
    expect![[r#"
        Finished. moon: ran 6 tasks, now up to date
    "#]]
    .assert_eq(&actual);

    check(
        get_stdout(&dir, ["test", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./.mooncakes/lijunchen/hello18/lib/hello.mbt -w -a -alert -all -o ./target/wasm-gc/debug/test/.mooncakes/lijunchen/hello18/lib/lib.core -pkg lijunchen/hello18/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources lijunchen/hello18/lib:./.mooncakes/lijunchen/hello18/lib -target wasm-gc -g -O0 -source-map -workspace-path ./.mooncakes/lijunchen/hello18
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/main/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/main/__internal_test_info.json ./main/main.mbt --target wasm-gc --pkg-name username/hello/main --driver-kind internal
            moonc build-package ./main/main.mbt ./target/wasm-gc/debug/test/main/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/main/main.internal_test.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/.mooncakes/lijunchen/hello18/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/.mooncakes/lijunchen/hello18/lib/lib.core ./target/wasm-gc/debug/test/main/main.internal_test.core -main username/hello/main -o ./target/wasm-gc/debug/test/main/main.internal_test.wasm -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources lijunchen/hello18/lib:./.mooncakes/lijunchen/hello18/lib -pkg-sources username/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/test/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/.mooncakes/lijunchen/hello18/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm-gc -g -O0 -source-map -workspace-path .
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/main/__blackbox_test_info.json --doctest-only ./main/main.mbt --target wasm-gc --pkg-name username/hello/main --driver-kind blackbox
            moonc build-package ./target/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt -doctest-only ./main/main.mbt -o ./target/wasm-gc/debug/test/main/main.blackbox_test.core -pkg username/hello/main_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/main/main.mi:main -i ./target/wasm-gc/debug/test/.mooncakes/lijunchen/hello18/lib/lib.mi:lib -pkg-sources username/hello/main_blackbox_test:./main -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/.mooncakes/lijunchen/hello18/lib/lib.core ./target/wasm-gc/debug/test/main/main.core ./target/wasm-gc/debug/test/main/main.blackbox_test.core -main username/hello/main_blackbox_test -o ./target/wasm-gc/debug/test/main/main.blackbox_test.wasm -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources lijunchen/hello18/lib:./.mooncakes/lijunchen/hello18/lib -pkg-sources username/hello/main:./main -pkg-sources username/hello/main_blackbox_test:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib1/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib1/__internal_test_info.json ./lib1/test.mbt --target wasm-gc --pkg-name username/hello/lib1 --driver-kind internal
            moonc build-package ./lib1/test.mbt ./target/wasm-gc/debug/test/lib1/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib1/lib1.internal_test.core -pkg username/hello/lib1 -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/.mooncakes/lijunchen/hello18/lib/lib.mi:lib -pkg-sources username/hello/lib1:./lib1 -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/.mooncakes/lijunchen/hello18/lib/lib.core ./target/wasm-gc/debug/test/lib1/lib1.internal_test.core -main username/hello/lib1 -o ./target/wasm-gc/debug/test/lib1/lib1.internal_test.wasm -test-mode -pkg-config-path ./lib1/moon.pkg.json -pkg-sources lijunchen/hello18/lib:./.mooncakes/lijunchen/hello18/lib -pkg-sources username/hello/lib1:./lib1 -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moonc build-package ./lib1/test.mbt -o ./target/wasm-gc/debug/test/lib1/lib1.core -pkg username/hello/lib1 -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/.mooncakes/lijunchen/hello18/lib/lib.mi:lib -pkg-sources username/hello/lib1:./lib1 -target wasm-gc -g -O0 -source-map -workspace-path .
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib1/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib1/__blackbox_test_info.json --doctest-only ./lib1/test.mbt --target wasm-gc --pkg-name username/hello/lib1 --driver-kind blackbox
            moonc build-package ./target/wasm-gc/debug/test/lib1/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib1/test.mbt -o ./target/wasm-gc/debug/test/lib1/lib1.blackbox_test.core -pkg username/hello/lib1_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/lib1/lib1.mi:lib1 -i ./target/wasm-gc/debug/test/.mooncakes/lijunchen/hello18/lib/lib.mi:lib -pkg-sources username/hello/lib1_blackbox_test:./lib1 -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/.mooncakes/lijunchen/hello18/lib/lib.core ./target/wasm-gc/debug/test/lib1/lib1.core ./target/wasm-gc/debug/test/lib1/lib1.blackbox_test.core -main username/hello/lib1_blackbox_test -o ./target/wasm-gc/debug/test/lib1/lib1.blackbox_test.wasm -test-mode -pkg-config-path ./lib1/moon.pkg.json -pkg-sources lijunchen/hello18/lib:./.mooncakes/lijunchen/hello18/lib -pkg-sources username/hello/lib1:./lib1 -pkg-sources username/hello/lib1_blackbox_test:./lib1 -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
        "#]],
    );

    check(
        get_stdout(&dir, ["test", "--sort-input"]),
        expect![[r#"
            Hello, world!
            Hello, world!
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );

    let actual = get_stderr(&dir, ["build"]);
    expect![[r#"
        Finished. moon: ran 3 tasks, now up to date
    "#]]
    .assert_eq(&actual);

    let actual = get_stdout(&dir, ["run", "main"]);
    assert!(actual.contains("Hello, world!"));
}

#[test]
fn test_moonbitlang_x() {
    if std::env::var("CI").is_err() {
        return;
    }
    let dir = TestDir::new("test_moonbitlang_x.in");
    get_stdout(&dir, ["update"]);
    get_stdout(&dir, ["install"]);

    let build_output = get_stdout(&dir, ["build", "--dry-run", "--sort-input"]);

    check(
        &build_output,
        expect![[r#"
            moonc build-package ./.mooncakes/moonbitlang/x/stack/deprecated.mbt ./.mooncakes/moonbitlang/x/stack/stack.mbt ./.mooncakes/moonbitlang/x/stack/types.mbt -w -a -alert -all -o ./target/wasm-gc/release/build/.mooncakes/moonbitlang/x/stack/stack.core -pkg moonbitlang/x/stack -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moonbitlang/x/stack:./.mooncakes/moonbitlang/x/stack -target wasm-gc -workspace-path ./.mooncakes/moonbitlang/x
            moonc build-package ./src/lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/.mooncakes/moonbitlang/x/stack/stack.mi:stack -pkg-sources username/hello/lib:./src/lib -target wasm-gc -workspace-path .
            moonc build-package ./src/main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/lib/lib.mi:lib -i ./target/wasm-gc/release/build/.mooncakes/moonbitlang/x/stack/stack.mi:stack -pkg-sources username/hello/main:./src/main -target wasm-gc -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/.mooncakes/moonbitlang/x/stack/stack.core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main username/hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./src/main/moon.pkg.json -pkg-sources moonbitlang/x/stack:./.mooncakes/moonbitlang/x/stack -pkg-sources username/hello/lib:./src/lib -pkg-sources username/hello/main:./src/main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc
        "#]],
    );

    let test_output = get_stdout(&dir, ["test", "--dry-run", "--sort-input"]);
    check(
        &test_output,
        expect![[r#"
            moonc build-package ./.mooncakes/moonbitlang/x/stack/deprecated.mbt ./.mooncakes/moonbitlang/x/stack/stack.mbt ./.mooncakes/moonbitlang/x/stack/types.mbt -w -a -alert -all -o ./target/wasm-gc/debug/test/.mooncakes/moonbitlang/x/stack/stack.core -pkg moonbitlang/x/stack -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moonbitlang/x/stack:./.mooncakes/moonbitlang/x/stack -target wasm-gc -g -O0 -source-map -workspace-path ./.mooncakes/moonbitlang/x
            moonc build-package ./src/lib/hello.mbt -o ./target/wasm-gc/debug/test/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/.mooncakes/moonbitlang/x/stack/stack.mi:stack -pkg-sources username/hello/lib:./src/lib -target wasm-gc -g -O0 -source-map -workspace-path .
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/main/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/main/__internal_test_info.json ./src/main/main.mbt --target wasm-gc --pkg-name username/hello/main --driver-kind internal
            moonc build-package ./src/main/main.mbt ./target/wasm-gc/debug/test/main/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/main/main.internal_test.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -i ./target/wasm-gc/debug/test/.mooncakes/moonbitlang/x/stack/stack.mi:stack -pkg-sources username/hello/main:./src/main -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/.mooncakes/moonbitlang/x/stack/stack.core ./target/wasm-gc/debug/test/lib/lib.core ./target/wasm-gc/debug/test/main/main.internal_test.core -main username/hello/main -o ./target/wasm-gc/debug/test/main/main.internal_test.wasm -test-mode -pkg-config-path ./src/main/moon.pkg.json -pkg-sources moonbitlang/x/stack:./.mooncakes/moonbitlang/x/stack -pkg-sources username/hello/lib:./src/lib -pkg-sources username/hello/main:./src/main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moonc build-package ./src/main/main.mbt -o ./target/wasm-gc/debug/test/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -i ./target/wasm-gc/debug/test/.mooncakes/moonbitlang/x/stack/stack.mi:stack -pkg-sources username/hello/main:./src/main -target wasm-gc -g -O0 -source-map -workspace-path .
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/main/__blackbox_test_info.json --doctest-only ./src/main/main.mbt --target wasm-gc --pkg-name username/hello/main --driver-kind blackbox
            moonc build-package ./target/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt -doctest-only ./src/main/main.mbt -o ./target/wasm-gc/debug/test/main/main.blackbox_test.core -pkg username/hello/main_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/main/main.mi:main -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -i ./target/wasm-gc/debug/test/.mooncakes/moonbitlang/x/stack/stack.mi:stack -pkg-sources username/hello/main_blackbox_test:./src/main -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/.mooncakes/moonbitlang/x/stack/stack.core ./target/wasm-gc/debug/test/lib/lib.core ./target/wasm-gc/debug/test/main/main.core ./target/wasm-gc/debug/test/main/main.blackbox_test.core -main username/hello/main_blackbox_test -o ./target/wasm-gc/debug/test/main/main.blackbox_test.wasm -test-mode -pkg-config-path ./src/main/moon.pkg.json -pkg-sources moonbitlang/x/stack:./.mooncakes/moonbitlang/x/stack -pkg-sources username/hello/lib:./src/lib -pkg-sources username/hello/main:./src/main -pkg-sources username/hello/main_blackbox_test:./src/main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__internal_test_info.json ./src/lib/hello.mbt --target wasm-gc --pkg-name username/hello/lib --driver-kind internal
            moonc build-package ./src/lib/hello.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/.mooncakes/moonbitlang/x/stack/stack.mi:stack -pkg-sources username/hello/lib:./src/lib -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/.mooncakes/moonbitlang/x/stack/stack.core ./target/wasm-gc/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./src/lib/moon.pkg.json -pkg-sources moonbitlang/x/stack:./.mooncakes/moonbitlang/x/stack -pkg-sources username/hello/lib:./src/lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__blackbox_test_info.json ./src/lib/hello_test.mbt --doctest-only ./src/lib/hello.mbt --target wasm-gc --pkg-name username/hello/lib --driver-kind blackbox
            moonc build-package ./src/lib/hello_test.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./src/lib/hello.mbt -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -pkg username/hello/lib_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -i ./target/wasm-gc/debug/test/.mooncakes/moonbitlang/x/stack/stack.mi:stack -pkg-sources username/hello/lib_blackbox_test:./src/lib -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/.mooncakes/moonbitlang/x/stack/stack.core ./target/wasm-gc/debug/test/lib/lib.core ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -main username/hello/lib_blackbox_test -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./src/lib/moon.pkg.json -pkg-sources moonbitlang/x/stack:./.mooncakes/moonbitlang/x/stack -pkg-sources username/hello/lib:./src/lib -pkg-sources username/hello/lib_blackbox_test:./src/lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
        "#]],
    );

    check(
        get_stdout(&dir, ["run", "src/main"]),
        expect![[r#"
            Some(123)
        "#]],
    );
}

#[test]
fn test_blackbox_success() {
    let dir = TestDir::new("blackbox_success_test.in");

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/A",
                "-f",
                "hello_test.mbt",
                "-i",
                "0",
                "--nostd",
                "--sort-input",
                "--dry-run",
            ],
        ),
        expect![[r#"
            moonc build-package ./D/hello.mbt -o ./target/wasm-gc/debug/test/D/D.core -pkg username/hello/D -pkg-sources username/hello/D:./D -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package ./B/hello.mbt -o ./target/wasm-gc/debug/test/B/B.core -pkg username/hello/B -pkg-sources username/hello/B:./B -target wasm-gc -g -O0 -source-map -workspace-path .
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/A/__generated_driver_for_whitebox_test.mbt --output-metadata ./target/wasm-gc/debug/test/A/__whitebox_test_info.json ./A/hello_wbtest.mbt --target wasm-gc --pkg-name username/hello/A --driver-kind whitebox
            moonc build-package ./A/hello.mbt ./A/hello_wbtest.mbt ./target/wasm-gc/debug/test/A/__generated_driver_for_whitebox_test.mbt -o ./target/wasm-gc/debug/test/A/A.whitebox_test.core -pkg username/hello/A -is-main -i ./target/wasm-gc/debug/test/D/D.mi:D -i ./target/wasm-gc/debug/test/B/B.mi:B -pkg-sources username/hello/A:./A -target wasm-gc -g -O0 -source-map -whitebox-test -no-mi -test-mode -workspace-path .
            moonc link-core ./target/wasm-gc/debug/test/D/D.core ./target/wasm-gc/debug/test/B/B.core ./target/wasm-gc/debug/test/A/A.whitebox_test.core -main username/hello/A -o ./target/wasm-gc/debug/test/A/A.whitebox_test.wasm -test-mode -pkg-config-path ./A/moon.pkg.json -pkg-sources username/hello/D:./D -pkg-sources username/hello/B:./B -pkg-sources username/hello/A:./A -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/A/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/A/__internal_test_info.json ./A/hello.mbt --target wasm-gc --pkg-name username/hello/A --driver-kind internal
            moonc build-package ./A/hello.mbt ./target/wasm-gc/debug/test/A/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/A/A.internal_test.core -pkg username/hello/A -is-main -i ./target/wasm-gc/debug/test/D/D.mi:D -pkg-sources username/hello/A:./A -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core ./target/wasm-gc/debug/test/D/D.core ./target/wasm-gc/debug/test/A/A.internal_test.core -main username/hello/A -o ./target/wasm-gc/debug/test/A/A.internal_test.wasm -test-mode -pkg-config-path ./A/moon.pkg.json -pkg-sources username/hello/D:./D -pkg-sources username/hello/A:./A -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moonc build-package ./C/hello.mbt -o ./target/wasm-gc/debug/test/C/C.core -pkg username/hello/C -pkg-sources username/hello/C:./C -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package ./A/hello.mbt -o ./target/wasm-gc/debug/test/A/A.core -pkg username/hello/A -i ./target/wasm-gc/debug/test/D/D.mi:D -pkg-sources username/hello/A:./A -target wasm-gc -g -O0 -source-map -workspace-path .
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/A/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/A/__blackbox_test_info.json ./A/hello_test.mbt --doctest-only ./A/hello.mbt --target wasm-gc --pkg-name username/hello/A --driver-kind blackbox
            moonc build-package ./A/hello_test.mbt ./target/wasm-gc/debug/test/A/__generated_driver_for_blackbox_test.mbt -doctest-only ./A/hello.mbt -o ./target/wasm-gc/debug/test/A/A.blackbox_test.core -pkg username/hello/A_blackbox_test -is-main -i ./target/wasm-gc/debug/test/A/A.mi:A -i ./target/wasm-gc/debug/test/D/D.mi:D -i ./target/wasm-gc/debug/test/C/C.mi:C -pkg-sources username/hello/A_blackbox_test:./A -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core ./target/wasm-gc/debug/test/D/D.core ./target/wasm-gc/debug/test/C/C.core ./target/wasm-gc/debug/test/A/A.core ./target/wasm-gc/debug/test/A/A.blackbox_test.core -main username/hello/A_blackbox_test -o ./target/wasm-gc/debug/test/A/A.blackbox_test.wasm -test-mode -pkg-config-path ./A/moon.pkg.json -pkg-sources username/hello/D:./D -pkg-sources username/hello/C:./C -pkg-sources username/hello/A:./A -pkg-sources username/hello/A_blackbox_test:./A -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/A",
                "-f",
                "hello_test.mbt",
                "-i",
                "0",
            ],
        ),
        expect![[r#"
            output from A/hello.mbt!
            output from C/hello.mbt!
            output from D/hello.mbt!
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout(&dir, ["test"]),
        expect![[r#"
            output from A/hello.mbt!
            output from C/hello.mbt!
            output from D/hello.mbt!
            self.a: 33
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );

    check(
        get_stdout(&dir, ["check", "--sort-input", "--dry-run"]),
        expect![[r#"
            moonc check ./main/main.mbt -o ./target/wasm-gc/release/check/main/main.mi -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/main:./main -target wasm-gc -workspace-path .
            moonc check -doctest-only ./main/main.mbt -include-doctests -o ./target/wasm-gc/release/check/main/main.blackbox_test.mi -pkg username/hello/main_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/main/main.mi:main -pkg-sources username/hello/main_blackbox_test:./main -target wasm-gc -blackbox-test -workspace-path .
            moonc check ./D/hello.mbt -o ./target/wasm-gc/release/check/D/D.mi -pkg username/hello/D -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/D:./D -target wasm-gc -workspace-path .
            moonc check -doctest-only ./D/hello.mbt -include-doctests -o ./target/wasm-gc/release/check/D/D.blackbox_test.mi -pkg username/hello/D_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/D/D.mi:D -pkg-sources username/hello/D_blackbox_test:./D -target wasm-gc -blackbox-test -workspace-path .
            moonc check ./C/hello.mbt -o ./target/wasm-gc/release/check/C/C.mi -pkg username/hello/C -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/C:./C -target wasm-gc -workspace-path .
            moonc check -doctest-only ./C/hello.mbt -include-doctests -o ./target/wasm-gc/release/check/C/C.blackbox_test.mi -pkg username/hello/C_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/C/C.mi:C -pkg-sources username/hello/C_blackbox_test:./C -target wasm-gc -blackbox-test -workspace-path .
            moonc check ./B/hello.mbt -o ./target/wasm-gc/release/check/B/B.mi -pkg username/hello/B -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/B:./B -target wasm-gc -workspace-path .
            moonc check -doctest-only ./B/hello.mbt -include-doctests -o ./target/wasm-gc/release/check/B/B.blackbox_test.mi -pkg username/hello/B_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/B/B.mi:B -pkg-sources username/hello/B_blackbox_test:./B -target wasm-gc -blackbox-test -workspace-path .
            moonc check ./A/hello.mbt ./A/hello_wbtest.mbt -o ./target/wasm-gc/release/check/A/A.whitebox_test.mi -pkg username/hello/A -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/D/D.mi:D -i ./target/wasm-gc/release/check/B/B.mi:B -pkg-sources username/hello/A:./A -target wasm-gc -whitebox-test -workspace-path .
            moonc check ./A/hello.mbt -o ./target/wasm-gc/release/check/A/A.mi -pkg username/hello/A -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/D/D.mi:D -pkg-sources username/hello/A:./A -target wasm-gc -workspace-path .
            moonc check ./A/hello_test.mbt -doctest-only ./A/hello.mbt -include-doctests -o ./target/wasm-gc/release/check/A/A.blackbox_test.mi -pkg username/hello/A_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/A/A.mi:A -i ./target/wasm-gc/release/check/D/D.mi:D -i ./target/wasm-gc/release/check/C/C.mi:C -pkg-sources username/hello/A_blackbox_test:./A -target wasm-gc -blackbox-test -workspace-path .
        "#]],
    );

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["check", "--sort-input"])
        .assert()
        .success();

    #[cfg(unix)]
    {
        let p = dir.join("target/packages.json");
        check(
            replace_dir(&std::fs::read_to_string(p).unwrap(), &dir),
            expect![[r#"
                {
                  "source_dir": "$ROOT",
                  "name": "username/hello",
                  "packages": [
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/A",
                      "root": "username/hello",
                      "rel": "A",
                      "files": {
                        "$ROOT/A/hello.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        }
                      },
                      "wbtest-files": {
                        "$ROOT/A/hello_wbtest.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        }
                      },
                      "test-files": {
                        "$ROOT/A/hello_test.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        }
                      },
                      "mbt-md-files": {},
                      "deps": [
                        {
                          "path": "username/hello/D",
                          "alias": "D",
                          "fspath": "$ROOT/D"
                        }
                      ],
                      "wbtest-deps": [
                        {
                          "path": "username/hello/B",
                          "alias": "B",
                          "fspath": "$ROOT/B"
                        }
                      ],
                      "test-deps": [
                        {
                          "path": "username/hello/C",
                          "alias": "C",
                          "fspath": "$ROOT/C"
                        }
                      ],
                      "artifact": "$ROOT/target/wasm-gc/release/check/A/A.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/B",
                      "root": "username/hello",
                      "rel": "B",
                      "files": {
                        "$ROOT/B/hello.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        }
                      },
                      "wbtest-files": {},
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [],
                      "wbtest-deps": [],
                      "test-deps": [],
                      "artifact": "$ROOT/target/wasm-gc/release/check/B/B.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/C",
                      "root": "username/hello",
                      "rel": "C",
                      "files": {
                        "$ROOT/C/hello.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        }
                      },
                      "wbtest-files": {},
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [],
                      "wbtest-deps": [],
                      "test-deps": [],
                      "artifact": "$ROOT/target/wasm-gc/release/check/C/C.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/D",
                      "root": "username/hello",
                      "rel": "D",
                      "files": {
                        "$ROOT/D/hello.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        }
                      },
                      "wbtest-files": {},
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [],
                      "wbtest-deps": [],
                      "test-deps": [],
                      "artifact": "$ROOT/target/wasm-gc/release/check/D/D.mi"
                    },
                    {
                      "is-main": true,
                      "is-third-party": false,
                      "root-path": "$ROOT/main",
                      "root": "username/hello",
                      "rel": "main",
                      "files": {
                        "$ROOT/main/main.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        }
                      },
                      "wbtest-files": {},
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [],
                      "wbtest-deps": [],
                      "test-deps": [],
                      "artifact": "$ROOT/target/wasm-gc/release/check/main/main.mi"
                    }
                  ],
                  "deps": [],
                  "backend": "wasm-gc",
                  "opt_level": "release",
                  "source": null
                }"#]],
        );
    }
}

#[test]
fn test_blackbox_failed() {
    let dir = TestDir::new("blackbox_failed_test.in");

    let output = snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .arg("test")
        .assert()
        .failure()
        .get_output()
        .stderr
        .to_owned();

    let output = String::from_utf8_lossy(&output);
    // bbtest can not use private function in bbtest_import
    assert!(output.contains("Value _private_hello not found in package `A`"));
    // bbtest_import could no be used in _wbtest.mbt
    assert!(output.contains("Package \"C\" not found in the loaded packages."));

    let output = snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["check"])
        .assert()
        .failure()
        .get_output()
        .stderr
        .to_owned();

    let output = String::from_utf8_lossy(&output);
    assert!(output.contains("Warning: Unused variable 'a'"));
    assert!(output.contains("Warning: Unused variable 'b'"));
    assert!(output.contains("Value _private_hello not found in package `A`"));
    assert!(output.contains("Package \"C\" not found in the loaded packages."));
}

#[test]
fn test_blackbox_test_core_override() {
    let dir = TestDir::new("blackbox_test_core_override.in");
    let output = get_stdout(
        &dir,
        ["test", "--enable-coverage", "--dry-run", "--sort-input"],
    );
    check(
        &output,
        expect![[r#"
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/prelude/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/prelude/__internal_test_info.json --target wasm-gc --pkg-name moonbitlang/core/prelude --enable-coverage --driver-kind internal
            moonc build-package ./target/wasm-gc/debug/test/prelude/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/prelude/prelude.internal_test.core -pkg moonbitlang/core/prelude -is-main -pkg-sources moonbitlang/core/prelude:./prelude -target wasm-gc -g -O0 -source-map -enable-coverage -no-mi -test-mode -workspace-path .
            moonc link-core ./target/wasm-gc/debug/test/prelude/prelude.internal_test.core -main moonbitlang/core/prelude -o ./target/wasm-gc/debug/test/prelude/prelude.internal_test.wasm -test-mode -pkg-config-path ./prelude/moon.pkg.json -pkg-sources moonbitlang/core/prelude:./prelude -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moonc build-package -o ./target/wasm-gc/debug/test/prelude/prelude.core -pkg moonbitlang/core/prelude -pkg-sources moonbitlang/core/prelude:./prelude -target wasm-gc -g -O0 -source-map -enable-coverage -workspace-path .
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/prelude/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/prelude/__blackbox_test_info.json --target wasm-gc --pkg-name moonbitlang/core/prelude --enable-coverage --driver-kind blackbox
            moonc build-package ./target/wasm-gc/debug/test/prelude/__generated_driver_for_blackbox_test.mbt -o ./target/wasm-gc/debug/test/prelude/prelude.blackbox_test.core -pkg moonbitlang/core/prelude_blackbox_test -is-main -i ./target/wasm-gc/debug/test/prelude/prelude.mi:moonbitlang/core/prelude -pkg-sources moonbitlang/core/prelude_blackbox_test:./prelude -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core ./target/wasm-gc/debug/test/prelude/prelude.core ./target/wasm-gc/debug/test/prelude/prelude.blackbox_test.core -main moonbitlang/core/prelude_blackbox_test -o ./target/wasm-gc/debug/test/prelude/prelude.blackbox_test.wasm -test-mode -pkg-config-path ./prelude/moon.pkg.json -pkg-sources moonbitlang/core/prelude:./prelude -pkg-sources moonbitlang/core/prelude_blackbox_test:./prelude -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/builtin/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/builtin/__internal_test_info.json ./builtin/main.mbt --target wasm-gc --pkg-name moonbitlang/core/builtin --enable-coverage --coverage-package-override=@self --driver-kind internal
            moonc build-package ./builtin/main.mbt ./target/wasm-gc/debug/test/builtin/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/builtin/builtin.internal_test.core -pkg moonbitlang/core/builtin -is-main -pkg-sources moonbitlang/core/builtin:./builtin -target wasm-gc -g -O0 -source-map -enable-coverage -coverage-package-override=@self -no-mi -test-mode -workspace-path .
            moonc link-core ./target/wasm-gc/debug/test/builtin/builtin.internal_test.core -main moonbitlang/core/builtin -o ./target/wasm-gc/debug/test/builtin/builtin.internal_test.wasm -test-mode -pkg-config-path ./builtin/moon.pkg.json -pkg-sources moonbitlang/core/builtin:./builtin -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moonc build-package ./builtin/main.mbt -o ./target/wasm-gc/debug/test/builtin/builtin.core -pkg moonbitlang/core/builtin -pkg-sources moonbitlang/core/builtin:./builtin -target wasm-gc -g -O0 -source-map -enable-coverage -coverage-package-override=@self -workspace-path .
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/builtin/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/builtin/__blackbox_test_info.json ./builtin/main_test.mbt --doctest-only ./builtin/main.mbt --target wasm-gc --pkg-name moonbitlang/core/builtin --enable-coverage --coverage-package-override=@self --driver-kind blackbox
            moonc build-package ./builtin/main_test.mbt ./target/wasm-gc/debug/test/builtin/__generated_driver_for_blackbox_test.mbt -doctest-only ./builtin/main.mbt -o ./target/wasm-gc/debug/test/builtin/builtin.blackbox_test.core -pkg moonbitlang/core/builtin_blackbox_test -is-main -i ./target/wasm-gc/debug/test/builtin/builtin.mi:builtin -i ./target/wasm-gc/debug/test/prelude/prelude.mi:prelude -pkg-sources moonbitlang/core/builtin_blackbox_test:./builtin -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core ./target/wasm-gc/debug/test/prelude/prelude.core ./target/wasm-gc/debug/test/builtin/builtin.core ./target/wasm-gc/debug/test/builtin/builtin.blackbox_test.core -main moonbitlang/core/builtin_blackbox_test -o ./target/wasm-gc/debug/test/builtin/builtin.blackbox_test.wasm -test-mode -pkg-config-path ./builtin/moon.pkg.json -pkg-sources moonbitlang/core/prelude:./prelude -pkg-sources moonbitlang/core/builtin:./builtin -pkg-sources moonbitlang/core/builtin_blackbox_test:./builtin -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
        "#]],
    );

    let mut found = false;
    for line in output.lines() {
        // For the command compiling builtin's blackbox tests,
        if line.contains("moonc build-package") && line.contains("builtin_blackbox_test") {
            found = true;
            // it should not have the -enable-coverage flag
            assert!(
                !line.contains("-enable-coverage"),
                "Black box tests themselves should not contain coverage, since all they contain are tests of various kinds. {line}"
            );
            // and should not contain -coverage-package-override to itself
            assert!(
                !line.contains("-coverage-package-override=@self"),
                "Unexpected -coverage-package-override=@self found in the command: {line}"
            );
        }
    }
    assert!(found, "builtin's blackbox tests not found in the output");
}

#[test]
fn test_blackbox_dedup_alias() {
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("RUST_BACKTRACE", "0") };
    let dir = TestDir::new("blackbox_test_dedup_alias.in");
    let output = get_err_stderr(&dir, ["test"]);
    check(
        &output,
        expect![[r#"
            Warning: Duplicate alias `lib` at "$ROOT/lib/moon.pkg.json". "test-import" will automatically add "import" and current package as dependency so you don't need to add it manually. If you're test-importing a dependency with the same default alias as your current package, considering give it a different alias than the current package. Violating import: `username/hello/dir/lib`
            Error: [4021]
               ╭─[ $ROOT/lib/hello_test.mbt:3:3 ]
               │
             3 │   @lib.hello()
               │   ─────┬────  
               │        ╰────── Value hello not found in package `lib`.
            ───╯
            Warning: [0029]
               ╭─[ $ROOT/lib/moon.pkg.json:3:5 ]
               │
             3 │     "username/hello/dir/lib"
               │     ────────────┬───────────  
               │                 ╰───────────── Warning: Unused package 'username/hello/dir/lib'
            ───╯
            Failed with 1 warnings, 1 errors.
            error: failed when testing project
        "#]],
    );
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
            moonc build-package ./lib/hello.mbt -o ./target/wasm/release/build/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target wasm -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/wasm/release/build/main/main.core -pkg username/hello/main -is-main -i ./target/wasm/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm -workspace-path .
            moonc link-core ./target/wasm/release/build/lib/lib.core ./target/wasm/release/build/main/main.core -main username/hello/main -o ./target/wasm/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -target wasm -import-memory-module xxx -import-memory-name yyy -heap-start-address 65536
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
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm-gc -workspace-path .
            moonc link-core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main username/hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -target wasm-gc -import-memory-module xxx -import-memory-name yyy
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
            moonc build-package ./lib/hello.mbt -o ./target/wasm/release/build/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target wasm -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/wasm/release/build/main/main.core -pkg username/hello/main -is-main -i ./target/wasm/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm -workspace-path .
            moonc link-core ./target/wasm/release/build/lib/lib.core ./target/wasm/release/build/main/main.core -main username/hello/main -o ./target/wasm/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -target wasm -import-memory-module xxx -import-memory-name yyy -memory-limits-min 1 -memory-limits-max 65536 -shared-memory -heap-start-address 65536
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
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm-gc -workspace-path .
            moonc link-core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main username/hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -target wasm-gc -import-memory-module xxx -import-memory-name yyy -memory-limits-min 1 -memory-limits-max 65535 -shared-memory
        "#]],
    );
}

#[cfg(unix)]
#[test]
fn test_moon_run_native() {
    let dir = TestDir::new("run_single_mbt_file.in");

    let output = get_stdout(
        &dir,
        ["run", "a/b/single.mbt", "--target", "native", "--dry-run"],
    );
    check(
        &output,
        expect![[r#"
            moonc build-package $ROOT/a/b/single.mbt -o $ROOT/a/b/target/single.core -std-path $MOON_HOME/lib/core/target/native/release/bundle -is-main -pkg moon/run/single -g -O0 -source-map -target native
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core $ROOT/a/b/target/single.core -o $ROOT/a/b/target/single.c -pkg-sources moon/run/single:$ROOT/a/b -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -g -O0 -source-map -target native
            $MOON_HOME/bin/internal/tcc -I$MOON_HOME/include -L$MOON_HOME/lib $MOON_HOME/lib/runtime.c -lm -DMOONBIT_NATIVE_NO_SYS_HEADER -run $ROOT/a/b/target/single.c
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
            moonc build-package $ROOT/a/b/single.mbt -o $ROOT/a/b/target/single.core -std-path $MOON_HOME/lib/core/target/native/release/bundle -is-main -pkg moon/run/single -g -O0 -source-map -target native
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core $ROOT/a/b/target/single.core -o $ROOT/a/b/target/single.c -pkg-sources moon/run/single:$ROOT/a/b -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -g -O0 -source-map -target native
            cc -o $ROOT/a/b/target/single.exe -I$MOON_HOME/include -fwrapv -fno-strict-aliasing -O0 $MOON_HOME/lib/runtime.c $ROOT/a/b/target/single.c -lm
            $ROOT/a/b/target/single.exe
        "#]],
    );
}

#[test]
fn test_moon_run_single_mbt_file() {
    let dir = TestDir::new("run_single_mbt_file.in");

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
            moonc build-package $ROOT/a/b/single.mbt -o $ROOT/a/b/target/single.core -std-path $MOON_HOME/lib/core/target/js/release/bundle -is-main -pkg moon/run/single -g -O0 -source-map -target js
            moonc link-core $MOON_HOME/lib/core/target/js/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/js/release/bundle/core.core $ROOT/a/b/target/single.core -o $ROOT/a/b/target/single.js -pkg-sources moon/run/single:$ROOT/a/b -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -g -O0 -source-map -target js
        "#]],
    );

    let output = get_stdout(
        &dir,
        ["run", "a/b/single.mbt", "--target", "js", "--build-only"],
    );
    check(
        &output,
        expect![[r#"
            {"artifacts_path":["$ROOT/a/b/target/single.js"]}
        "#]],
    );
    assert!(dir.join("a/b/target/single.js").exists());

    let output = get_stdout(&dir, ["run", "a/b/single.mbt", "--dry-run"]);
    check(
        &output,
        expect![[r#"
            moonc build-package $ROOT/a/b/single.mbt -o $ROOT/a/b/target/single.core -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -is-main -pkg moon/run/single -g -O0 -source-map -target wasm-gc
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core $ROOT/a/b/target/single.core -o $ROOT/a/b/target/single.wasm -pkg-sources moon/run/single:$ROOT/a/b -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -g -O0 -source-map -target wasm-gc
            moonrun $ROOT/a/b/target/single.wasm
        "#]],
    );

    let output = get_stdout(
        &dir,
        ["run", "a/b/single.mbt", "--target", "js", "--dry-run"],
    );
    check(
        &output,
        expect![[r#"
            moonc build-package $ROOT/a/b/single.mbt -o $ROOT/a/b/target/single.core -std-path $MOON_HOME/lib/core/target/js/release/bundle -is-main -pkg moon/run/single -g -O0 -source-map -target js
            moonc link-core $MOON_HOME/lib/core/target/js/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/js/release/bundle/core.core $ROOT/a/b/target/single.core -o $ROOT/a/b/target/single.js -pkg-sources moon/run/single:$ROOT/a/b -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -g -O0 -source-map -target js
            node $ROOT/a/b/target/single.js
        "#]],
    );

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
fn test_moon_check_json_output() {
    let dir = TestDir::new("warns/deny_warn");

    check(
        get_stdout(&dir, ["check", "--output-json", "-q"]),
        expect![[r#"
            {"$message_type":"diagnostic","level":"warning","loc":{"path":"$ROOT/lib/hello.mbt","start":{"line":4,"col":7},"end":{"line":4,"col":8}},"message":"Warning: Unused variable 'a'","error_code":2}
            {"$message_type":"diagnostic","level":"warning","loc":{"path":"$ROOT/lib/hello.mbt","start":{"line":11,"col":7},"end":{"line":11,"col":9}},"message":"Warning: Unused variable '中文'","error_code":2}
            {"$message_type":"diagnostic","level":"warning","loc":{"path":"$ROOT/lib/hello.mbt","start":{"line":12,"col":7},"end":{"line":12,"col":12}},"message":"Warning: Unused variable '🤣😭🤣😭🤣'","error_code":2}
            {"$message_type":"diagnostic","level":"warning","loc":{"path":"$ROOT/main/main.mbt","start":{"line":2,"col":7},"end":{"line":2,"col":8}},"message":"Warning: Unused variable 'a'","error_code":2}
        "#]],
    );
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
fn test_specify_source_dir_001() {
    let dir = TestDir::new("specify_source_dir_001.in");
    check(
        get_stdout(&dir, ["check", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc check ./src/lib/hello.mbt -o ./target/wasm-gc/release/check/lib/lib.mi -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./src/lib -target wasm-gc -workspace-path .
            moonc check ./src/main/main.mbt -o ./target/wasm-gc/release/check/main/main.mi -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib/lib.mi:lib -pkg-sources username/hello/main:./src/main -target wasm-gc -workspace-path .
            moonc check -doctest-only ./src/main/main.mbt -include-doctests -o ./target/wasm-gc/release/check/main/main.blackbox_test.mi -pkg username/hello/main_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/main/main.mi:main -i ./target/wasm-gc/release/check/lib/lib.mi:lib -pkg-sources username/hello/main_blackbox_test:./src/main -target wasm-gc -blackbox-test -workspace-path .
            moonc check ./src/lib/hello_test.mbt -doctest-only ./src/lib/hello.mbt -include-doctests -o ./target/wasm-gc/release/check/lib/lib.blackbox_test.mi -pkg username/hello/lib_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib/lib.mi:lib -pkg-sources username/hello/lib_blackbox_test:./src/lib -target wasm-gc -blackbox-test -workspace-path .
        "#]],
    );
    check(
        get_stdout(&dir, ["build", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./src/lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./src/lib -target wasm-gc -workspace-path .
            moonc build-package ./src/main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./src/main -target wasm-gc -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main username/hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./src/main/moon.pkg.json -pkg-sources username/hello/lib:./src/lib -pkg-sources username/hello/main:./src/main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc
        "#]],
    );
    check(
        get_stdout(&dir, ["test", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./src/lib/hello.mbt -o ./target/wasm-gc/debug/test/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./src/lib -target wasm-gc -g -O0 -source-map -workspace-path .
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/main/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/main/__internal_test_info.json ./src/main/main.mbt --target wasm-gc --pkg-name username/hello/main --driver-kind internal
            moonc build-package ./src/main/main.mbt ./target/wasm-gc/debug/test/main/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/main/main.internal_test.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources username/hello/main:./src/main -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.core ./target/wasm-gc/debug/test/main/main.internal_test.core -main username/hello/main -o ./target/wasm-gc/debug/test/main/main.internal_test.wasm -test-mode -pkg-config-path ./src/main/moon.pkg.json -pkg-sources username/hello/lib:./src/lib -pkg-sources username/hello/main:./src/main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moonc build-package ./src/main/main.mbt -o ./target/wasm-gc/debug/test/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources username/hello/main:./src/main -target wasm-gc -g -O0 -source-map -workspace-path .
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/main/__blackbox_test_info.json --doctest-only ./src/main/main.mbt --target wasm-gc --pkg-name username/hello/main --driver-kind blackbox
            moonc build-package ./target/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt -doctest-only ./src/main/main.mbt -o ./target/wasm-gc/debug/test/main/main.blackbox_test.core -pkg username/hello/main_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/main/main.mi:main -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources username/hello/main_blackbox_test:./src/main -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.core ./target/wasm-gc/debug/test/main/main.core ./target/wasm-gc/debug/test/main/main.blackbox_test.core -main username/hello/main_blackbox_test -o ./target/wasm-gc/debug/test/main/main.blackbox_test.wasm -test-mode -pkg-config-path ./src/main/moon.pkg.json -pkg-sources username/hello/lib:./src/lib -pkg-sources username/hello/main:./src/main -pkg-sources username/hello/main_blackbox_test:./src/main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__internal_test_info.json ./src/lib/hello.mbt --target wasm-gc --pkg-name username/hello/lib --driver-kind internal
            moonc build-package ./src/lib/hello.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./src/lib -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./src/lib/moon.pkg.json -pkg-sources username/hello/lib:./src/lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__blackbox_test_info.json ./src/lib/hello_test.mbt --doctest-only ./src/lib/hello.mbt --target wasm-gc --pkg-name username/hello/lib --driver-kind blackbox
            moonc build-package ./src/lib/hello_test.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./src/lib/hello.mbt -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -pkg username/hello/lib_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources username/hello/lib_blackbox_test:./src/lib -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.core ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -main username/hello/lib_blackbox_test -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./src/lib/moon.pkg.json -pkg-sources username/hello/lib:./src/lib -pkg-sources username/hello/lib_blackbox_test:./src/lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
        "#]],
    );
    check(
        get_stderr(&dir, ["check", "--sort-input"]),
        expect![[r#"
            Finished. moon: ran 4 tasks, now up to date
        "#]],
    );
    #[cfg(unix)]
    {
        let p = dir.join("target/packages.json");
        check(
            replace_dir(&std::fs::read_to_string(p).unwrap(), &dir),
            expect![[r#"
                {
                  "source_dir": "$ROOT",
                  "name": "username/hello",
                  "packages": [
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/src/lib",
                      "root": "username/hello",
                      "rel": "lib",
                      "files": {
                        "$ROOT/src/lib/hello.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        }
                      },
                      "wbtest-files": {},
                      "test-files": {
                        "$ROOT/src/lib/hello_test.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        }
                      },
                      "mbt-md-files": {},
                      "deps": [],
                      "wbtest-deps": [],
                      "test-deps": [],
                      "artifact": "$ROOT/target/wasm-gc/release/check/lib/lib.mi"
                    },
                    {
                      "is-main": true,
                      "is-third-party": false,
                      "root-path": "$ROOT/src/main",
                      "root": "username/hello",
                      "rel": "main",
                      "files": {
                        "$ROOT/src/main/main.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        }
                      },
                      "wbtest-files": {},
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [
                        {
                          "path": "username/hello/lib",
                          "alias": "lib",
                          "fspath": "$ROOT/src/lib"
                        }
                      ],
                      "wbtest-deps": [],
                      "test-deps": [],
                      "artifact": "$ROOT/target/wasm-gc/release/check/main/main.mi"
                    }
                  ],
                  "deps": [],
                  "backend": "wasm-gc",
                  "opt_level": "release",
                  "source": "src"
                }"#]],
        )
    }
    check(
        get_stderr(&dir, ["build"]),
        expect![[r#"
            Finished. moon: ran 3 tasks, now up to date
        "#]],
    );
    check(
        get_stdout(&dir, ["test"]),
        expect![[r#"
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    check(
        get_stdout(&dir, ["run", "./src/main"]),
        expect![[r#"
            Hello, world!
        "#]],
    );
}

#[test]
fn test_specify_source_dir_002() {
    let dir = TestDir::new("specify_source_dir_002.in");
    check(
        get_err_stdout(&dir, ["test"]),
        expect![[r#"
            [username/hello] test lib/hello_test.mbt:1 ("hello") failed
            expect test failed at $ROOT/src/lib/hello_test.mbt:2:3-2:24
            Diff: (- expected, + actual)
            ----
            +Hello, world!
            ----

            Total tests: 1, passed: 0, failed: 1.
        "#]],
    );
    check(
        get_stdout(&dir, ["test", "-u", "--no-parallelize"]),
        expect![[r#"

            Auto updating expect tests and retesting ...

            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        read(dir.join("src").join("lib").join("hello_test.mbt")),
        expect![[r#"
            test "hello" {
              inspect(@lib.hello(), content="Hello, world!")
            }
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
            {"artifacts_path":["$ROOT/target/js/release/build/main/main.js"]}
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
    check(
        get_err_stderr(&dir, ["check"]),
        expect![[r#"
            error: failed to load `$ROOT/moon.mod.json`

            Caused by:
                0: `source` bad format
                1: `source` not a subdirectory of the parent directory
        "#]],
    );
}

#[test]
fn test_specify_source_dir_with_deps() {
    let dir = TestDir::new("specify_source_dir_with_deps_001.in");
    check(
        get_stdout(&dir, ["check", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc check ./anyhow/lib/hello.mbt -o ./target/wasm-gc/release/check/lib/lib.mi -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./anyhow/lib -target wasm-gc -workspace-path .
            moonc check ./deps/hello19/source/lib/hello.mbt -w -a -alert -all -o ./target/wasm-gc/release/check/.mooncakes/just/hello19/lib/lib.mi -pkg just/hello19/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources just/hello19/lib:./deps/hello19/source/lib -target wasm-gc -workspace-path ./deps/hello19
            moonc check ./deps/hello19/source/top.mbt -w -a -alert -all -o ./target/wasm-gc/release/check/.mooncakes/just/hello19/hello19.mi -pkg just/hello19 -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources just/hello19:./deps/hello19/source -target wasm-gc -workspace-path ./deps/hello19
            moonc check ./anyhow/main/main.mbt -o ./target/wasm-gc/release/check/main/main.mi -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib/lib.mi:emmm -i ./target/wasm-gc/release/check/.mooncakes/just/hello19/hello19.mi:hello19 -i ./target/wasm-gc/release/check/.mooncakes/just/hello19/lib/lib.mi:lib -pkg-sources username/hello/main:./anyhow/main -target wasm-gc -workspace-path .
            moonc check -doctest-only ./anyhow/main/main.mbt -include-doctests -o ./target/wasm-gc/release/check/main/main.blackbox_test.mi -pkg username/hello/main_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/main/main.mi:main -i ./target/wasm-gc/release/check/lib/lib.mi:emmm -i ./target/wasm-gc/release/check/.mooncakes/just/hello19/hello19.mi:hello19 -i ./target/wasm-gc/release/check/.mooncakes/just/hello19/lib/lib.mi:lib -pkg-sources username/hello/main_blackbox_test:./anyhow/main -target wasm-gc -blackbox-test -workspace-path .
            moonc check ./anyhow/lib/hello_test.mbt -doctest-only ./anyhow/lib/hello.mbt -include-doctests -o ./target/wasm-gc/release/check/lib/lib.blackbox_test.mi -pkg username/hello/lib_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib/lib.mi:lib -pkg-sources username/hello/lib_blackbox_test:./anyhow/lib -target wasm-gc -blackbox-test -workspace-path .
        "#]],
    );
    check(
        get_stdout(&dir, ["build", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./anyhow/lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./anyhow/lib -target wasm-gc -workspace-path .
            moonc build-package ./deps/hello19/source/lib/hello.mbt -w -a -alert -all -o ./target/wasm-gc/release/build/.mooncakes/just/hello19/lib/lib.core -pkg just/hello19/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources just/hello19/lib:./deps/hello19/source/lib -target wasm-gc -workspace-path ./deps/hello19
            moonc build-package ./deps/hello19/source/top.mbt -w -a -alert -all -o ./target/wasm-gc/release/build/.mooncakes/just/hello19/hello19.core -pkg just/hello19 -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources just/hello19:./deps/hello19/source -target wasm-gc -workspace-path ./deps/hello19
            moonc build-package ./anyhow/main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/lib/lib.mi:emmm -i ./target/wasm-gc/release/build/.mooncakes/just/hello19/hello19.mi:hello19 -i ./target/wasm-gc/release/build/.mooncakes/just/hello19/lib/lib.mi:lib -pkg-sources username/hello/main:./anyhow/main -target wasm-gc -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/.mooncakes/just/hello19/hello19.core ./target/wasm-gc/release/build/.mooncakes/just/hello19/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main username/hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./anyhow/main/moon.pkg.json -pkg-sources username/hello/lib:./anyhow/lib -pkg-sources just/hello19:./deps/hello19/source -pkg-sources just/hello19/lib:./deps/hello19/source/lib -pkg-sources username/hello/main:./anyhow/main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc
        "#]],
    );
    check(
        get_stdout(&dir, ["test", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./anyhow/lib/hello.mbt -o ./target/wasm-gc/debug/test/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./anyhow/lib -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package ./deps/hello19/source/lib/hello.mbt -w -a -alert -all -o ./target/wasm-gc/debug/test/.mooncakes/just/hello19/lib/lib.core -pkg just/hello19/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources just/hello19/lib:./deps/hello19/source/lib -target wasm-gc -g -O0 -source-map -workspace-path ./deps/hello19
            moonc build-package ./deps/hello19/source/top.mbt -w -a -alert -all -o ./target/wasm-gc/debug/test/.mooncakes/just/hello19/hello19.core -pkg just/hello19 -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources just/hello19:./deps/hello19/source -target wasm-gc -g -O0 -source-map -workspace-path ./deps/hello19
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/main/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/main/__internal_test_info.json ./anyhow/main/main.mbt --target wasm-gc --pkg-name username/hello/main --driver-kind internal
            moonc build-package ./anyhow/main/main.mbt ./target/wasm-gc/debug/test/main/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/main/main.internal_test.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/lib/lib.mi:emmm -i ./target/wasm-gc/debug/test/.mooncakes/just/hello19/hello19.mi:hello19 -i ./target/wasm-gc/debug/test/.mooncakes/just/hello19/lib/lib.mi:lib -pkg-sources username/hello/main:./anyhow/main -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.core ./target/wasm-gc/debug/test/.mooncakes/just/hello19/hello19.core ./target/wasm-gc/debug/test/.mooncakes/just/hello19/lib/lib.core ./target/wasm-gc/debug/test/main/main.internal_test.core -main username/hello/main -o ./target/wasm-gc/debug/test/main/main.internal_test.wasm -test-mode -pkg-config-path ./anyhow/main/moon.pkg.json -pkg-sources username/hello/lib:./anyhow/lib -pkg-sources just/hello19:./deps/hello19/source -pkg-sources just/hello19/lib:./deps/hello19/source/lib -pkg-sources username/hello/main:./anyhow/main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moonc build-package ./anyhow/main/main.mbt -o ./target/wasm-gc/debug/test/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/lib/lib.mi:emmm -i ./target/wasm-gc/debug/test/.mooncakes/just/hello19/hello19.mi:hello19 -i ./target/wasm-gc/debug/test/.mooncakes/just/hello19/lib/lib.mi:lib -pkg-sources username/hello/main:./anyhow/main -target wasm-gc -g -O0 -source-map -workspace-path .
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/main/__blackbox_test_info.json --doctest-only ./anyhow/main/main.mbt --target wasm-gc --pkg-name username/hello/main --driver-kind blackbox
            moonc build-package ./target/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt -doctest-only ./anyhow/main/main.mbt -o ./target/wasm-gc/debug/test/main/main.blackbox_test.core -pkg username/hello/main_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/main/main.mi:main -i ./target/wasm-gc/debug/test/lib/lib.mi:emmm -i ./target/wasm-gc/debug/test/.mooncakes/just/hello19/hello19.mi:hello19 -i ./target/wasm-gc/debug/test/.mooncakes/just/hello19/lib/lib.mi:lib -pkg-sources username/hello/main_blackbox_test:./anyhow/main -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.core ./target/wasm-gc/debug/test/.mooncakes/just/hello19/hello19.core ./target/wasm-gc/debug/test/.mooncakes/just/hello19/lib/lib.core ./target/wasm-gc/debug/test/main/main.core ./target/wasm-gc/debug/test/main/main.blackbox_test.core -main username/hello/main_blackbox_test -o ./target/wasm-gc/debug/test/main/main.blackbox_test.wasm -test-mode -pkg-config-path ./anyhow/main/moon.pkg.json -pkg-sources username/hello/lib:./anyhow/lib -pkg-sources just/hello19:./deps/hello19/source -pkg-sources just/hello19/lib:./deps/hello19/source/lib -pkg-sources username/hello/main:./anyhow/main -pkg-sources username/hello/main_blackbox_test:./anyhow/main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__internal_test_info.json ./anyhow/lib/hello.mbt --target wasm-gc --pkg-name username/hello/lib --driver-kind internal
            moonc build-package ./anyhow/lib/hello.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./anyhow/lib -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./anyhow/lib/moon.pkg.json -pkg-sources username/hello/lib:./anyhow/lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__blackbox_test_info.json ./anyhow/lib/hello_test.mbt --doctest-only ./anyhow/lib/hello.mbt --target wasm-gc --pkg-name username/hello/lib --driver-kind blackbox
            moonc build-package ./anyhow/lib/hello_test.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./anyhow/lib/hello.mbt -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -pkg username/hello/lib_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources username/hello/lib_blackbox_test:./anyhow/lib -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.core ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -main username/hello/lib_blackbox_test -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./anyhow/lib/moon.pkg.json -pkg-sources username/hello/lib:./anyhow/lib -pkg-sources username/hello/lib_blackbox_test:./anyhow/lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
        "#]],
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
            Finished. moon: ran 10 tasks, now up to date
        "#]],
    );
    check(
        get_stderr(&dir, ["build"]),
        expect![[r#"
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
fn test_snapshot_test() {
    let dir = TestDir::new("snapshot_testing.in");
    check(
        get_err_stdout(&dir, ["test", "--sort-input", "--no-parallelize"]),
        expect![[r#"
            [username/hello] test lib/hello_test.mbt:7 ("snapshot in blackbox test") failed
            expect test failed at $ROOT/src/lib/hello_test.mbt:9:3
            Diff: (- expected, + actual)
            ----
            +Hello, world!
            ----

            [username/hello] test lib/hello.mbt:5 ("test inspect 1") failed
            expect test failed at $ROOT/src/lib/hello.mbt:6:3-6:15
            Diff: (- expected, + actual)
            ----
            +a
            ----

            [username/hello] test lib/hello.mbt:10 ("test snapshot 1") failed
            expect test failed at $ROOT/src/lib/hello.mbt:14:3
            Diff: (- expected, + actual)
            ----
            +hello
            +snapshot
            +testing
            ----

            [username/hello] test lib/hello.mbt:17 ("test inspect 2") failed
            expect test failed at $ROOT/src/lib/hello.mbt:18:3-18:15
            Diff: (- expected, + actual)
            ----
            +c
            ----

            [username/hello] test lib/hello.mbt:22 ("test snapshot 2") failed
            expect test failed at $ROOT/src/lib/hello.mbt:26:3
            Diff: (- expected, + actual)
            ----
            +should
            +be
            +work
            ----

            Total tests: 6, passed: 1, failed: 5.
        "#]],
    );
    check(
        get_err_stdout(
            &dir,
            [
                "test",
                "--sort-input",
                "--no-parallelize",
                "--target",
                "native",
            ],
        ),
        expect![[r#"
            [username/hello] test lib/hello_test.mbt:7 ("snapshot in blackbox test") failed
            expect test failed at $ROOT/src/lib/hello_test.mbt:9:3
            Diff: (- expected, + actual)
            ----
            +Hello, world!
            ----

            [username/hello] test lib/hello.mbt:5 ("test inspect 1") failed
            expect test failed at $ROOT/src/lib/hello.mbt:6:3-6:15
            Diff: (- expected, + actual)
            ----
            +a
            ----

            [username/hello] test lib/hello.mbt:10 ("test snapshot 1") failed
            expect test failed at $ROOT/src/lib/hello.mbt:14:3
            Diff: (- expected, + actual)
            ----
            +hello
            +snapshot
            +testing
            ----

            [username/hello] test lib/hello.mbt:17 ("test inspect 2") failed
            expect test failed at $ROOT/src/lib/hello.mbt:18:3-18:15
            Diff: (- expected, + actual)
            ----
            +c
            ----

            [username/hello] test lib/hello.mbt:22 ("test snapshot 2") failed
            expect test failed at $ROOT/src/lib/hello.mbt:26:3
            Diff: (- expected, + actual)
            ----
            +should
            +be
            +work
            ----

            Total tests: 6, passed: 1, failed: 5.
        "#]],
    );
    check(
        get_stdout(&dir, ["test", "-u", "--no-parallelize"]),
        expect![[r#"

            Auto updating expect tests and retesting ...

            Total tests: 6, passed: 6, failed: 0.
        "#]],
    );

    check(
        read(dir.join("src/lib/hello.mbt")),
        expect![[r#"
            pub fn hello() -> String {
              "Hello, world!"
            }

            test "test inspect 1" {
              inspect("a", content="a")
              inspect("b", content="b")
            }

            test "test snapshot 1" (it : @test.Test) {
              it.writeln("hello")
              it.writeln("snapshot")
              it.writeln("testing")
              it.snapshot!(filename="001.txt")
            }

            test "test inspect 2" {
              inspect("c", content="c")
              inspect("d", content="d")
            }

            test "test snapshot 2" (it : @test.Test) {
              it.writeln("should")
              it.writeln("be")
              it.writeln("work")
              it.snapshot!(filename="002.txt")
            }
        "#]],
    );
    check(
        read(dir.join("src/lib/__snapshot__/001.txt")),
        expect![[r#"
        hello
        snapshot
        testing
    "#]],
    );
    check(
        read(dir.join("src/lib/__snapshot__/002.txt")),
        expect![[r#"
        should
        be
        work
    "#]],
    );
    check(
        read(dir.join("src/lib/__snapshot__/003.txt")),
        expect!["Hello, world!"],
    );
}

#[test]
fn test_snapshot_test_target_js() {
    let dir = TestDir::new("snapshot_testing.in");
    check(
        get_err_stdout(
            &dir,
            ["test", "--target", "js", "--sort-input", "--no-parallelize"],
        ),
        expect![[r#"
            [username/hello] test lib/hello_test.mbt:7 ("snapshot in blackbox test") failed
            expect test failed at $ROOT/src/lib/hello_test.mbt:9:3
            Diff: (- expected, + actual)
            ----
            +Hello, world!
            ----

            [username/hello] test lib/hello.mbt:5 ("test inspect 1") failed
            expect test failed at $ROOT/src/lib/hello.mbt:6:3-6:15
            Diff: (- expected, + actual)
            ----
            +a
            ----

            [username/hello] test lib/hello.mbt:10 ("test snapshot 1") failed
            expect test failed at $ROOT/src/lib/hello.mbt:14:3
            Diff: (- expected, + actual)
            ----
            +hello
            +snapshot
            +testing
            ----

            [username/hello] test lib/hello.mbt:17 ("test inspect 2") failed
            expect test failed at $ROOT/src/lib/hello.mbt:18:3-18:15
            Diff: (- expected, + actual)
            ----
            +c
            ----

            [username/hello] test lib/hello.mbt:22 ("test snapshot 2") failed
            expect test failed at $ROOT/src/lib/hello.mbt:26:3
            Diff: (- expected, + actual)
            ----
            +should
            +be
            +work
            ----

            Total tests: 6, passed: 1, failed: 5.
        "#]],
    );
    assert!(dir.join("target/js/debug/test/package.json").exists());
    check(
        get_stdout(
            &dir,
            [
                "test",
                "--target",
                "js",
                "-u",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"

            Auto updating expect tests and retesting ...

            Total tests: 6, passed: 6, failed: 0.
        "#]],
    );

    check(
        read(dir.join("src/lib/hello.mbt")),
        expect![[r#"
            pub fn hello() -> String {
              "Hello, world!"
            }

            test "test inspect 1" {
              inspect("a", content="a")
              inspect("b", content="b")
            }

            test "test snapshot 1" (it : @test.Test) {
              it.writeln("hello")
              it.writeln("snapshot")
              it.writeln("testing")
              it.snapshot!(filename="001.txt")
            }

            test "test inspect 2" {
              inspect("c", content="c")
              inspect("d", content="d")
            }

            test "test snapshot 2" (it : @test.Test) {
              it.writeln("should")
              it.writeln("be")
              it.writeln("work")
              it.snapshot!(filename="002.txt")
            }
        "#]],
    );
    check(
        read(dir.join("src/lib/__snapshot__/001.txt")),
        expect![[r#"
        hello
        snapshot
        testing
    "#]],
    );
    check(
        read(dir.join("src/lib/__snapshot__/002.txt")),
        expect![[r#"
        should
        be
        work
    "#]],
    );
    check(
        read(dir.join("src/lib/__snapshot__/003.txt")),
        expect!["Hello, world!"],
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
            {"package":"username/hello/lib1","filename":"hello.mbt","index":"0","test_name":"test_1","message":"$ROOT/src/lib1/hello.mbt:7:3-7:24 FAILED: test_1 failed"}
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
            moonc check ./src/lib/hello.mbt -o ./target/wasm-gc/release/check/lib/lib.mi -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./src/lib -target wasm-gc -workspace-path .
            moonc check ./src/main/main.mbt -o ./target/wasm-gc/release/check/main/main.mi -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib/lib.mi:lib -pkg-sources username/hello/main:./src/main -target wasm-gc -workspace-path .
            moonc check -doctest-only ./src/main/main.mbt -include-doctests -o ./target/wasm-gc/release/check/main/main.blackbox_test.mi -pkg username/hello/main_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/main/main.mi:main -i ./target/wasm-gc/release/check/lib/lib.mi:lib -pkg-sources username/hello/main_blackbox_test:./src/main -target wasm-gc -blackbox-test -workspace-path .
            moonc check ./src/lib/hello_test.mbt -doctest-only ./src/lib/hello.mbt -include-doctests -o ./target/wasm-gc/release/check/lib/lib.blackbox_test.mi -pkg username/hello/lib_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib/lib.mi:lib -pkg-sources username/hello/lib_blackbox_test:./src/lib -target wasm-gc -blackbox-test -workspace-path .
            moondoc $ROOT -o $ROOT/target/doc -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -packages-json $ROOT/target/packages.json
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
    let dir = TestDir::new("hello");
    check(
        get_stderr(&dir, ["check"]),
        expect![[r#"
            Finished. moon: ran 2 tasks, now up to date
        "#]],
    );
    let moon_db_path = dir.join("./target/wasm-gc/release/check/check.moon_db");
    if moon_db_path.exists() {
        std::fs::remove_file(&moon_db_path).unwrap();
    }
    std::fs::write(&moon_db_path, "").unwrap();
    check(
        get_err_stderr(&dir, ["check"]),
        expect![[r#"
            error: internal error

            Caused by:
                0: failed to open n2 database
                1: failed to open $ROOT/target/wasm-gc/release/check/check.moon_db
                2: failed to read
                3: failed to fill whole buffer
        "#]],
    );
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
        "#]],
    );
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TraceResult(Vec<TraceEvent>);

#[derive(Debug, Serialize, Deserialize)]
pub struct TraceEvent {
    pid: u64,
    name: String,
    ts: u64,
    tid: u64,
    ph: String,
    dur: u64,
}

#[test]
fn test_trace_001() {
    let dir = TestDir::new("hello");
    let _ = get_stdout(&dir, ["build", "--trace"]);
    let s = replace_dir(&read(dir.join("trace.json")), &dir);
    let j: TraceResult = serde_json::from_str(&s).unwrap();
    let event_names = j.0.iter().map(|e| e.name.clone()).collect::<Vec<_>>();
    check(
        format!("{event_names:#?}"),
        expect![[r#"
            [
                "moonbit::build::read",
                "moonc build-package -error-format json $ROOT/main/main.mbt -o $ROOT/target/wasm-gc/release/build/main/main.core -pkg hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources hello/main:$ROOT/main -target wasm-gc -workspace-path $ROOT",
                "moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core $ROOT/target/wasm-gc/release/build/main/main.core -main hello/main -o $ROOT/target/wasm-gc/release/build/main/main.wasm -pkg-config-path $ROOT/main/moon.pkg.json -pkg-sources hello/main:$ROOT/main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc",
                "work.run",
                "main",
            ]"#]],
    );
    for e in j.0.iter() {
        assert!(e.dur > 0);
    }
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
            let _b : Bytes = Bytes::of([
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
            ])
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
    check(
        get_err_stderr(&dir, ["check"]),
        expect![[r#"
        error: failed to load `$ROOT/moon.mod.json`

        Caused by:
            0: `version` bad format
            1: unexpected end of input while parsing minor version number
    "#]],
    );
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

#[cfg(unix)]
#[test]
fn test_use_cc_for_native_release() {
    let dir = TestDir::new("moon_test/hello_exec_fntest");
    // build
    {
        check(
            get_stdout(
                &dir,
                [
                    "build",
                    "--target",
                    "native",
                    "--release",
                    "--sort-input",
                    "--dry-run",
                ],
            ),
            expect![[r#"
                cc -o ./target/native/release/build/runtime.o -I$MOON_HOME/include -g -c -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c
                moonc build-package ./lib/hello.mbt -o ./target/native/release/build/lib/lib.core -pkg moonbitlang/hello/lib -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moonbitlang/hello/lib:./lib -target native -workspace-path .
                moonc build-package ./main/main.mbt -o ./target/native/release/build/main/main.core -pkg moonbitlang/hello/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/release/build/lib/lib.mi:lib -pkg-sources moonbitlang/hello/main:./main -target native -workspace-path .
                moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/build/lib/lib.core ./target/native/release/build/main/main.core -main moonbitlang/hello/main -o ./target/native/release/build/main/main.c -pkg-config-path ./main/moon.pkg.json -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native
                cc -o ./target/native/release/build/main/main.exe -I$MOON_HOME/include -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/libmoonbitrun.o ./target/native/release/build/main/main.c ./target/native/release/build/runtime.o -lm
            "#]],
        );
        // if --release is not specified, it should not use cc
        check(
            get_stdout(
                &dir,
                ["build", "--target", "native", "--sort-input", "--dry-run"],
            ),
            expect![[r#"
                cc -o ./target/native/release/build/runtime.o -I$MOON_HOME/include -g -c -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c
                moonc build-package ./lib/hello.mbt -o ./target/native/release/build/lib/lib.core -pkg moonbitlang/hello/lib -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moonbitlang/hello/lib:./lib -target native -workspace-path .
                moonc build-package ./main/main.mbt -o ./target/native/release/build/main/main.core -pkg moonbitlang/hello/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/release/build/lib/lib.mi:lib -pkg-sources moonbitlang/hello/main:./main -target native -workspace-path .
                moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/build/lib/lib.core ./target/native/release/build/main/main.core -main moonbitlang/hello/main -o ./target/native/release/build/main/main.c -pkg-config-path ./main/moon.pkg.json -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native
                cc -o ./target/native/release/build/main/main.exe -I$MOON_HOME/include -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/libmoonbitrun.o ./target/native/release/build/main/main.c ./target/native/release/build/runtime.o -lm
            "#]],
        );
        check(
            get_stdout(
                &dir,
                [
                    "build",
                    "--target",
                    "native",
                    "--debug",
                    "--sort-input",
                    "--dry-run",
                ],
            ),
            expect![[r#"
                cc -o ./target/native/debug/build/runtime.o -I$MOON_HOME/include -g -c -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c
                moonc build-package ./lib/hello.mbt -o ./target/native/debug/build/lib/lib.core -pkg moonbitlang/hello/lib -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moonbitlang/hello/lib:./lib -target native -g -O0 -workspace-path .
                moonc build-package ./main/main.mbt -o ./target/native/debug/build/main/main.core -pkg moonbitlang/hello/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/build/lib/lib.mi:lib -pkg-sources moonbitlang/hello/main:./main -target native -g -O0 -workspace-path .
                moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/build/lib/lib.core ./target/native/debug/build/main/main.core -main moonbitlang/hello/main -o ./target/native/debug/build/main/main.c -pkg-config-path ./main/moon.pkg.json -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native -g -O0
                cc -o ./target/native/debug/build/main/main.exe -I$MOON_HOME/include -g -fwrapv -fno-strict-aliasing -Og $MOON_HOME/lib/libmoonbitrun.o ./target/native/debug/build/main/main.c ./target/native/debug/build/runtime.o -lm
            "#]],
        );
    }

    // run
    {
        check(
            get_stdout(
                &dir,
                [
                    "run",
                    "main",
                    "--target",
                    "native",
                    "--release",
                    "--sort-input",
                    "--dry-run",
                ],
            ),
            expect![[r#"
                cc -o ./target/native/release/build/runtime.o -I$MOON_HOME/include -g -c -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c
                moonc build-package ./lib/hello.mbt -o ./target/native/release/build/lib/lib.core -pkg moonbitlang/hello/lib -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moonbitlang/hello/lib:./lib -target native -workspace-path .
                moonc build-package ./main/main.mbt -o ./target/native/release/build/main/main.core -pkg moonbitlang/hello/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/release/build/lib/lib.mi:lib -pkg-sources moonbitlang/hello/main:./main -target native -workspace-path .
                moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/build/lib/lib.core ./target/native/release/build/main/main.core -main moonbitlang/hello/main -o ./target/native/release/build/main/main.c -pkg-config-path ./main/moon.pkg.json -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native
                cc -o ./target/native/release/build/main/main.exe -I$MOON_HOME/include -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/libmoonbitrun.o ./target/native/release/build/main/main.c ./target/native/release/build/runtime.o -lm
                ./target/native/release/build/main/main.exe
            "#]],
        );
        // if --release is not specified, it should not use cc
        check(
            get_stdout(
                &dir,
                [
                    "run",
                    "main",
                    "--target",
                    "native",
                    "--sort-input",
                    "--dry-run",
                ],
            ),
            expect![[r#"
                cc -o ./target/native/release/build/runtime.o -I$MOON_HOME/include -g -c -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c
                moonc build-package ./lib/hello.mbt -o ./target/native/release/build/lib/lib.core -pkg moonbitlang/hello/lib -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moonbitlang/hello/lib:./lib -target native -workspace-path .
                moonc build-package ./main/main.mbt -o ./target/native/release/build/main/main.core -pkg moonbitlang/hello/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/release/build/lib/lib.mi:lib -pkg-sources moonbitlang/hello/main:./main -target native -workspace-path .
                moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/build/lib/lib.core ./target/native/release/build/main/main.core -main moonbitlang/hello/main -o ./target/native/release/build/main/main.c -pkg-config-path ./main/moon.pkg.json -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native
                cc -o ./target/native/release/build/main/main.exe -I$MOON_HOME/include -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/libmoonbitrun.o ./target/native/release/build/main/main.c ./target/native/release/build/runtime.o -lm
                ./target/native/release/build/main/main.exe
            "#]],
        );
        check(
            get_stdout(
                &dir,
                [
                    "run",
                    "main",
                    "--target",
                    "native",
                    "--debug",
                    "--sort-input",
                    "--dry-run",
                ],
            ),
            expect![[r#"
                cc -o ./target/native/debug/build/runtime.o -I$MOON_HOME/include -g -c -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c
                moonc build-package ./lib/hello.mbt -o ./target/native/debug/build/lib/lib.core -pkg moonbitlang/hello/lib -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moonbitlang/hello/lib:./lib -target native -g -O0 -workspace-path .
                moonc build-package ./main/main.mbt -o ./target/native/debug/build/main/main.core -pkg moonbitlang/hello/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/build/lib/lib.mi:lib -pkg-sources moonbitlang/hello/main:./main -target native -g -O0 -workspace-path .
                moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/build/lib/lib.core ./target/native/debug/build/main/main.core -main moonbitlang/hello/main -o ./target/native/debug/build/main/main.c -pkg-config-path ./main/moon.pkg.json -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native -g -O0
                cc -o ./target/native/debug/build/main/main.exe -I$MOON_HOME/include -g -fwrapv -fno-strict-aliasing -Og $MOON_HOME/lib/libmoonbitrun.o ./target/native/debug/build/main/main.c ./target/native/debug/build/runtime.o -lm
                ./target/native/debug/build/main/main.exe
            "#]],
        );
    }

    // test
    {
        check(
            get_stdout(
                &dir,
                [
                    "test",
                    "--target",
                    "native",
                    "--release",
                    "--sort-input",
                    "--dry-run",
                ],
            ),
            expect![[r#"
                cc -o ./target/native/release/test/runtime.o -I$MOON_HOME/include -g -c -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c
                moonc build-package ./lib/hello.mbt -o ./target/native/release/test/lib/lib.core -pkg moonbitlang/hello/lib -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moonbitlang/hello/lib:./lib -target native -workspace-path .
                moon generate-test-driver --output-driver ./target/native/release/test/main/__generated_driver_for_internal_test.mbt --output-metadata ./target/native/release/test/main/__internal_test_info.json ./main/main.mbt --target native --pkg-name moonbitlang/hello/main --driver-kind internal
                moonc build-package ./main/main.mbt ./target/native/release/test/main/__generated_driver_for_internal_test.mbt -o ./target/native/release/test/main/main.internal_test.core -pkg moonbitlang/hello/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/release/test/lib/lib.mi:lib -pkg-sources moonbitlang/hello/main:./main -target native -no-mi -test-mode -workspace-path .
                moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/test/lib/lib.core ./target/native/release/test/main/main.internal_test.core -main moonbitlang/hello/main -o ./target/native/release/test/main/main.internal_test.c -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native
                cc -o ./target/native/release/test/main/main.internal_test.exe -I$MOON_HOME/include -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/libmoonbitrun.o ./target/native/release/test/main/main.internal_test.c ./target/native/release/test/runtime.o -lm
                moonc build-package ./main/main.mbt -o ./target/native/release/test/main/main.core -pkg moonbitlang/hello/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/release/test/lib/lib.mi:lib -pkg-sources moonbitlang/hello/main:./main -target native -workspace-path .
                moon generate-test-driver --output-driver ./target/native/release/test/main/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/native/release/test/main/__blackbox_test_info.json --doctest-only ./main/main.mbt --target native --pkg-name moonbitlang/hello/main --driver-kind blackbox
                moonc build-package ./target/native/release/test/main/__generated_driver_for_blackbox_test.mbt -doctest-only ./main/main.mbt -o ./target/native/release/test/main/main.blackbox_test.core -pkg moonbitlang/hello/main_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/release/test/main/main.mi:main -i ./target/native/release/test/lib/lib.mi:lib -pkg-sources moonbitlang/hello/main_blackbox_test:./main -target native -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
                moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/test/lib/lib.core ./target/native/release/test/main/main.core ./target/native/release/test/main/main.blackbox_test.core -main moonbitlang/hello/main_blackbox_test -o ./target/native/release/test/main/main.blackbox_test.c -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/hello/main:./main -pkg-sources moonbitlang/hello/main_blackbox_test:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native
                cc -o ./target/native/release/test/main/main.blackbox_test.exe -I$MOON_HOME/include -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/libmoonbitrun.o ./target/native/release/test/main/main.blackbox_test.c ./target/native/release/test/runtime.o -lm
                moon generate-test-driver --output-driver ./target/native/release/test/lib/__generated_driver_for_whitebox_test.mbt --output-metadata ./target/native/release/test/lib/__whitebox_test_info.json ./lib/hello_wbtest.mbt --target native --pkg-name moonbitlang/hello/lib --driver-kind whitebox
                moonc build-package ./lib/hello.mbt ./lib/hello_wbtest.mbt ./target/native/release/test/lib/__generated_driver_for_whitebox_test.mbt -o ./target/native/release/test/lib/lib.whitebox_test.core -pkg moonbitlang/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moonbitlang/hello/lib:./lib -target native -whitebox-test -no-mi -test-mode -workspace-path .
                moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/test/lib/lib.whitebox_test.core -main moonbitlang/hello/lib -o ./target/native/release/test/lib/lib.whitebox_test.c -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native
                cc -o ./target/native/release/test/lib/lib.whitebox_test.exe -I$MOON_HOME/include -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/libmoonbitrun.o ./target/native/release/test/lib/lib.whitebox_test.c ./target/native/release/test/runtime.o -lm
                moon generate-test-driver --output-driver ./target/native/release/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./target/native/release/test/lib/__internal_test_info.json ./lib/hello.mbt --target native --pkg-name moonbitlang/hello/lib --driver-kind internal
                moonc build-package ./lib/hello.mbt ./target/native/release/test/lib/__generated_driver_for_internal_test.mbt -o ./target/native/release/test/lib/lib.internal_test.core -pkg moonbitlang/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moonbitlang/hello/lib:./lib -target native -no-mi -test-mode -workspace-path .
                moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/test/lib/lib.internal_test.core -main moonbitlang/hello/lib -o ./target/native/release/test/lib/lib.internal_test.c -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native
                cc -o ./target/native/release/test/lib/lib.internal_test.exe -I$MOON_HOME/include -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/libmoonbitrun.o ./target/native/release/test/lib/lib.internal_test.c ./target/native/release/test/runtime.o -lm
                moon generate-test-driver --output-driver ./target/native/release/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/native/release/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target native --pkg-name moonbitlang/hello/lib --driver-kind blackbox
                moonc build-package ./target/native/release/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./target/native/release/test/lib/lib.blackbox_test.core -pkg moonbitlang/hello/lib_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/release/test/lib/lib.mi:lib -pkg-sources moonbitlang/hello/lib_blackbox_test:./lib -target native -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
                moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/test/lib/lib.core ./target/native/release/test/lib/lib.blackbox_test.core -main moonbitlang/hello/lib_blackbox_test -o ./target/native/release/test/lib/lib.blackbox_test.c -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/hello/lib_blackbox_test:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native
                cc -o ./target/native/release/test/lib/lib.blackbox_test.exe -I$MOON_HOME/include -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/libmoonbitrun.o ./target/native/release/test/lib/lib.blackbox_test.c ./target/native/release/test/runtime.o -lm
            "#]],
        );

        // use tcc for debug test
        #[cfg(target_os = "macos")]
        check(
            get_stdout(
                &dir,
                [
                    "test",
                    "--target",
                    "native",
                    "--debug",
                    "--sort-input",
                    "--dry-run",
                ],
            ),
            expect![[r#"
                moonc build-package ./lib/hello.mbt -o ./target/native/debug/test/lib/lib.core -pkg moonbitlang/hello/lib -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moonbitlang/hello/lib:./lib -target native -g -O0 -workspace-path .
                moon generate-test-driver --output-driver ./target/native/debug/test/main/__generated_driver_for_internal_test.mbt --output-metadata ./target/native/debug/test/main/__internal_test_info.json ./main/main.mbt --target native --pkg-name moonbitlang/hello/main --driver-kind internal
                moonc build-package ./main/main.mbt ./target/native/debug/test/main/__generated_driver_for_internal_test.mbt -o ./target/native/debug/test/main/main.internal_test.core -pkg moonbitlang/hello/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/test/lib/lib.mi:lib -pkg-sources moonbitlang/hello/main:./main -target native -g -O0 -no-mi -test-mode -workspace-path .
                moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.core ./target/native/debug/test/main/main.internal_test.core -main moonbitlang/hello/main -o ./target/native/debug/test/main/main.internal_test.c -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
                moonc build-package ./main/main.mbt -o ./target/native/debug/test/main/main.core -pkg moonbitlang/hello/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/test/lib/lib.mi:lib -pkg-sources moonbitlang/hello/main:./main -target native -g -O0 -workspace-path .
                moon generate-test-driver --output-driver ./target/native/debug/test/main/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/native/debug/test/main/__blackbox_test_info.json --doctest-only ./main/main.mbt --target native --pkg-name moonbitlang/hello/main --driver-kind blackbox
                moonc build-package ./target/native/debug/test/main/__generated_driver_for_blackbox_test.mbt -doctest-only ./main/main.mbt -o ./target/native/debug/test/main/main.blackbox_test.core -pkg moonbitlang/hello/main_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/test/main/main.mi:main -i ./target/native/debug/test/lib/lib.mi:lib -pkg-sources moonbitlang/hello/main_blackbox_test:./main -target native -g -O0 -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
                moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.core ./target/native/debug/test/main/main.core ./target/native/debug/test/main/main.blackbox_test.core -main moonbitlang/hello/main_blackbox_test -o ./target/native/debug/test/main/main.blackbox_test.c -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/hello/main:./main -pkg-sources moonbitlang/hello/main_blackbox_test:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
                cc -o ./target/native/debug/test/libruntime.dylib -I$MOON_HOME/include -g -shared -fPIC -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c -lm
                moon generate-test-driver --output-driver ./target/native/debug/test/lib/__generated_driver_for_whitebox_test.mbt --output-metadata ./target/native/debug/test/lib/__whitebox_test_info.json ./lib/hello_wbtest.mbt --target native --pkg-name moonbitlang/hello/lib --driver-kind whitebox
                moonc build-package ./lib/hello.mbt ./lib/hello_wbtest.mbt ./target/native/debug/test/lib/__generated_driver_for_whitebox_test.mbt -o ./target/native/debug/test/lib/lib.whitebox_test.core -pkg moonbitlang/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moonbitlang/hello/lib:./lib -target native -g -O0 -whitebox-test -no-mi -test-mode -workspace-path .
                moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.whitebox_test.core -main moonbitlang/hello/lib -o ./target/native/debug/test/lib/lib.whitebox_test.c -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
                moon generate-test-driver --output-driver ./target/native/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./target/native/debug/test/lib/__internal_test_info.json ./lib/hello.mbt --target native --pkg-name moonbitlang/hello/lib --driver-kind internal
                moonc build-package ./lib/hello.mbt ./target/native/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/native/debug/test/lib/lib.internal_test.core -pkg moonbitlang/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moonbitlang/hello/lib:./lib -target native -g -O0 -no-mi -test-mode -workspace-path .
                moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.internal_test.core -main moonbitlang/hello/lib -o ./target/native/debug/test/lib/lib.internal_test.c -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
                moon generate-test-driver --output-driver ./target/native/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/native/debug/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target native --pkg-name moonbitlang/hello/lib --driver-kind blackbox
                moonc build-package ./target/native/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./target/native/debug/test/lib/lib.blackbox_test.core -pkg moonbitlang/hello/lib_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/test/lib/lib.mi:lib -pkg-sources moonbitlang/hello/lib_blackbox_test:./lib -target native -g -O0 -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
                moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.core ./target/native/debug/test/lib/lib.blackbox_test.core -main moonbitlang/hello/lib_blackbox_test -o ./target/native/debug/test/lib/lib.blackbox_test.c -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/hello/lib_blackbox_test:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
            "#]],
        );
        #[cfg(target_os = "linux")]
        check(
            get_stdout(
                &dir,
                [
                    "test",
                    "--target",
                    "native",
                    "--debug",
                    "--sort-input",
                    "--dry-run",
                ],
            ),
            expect![[r#"
                moonc build-package ./lib/hello.mbt -o ./target/native/debug/test/lib/lib.core -pkg moonbitlang/hello/lib -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moonbitlang/hello/lib:./lib -target native -g -O0 -workspace-path .
                moon generate-test-driver --output-driver ./target/native/debug/test/main/__generated_driver_for_internal_test.mbt --output-metadata ./target/native/debug/test/main/__internal_test_info.json ./main/main.mbt --target native --pkg-name moonbitlang/hello/main --driver-kind internal
                moonc build-package ./main/main.mbt ./target/native/debug/test/main/__generated_driver_for_internal_test.mbt -o ./target/native/debug/test/main/main.internal_test.core -pkg moonbitlang/hello/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/test/lib/lib.mi:lib -pkg-sources moonbitlang/hello/main:./main -target native -g -O0 -no-mi -test-mode -workspace-path .
                moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.core ./target/native/debug/test/main/main.internal_test.core -main moonbitlang/hello/main -o ./target/native/debug/test/main/main.internal_test.c -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
                moonc build-package ./main/main.mbt -o ./target/native/debug/test/main/main.core -pkg moonbitlang/hello/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/test/lib/lib.mi:lib -pkg-sources moonbitlang/hello/main:./main -target native -g -O0 -workspace-path .
                moon generate-test-driver --output-driver ./target/native/debug/test/main/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/native/debug/test/main/__blackbox_test_info.json --doctest-only ./main/main.mbt --target native --pkg-name moonbitlang/hello/main --driver-kind blackbox
                moonc build-package ./target/native/debug/test/main/__generated_driver_for_blackbox_test.mbt -doctest-only ./main/main.mbt -o ./target/native/debug/test/main/main.blackbox_test.core -pkg moonbitlang/hello/main_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/test/main/main.mi:main -i ./target/native/debug/test/lib/lib.mi:lib -pkg-sources moonbitlang/hello/main_blackbox_test:./main -target native -g -O0 -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
                moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.core ./target/native/debug/test/main/main.core ./target/native/debug/test/main/main.blackbox_test.core -main moonbitlang/hello/main_blackbox_test -o ./target/native/debug/test/main/main.blackbox_test.c -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/hello/main:./main -pkg-sources moonbitlang/hello/main_blackbox_test:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
                cc -o ./target/native/debug/test/libruntime.so -I$MOON_HOME/include -g -shared -fPIC -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c -lm
                moon generate-test-driver --output-driver ./target/native/debug/test/lib/__generated_driver_for_whitebox_test.mbt --output-metadata ./target/native/debug/test/lib/__whitebox_test_info.json ./lib/hello_wbtest.mbt --target native --pkg-name moonbitlang/hello/lib --driver-kind whitebox
                moonc build-package ./lib/hello.mbt ./lib/hello_wbtest.mbt ./target/native/debug/test/lib/__generated_driver_for_whitebox_test.mbt -o ./target/native/debug/test/lib/lib.whitebox_test.core -pkg moonbitlang/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moonbitlang/hello/lib:./lib -target native -g -O0 -whitebox-test -no-mi -test-mode -workspace-path .
                moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.whitebox_test.core -main moonbitlang/hello/lib -o ./target/native/debug/test/lib/lib.whitebox_test.c -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
                moon generate-test-driver --output-driver ./target/native/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./target/native/debug/test/lib/__internal_test_info.json ./lib/hello.mbt --target native --pkg-name moonbitlang/hello/lib --driver-kind internal
                moonc build-package ./lib/hello.mbt ./target/native/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/native/debug/test/lib/lib.internal_test.core -pkg moonbitlang/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moonbitlang/hello/lib:./lib -target native -g -O0 -no-mi -test-mode -workspace-path .
                moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.internal_test.core -main moonbitlang/hello/lib -o ./target/native/debug/test/lib/lib.internal_test.c -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
                moon generate-test-driver --output-driver ./target/native/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/native/debug/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target native --pkg-name moonbitlang/hello/lib --driver-kind blackbox
                moonc build-package ./target/native/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./target/native/debug/test/lib/lib.blackbox_test.core -pkg moonbitlang/hello/lib_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/test/lib/lib.mi:lib -pkg-sources moonbitlang/hello/lib_blackbox_test:./lib -target native -g -O0 -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
                moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.core ./target/native/debug/test/lib/lib.blackbox_test.core -main moonbitlang/hello/lib_blackbox_test -o ./target/native/debug/test/lib/lib.blackbox_test.c -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/hello/lib_blackbox_test:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
            "#]],
        );
    }
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
            Package to $ROOT/target/publish/username-hello-0.1.0.zip
        "#]],
    );
}

#[test]
#[cfg(unix)]
#[ignore = "platform-dependent behavior"]
fn test_native_backend_cc_flags() {
    let dir = TestDir::new("native_backend_cc_flags.in");
    check(
        get_stdout(
            &dir,
            ["build", "--target", "native", "--dry-run", "--sort-input"],
        ),
        expect![[r#"
            stubcc -o ./target/native/release/build/lib/stub.o -I$MOON_HOME/include -L$MOON_HOME/lib -c -fwrapv -fno-strict-aliasing ./lib/stub.c stubccflags
            moonc build-package ./lib/hello.mbt -o ./target/native/release/build/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moon_new/lib:./lib -target native
            moonc build-package ./main/main.mbt -o ./target/native/release/build/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/release/build/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target native
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/build/lib/lib.core ./target/native/release/build/main/main.core -main moon_new/main -o ./target/native/release/build/main/main.c -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native
            cc -o ./target/native/release/build/runtime.o -I$MOON_HOME/include -L$MOON_HOME/lib -g -c -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c
            stubar -r -c -s ./target/native/release/build/lib/liblib.a ./target/native/release/build/lib/stub.o
            cc -o ./target/native/release/build/main/main.exe -I$MOON_HOME/include -L$MOON_HOME/lib -fwrapv -fno-strict-aliasing $MOON_HOME/lib/libmoonbitrun.o ./target/native/release/build/main/main.c ./target/native/release/build/runtime.o ./target/native/release/build/lib/liblib.a -lm ccflags fasd cclinkflags
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/build/lib/lib.core -main moon_new/lib -o ./target/native/release/build/lib/lib.c -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native
            cc -o ./target/native/release/build/lib/lib.exe -I$MOON_HOME/include -L$MOON_HOME/lib -fwrapv -fno-strict-aliasing $MOON_HOME/lib/libmoonbitrun.o ./target/native/release/build/lib/lib.c ./target/native/release/build/runtime.o ./target/native/release/build/lib/liblib.a -lm ccflags fasd cclinkflags
        "#]],
    );
    // don't pass native cc flags for no native backend
    check(
        get_stdout(
            &dir,
            ["build", "--target", "wasm-gc", "--dry-run", "--sort-input"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target wasm-gc
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main moon_new/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/lib/lib.core -main moon_new/lib -o ./target/wasm-gc/release/build/lib/lib.wasm -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc
        "#]],
    );

    check(
        get_stdout(
            &dir,
            ["test", "--target", "native", "--dry-run", "--sort-input"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/native/debug/test/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moon_new/lib:./lib -target native -g -O0
            moon generate-test-driver --output-driver ./target/native/debug/test/main/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/native/debug/test/main/__blackbox_test_info.json --doctest-only ./main/main.mbt --target native --pkg-name moon_new/main --driver-kind blackbox
            moonc build-package ./main/main.mbt -o ./target/native/debug/test/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/test/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target native -g -O0
            moonc build-package ./target/native/debug/test/main/__generated_driver_for_blackbox_test.mbt -doctest-only ./main/main.mbt -o ./target/native/debug/test/main/main.blackbox_test.core -pkg moon_new/main_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/test/main/main.mi:main -i ./target/native/debug/test/lib/lib.mi:lib -pkg-sources moon_new/main_blackbox_test:./main -target native -g -O0 -blackbox-test -include-doctests -no-mi -test-mode
            stubcc -o ./target/native/debug/test/lib/stub.o -I$MOON_HOME/include -L$MOON_HOME/lib -g -c -fwrapv -fno-strict-aliasing ./lib/stub.c stubccflags
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.core ./target/native/debug/test/main/main.core ./target/native/debug/test/main/main.blackbox_test.core -main moon_new/main_blackbox_test -o ./target/native/debug/test/main/main.blackbox_test.c -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moon_new/main_blackbox_test:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
            cc -o ./target/native/debug/test/runtime.o -I$MOON_HOME/include -L$MOON_HOME/lib -g -c -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c
            stubar -r -c -s ./target/native/debug/test/lib/liblib.a ./target/native/debug/test/lib/stub.o
            cc -o ./target/native/debug/test/main/main.blackbox_test.exe -I$MOON_HOME/include -L$MOON_HOME/lib -g -fwrapv -fno-strict-aliasing $MOON_HOME/lib/libmoonbitrun.o ./target/native/debug/test/main/main.blackbox_test.c ./target/native/debug/test/runtime.o ./target/native/debug/test/lib/liblib.a -lm ccflags fasd cclinkflags
            moon generate-test-driver --output-driver ./target/native/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/native/debug/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target native --pkg-name moon_new/lib --driver-kind blackbox
            moonc build-package ./target/native/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./target/native/debug/test/lib/lib.blackbox_test.core -pkg moon_new/lib_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/test/lib/lib.mi:lib -pkg-sources moon_new/lib_blackbox_test:./lib -target native -g -O0 -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.core ./target/native/debug/test/lib/lib.blackbox_test.core -main moon_new/lib_blackbox_test -o ./target/native/debug/test/lib/lib.blackbox_test.c -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/lib_blackbox_test:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
            cc -o ./target/native/debug/test/lib/lib.blackbox_test.exe -I$MOON_HOME/include -L$MOON_HOME/lib -g -fwrapv -fno-strict-aliasing $MOON_HOME/lib/libmoonbitrun.o ./target/native/debug/test/lib/lib.blackbox_test.c ./target/native/debug/test/runtime.o ./target/native/debug/test/lib/liblib.a -lm ccflags fasd cclinkflags
            moon generate-test-driver --output-driver ./target/native/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./target/native/debug/test/lib/__internal_test_info.json ./lib/hello.mbt --target native --pkg-name moon_new/lib --driver-kind internal
            moonc build-package ./lib/hello.mbt ./target/native/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/native/debug/test/lib/lib.internal_test.core -pkg moon_new/lib -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moon_new/lib:./lib -target native -g -O0 -no-mi -test-mode
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.internal_test.core -main moon_new/lib -o ./target/native/debug/test/lib/lib.internal_test.c -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
            cc -o ./target/native/debug/test/lib/lib.internal_test.exe -I$MOON_HOME/include -L$MOON_HOME/lib -g -fwrapv -fno-strict-aliasing $MOON_HOME/lib/libmoonbitrun.o ./target/native/debug/test/lib/lib.internal_test.c ./target/native/debug/test/runtime.o ./target/native/debug/test/lib/liblib.a -lm ccflags fasd cclinkflags
        "#]],
    );
    // don't pass native cc flags for no native backend
    check(
        get_stdout(&dir, ["test", "--target", "wasm", "--dry-run"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm/debug/test/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/wasm/release/bundle -pkg-sources moon_new/lib:./lib -target wasm -g -O0
            moon generate-test-driver --output-driver ./target/wasm/debug/test/main/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm/debug/test/main/__blackbox_test_info.json --doctest-only ./main/main.mbt --target wasm --pkg-name moon_new/main --driver-kind blackbox
            moonc build-package ./main/main.mbt -o ./target/wasm/debug/test/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/wasm/release/bundle -i ./target/wasm/debug/test/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target wasm -g -O0
            moonc build-package ./target/wasm/debug/test/main/__generated_driver_for_blackbox_test.mbt -doctest-only ./main/main.mbt -o ./target/wasm/debug/test/main/main.blackbox_test.core -pkg moon_new/main_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm/release/bundle -i ./target/wasm/debug/test/main/main.mi:main -i ./target/wasm/debug/test/lib/lib.mi:lib -pkg-sources moon_new/main_blackbox_test:./main -target wasm -g -O0 -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core $MOON_HOME/lib/core/target/wasm/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm/release/bundle/core.core ./target/wasm/debug/test/lib/lib.core ./target/wasm/debug/test/main/main.core ./target/wasm/debug/test/main/main.blackbox_test.core -main moon_new/main_blackbox_test -o ./target/wasm/debug/test/main/main.blackbox_test.wasm -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moon_new/main_blackbox_test:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm -g -O0
            moon generate-test-driver --output-driver ./target/wasm/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm/debug/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target wasm --pkg-name moon_new/lib --driver-kind blackbox
            moonc build-package ./target/wasm/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./target/wasm/debug/test/lib/lib.blackbox_test.core -pkg moon_new/lib_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm/release/bundle -i ./target/wasm/debug/test/lib/lib.mi:lib -pkg-sources moon_new/lib_blackbox_test:./lib -target wasm -g -O0 -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core $MOON_HOME/lib/core/target/wasm/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm/release/bundle/core.core ./target/wasm/debug/test/lib/lib.core ./target/wasm/debug/test/lib/lib.blackbox_test.core -main moon_new/lib_blackbox_test -o ./target/wasm/debug/test/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/lib_blackbox_test:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm -g -O0
            moon generate-test-driver --output-driver ./target/wasm/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm/debug/test/lib/__internal_test_info.json ./lib/hello.mbt --target wasm --pkg-name moon_new/lib --driver-kind internal
            moonc build-package ./lib/hello.mbt ./target/wasm/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm/debug/test/lib/lib.internal_test.core -pkg moon_new/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm/release/bundle -pkg-sources moon_new/lib:./lib -target wasm -g -O0 -no-mi -test-mode
            moonc link-core $MOON_HOME/lib/core/target/wasm/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm/release/bundle/core.core ./target/wasm/debug/test/lib/lib.internal_test.core -main moon_new/lib -o ./target/wasm/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm -g -O0
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "run",
                "main",
                "--target",
                "native",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            stubcc -o ./target/native/release/build/lib/stub.o -I$MOON_HOME/include -L$MOON_HOME/lib -c -fwrapv -fno-strict-aliasing ./lib/stub.c stubccflags
            moonc build-package ./lib/hello.mbt -o ./target/native/release/build/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moon_new/lib:./lib -target native
            moonc build-package ./main/main.mbt -o ./target/native/release/build/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/release/build/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target native
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/build/lib/lib.core ./target/native/release/build/main/main.core -main moon_new/main -o ./target/native/release/build/main/main.c -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native
            cc -o ./target/native/release/build/runtime.o -I$MOON_HOME/include -L$MOON_HOME/lib -g -c -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c
            stubar -r -c -s ./target/native/release/build/lib/liblib.a ./target/native/release/build/lib/stub.o
            cc -o ./target/native/release/build/main/main.exe -I$MOON_HOME/include -L$MOON_HOME/lib -fwrapv -fno-strict-aliasing $MOON_HOME/lib/libmoonbitrun.o ./target/native/release/build/main/main.c ./target/native/release/build/runtime.o ./target/native/release/build/lib/liblib.a -lm ccflags fasd cclinkflags
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/build/lib/lib.core -main moon_new/lib -o ./target/native/release/build/lib/lib.c -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native
            cc -o ./target/native/release/build/lib/lib.exe -I$MOON_HOME/include -L$MOON_HOME/lib -fwrapv -fno-strict-aliasing $MOON_HOME/lib/libmoonbitrun.o ./target/native/release/build/lib/lib.c ./target/native/release/build/runtime.o ./target/native/release/build/lib/liblib.a -lm ccflags fasd cclinkflags
            ./target/native/release/build/lib/lib.exe
            ./target/native/release/build/main/main.exe
        "#]],
    );
    // don't pass native cc flags for no native backend
    check(
        get_stdout(
            &dir,
            [
                "run",
                "main",
                "--target",
                "wasm",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm/release/build/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/wasm/release/bundle -pkg-sources moon_new/lib:./lib -target wasm
            moonc build-package ./main/main.mbt -o ./target/wasm/release/build/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/wasm/release/bundle -i ./target/wasm/release/build/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target wasm
            moonc link-core $MOON_HOME/lib/core/target/wasm/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm/release/bundle/core.core ./target/wasm/release/build/lib/lib.core ./target/wasm/release/build/main/main.core -main moon_new/main -o ./target/wasm/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm
            moonc link-core $MOON_HOME/lib/core/target/wasm/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm/release/bundle/core.core ./target/wasm/release/build/lib/lib.core -main moon_new/lib -o ./target/wasm/release/build/lib/lib.wasm -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm
            moonrun ./target/wasm/release/build/lib/lib.wasm
            moonrun ./target/wasm/release/build/main/main.wasm
        "#]],
    );
}

#[test]
#[cfg(unix)]
fn test_native_backend_cc_flags_with_env_override() {
    let dir = TestDir::new("native_backend_cc_flags.in");
    check(
        get_stdout_with_envs(
            &dir,
            ["build", "--target", "native", "--dry-run", "--sort-input"],
            [("MOON_CC", "x86_64-unknown-fake_os-fake_libc-gcc")],
        ),
        expect![[r#"
            x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/release/build/lib/stub.o -I$MOON_HOME/include -c -fwrapv -fno-strict-aliasing ./lib/stub.c stubccflags
            x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/release/build/runtime.o -I$MOON_HOME/include -g -c -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c
            moonc build-package ./lib/hello.mbt -o ./target/native/release/build/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moon_new/lib:./lib -target native -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/native/release/build/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/release/build/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target native -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/build/lib/lib.core ./target/native/release/build/main/main.core -main moon_new/main -o ./target/native/release/build/main/main.c -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native
            x86_64-unknown-fake_os-fake_libc-ar -r -c -s ./target/native/release/build/lib/liblib.a ./target/native/release/build/lib/stub.o
            x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/release/build/main/main.exe -I$MOON_HOME/include -fwrapv -fno-strict-aliasing ./target/native/release/build/main/main.c ./target/native/release/build/runtime.o ./target/native/release/build/lib/liblib.a -lm ccflags fasd cclinkflags
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/build/lib/lib.core -main moon_new/lib -o ./target/native/release/build/lib/lib.c -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native
            x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/release/build/lib/lib.exe -I$MOON_HOME/include -fwrapv -fno-strict-aliasing ./target/native/release/build/lib/lib.c ./target/native/release/build/runtime.o ./target/native/release/build/lib/liblib.a -lm ccflags fasd cclinkflags
        "#]],
    );

    check(
        get_stdout_with_envs(
            &dir,
            ["test", "--target", "native", "--dry-run", "--sort-input"],
            [("MOON_CC", "x86_64-unknown-fake_os-fake_libc-gcc")],
        ),
        expect![[r#"
            x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/debug/test/runtime.o -I$MOON_HOME/include -g -c -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c
            moonc build-package ./lib/hello.mbt -o ./target/native/debug/test/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moon_new/lib:./lib -target native -g -O0 -workspace-path .
            moon generate-test-driver --output-driver ./target/native/debug/test/main/__generated_driver_for_internal_test.mbt --output-metadata ./target/native/debug/test/main/__internal_test_info.json ./main/main.mbt --target native --pkg-name moon_new/main --driver-kind internal
            moonc build-package ./main/main.mbt ./target/native/debug/test/main/__generated_driver_for_internal_test.mbt -o ./target/native/debug/test/main/main.internal_test.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/test/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target native -g -O0 -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.core ./target/native/debug/test/main/main.internal_test.core -main moon_new/main -o ./target/native/debug/test/main/main.internal_test.c -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
            x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/debug/test/lib/stub.o -I$MOON_HOME/include -g -c -fwrapv -fno-strict-aliasing ./lib/stub.c stubccflags
            x86_64-unknown-fake_os-fake_libc-ar -r -c -s ./target/native/debug/test/lib/liblib.a ./target/native/debug/test/lib/stub.o
            x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/debug/test/main/main.internal_test.exe -I$MOON_HOME/include -g -fwrapv -fno-strict-aliasing ./target/native/debug/test/main/main.internal_test.c ./target/native/debug/test/runtime.o ./target/native/debug/test/lib/liblib.a -lm ccflags fasd cclinkflags
            moonc build-package ./main/main.mbt -o ./target/native/debug/test/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/test/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target native -g -O0 -workspace-path .
            moon generate-test-driver --output-driver ./target/native/debug/test/main/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/native/debug/test/main/__blackbox_test_info.json --doctest-only ./main/main.mbt --target native --pkg-name moon_new/main --driver-kind blackbox
            moonc build-package ./target/native/debug/test/main/__generated_driver_for_blackbox_test.mbt -doctest-only ./main/main.mbt -o ./target/native/debug/test/main/main.blackbox_test.core -pkg moon_new/main_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/test/main/main.mi:main -i ./target/native/debug/test/lib/lib.mi:lib -pkg-sources moon_new/main_blackbox_test:./main -target native -g -O0 -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.core ./target/native/debug/test/main/main.core ./target/native/debug/test/main/main.blackbox_test.core -main moon_new/main_blackbox_test -o ./target/native/debug/test/main/main.blackbox_test.c -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moon_new/main_blackbox_test:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
            x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/debug/test/main/main.blackbox_test.exe -I$MOON_HOME/include -g -fwrapv -fno-strict-aliasing ./target/native/debug/test/main/main.blackbox_test.c ./target/native/debug/test/runtime.o ./target/native/debug/test/lib/liblib.a -lm ccflags fasd cclinkflags
            moon generate-test-driver --output-driver ./target/native/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./target/native/debug/test/lib/__internal_test_info.json ./lib/hello.mbt --target native --pkg-name moon_new/lib --driver-kind internal
            moonc build-package ./lib/hello.mbt ./target/native/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/native/debug/test/lib/lib.internal_test.core -pkg moon_new/lib -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moon_new/lib:./lib -target native -g -O0 -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.internal_test.core -main moon_new/lib -o ./target/native/debug/test/lib/lib.internal_test.c -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
            x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/debug/test/lib/lib.internal_test.exe -I$MOON_HOME/include -g -fwrapv -fno-strict-aliasing ./target/native/debug/test/lib/lib.internal_test.c ./target/native/debug/test/runtime.o ./target/native/debug/test/lib/liblib.a -lm ccflags fasd cclinkflags
            moon generate-test-driver --output-driver ./target/native/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/native/debug/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target native --pkg-name moon_new/lib --driver-kind blackbox
            moonc build-package ./target/native/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./target/native/debug/test/lib/lib.blackbox_test.core -pkg moon_new/lib_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/test/lib/lib.mi:lib -pkg-sources moon_new/lib_blackbox_test:./lib -target native -g -O0 -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.core ./target/native/debug/test/lib/lib.blackbox_test.core -main moon_new/lib_blackbox_test -o ./target/native/debug/test/lib/lib.blackbox_test.c -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/lib_blackbox_test:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
            x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/debug/test/lib/lib.blackbox_test.exe -I$MOON_HOME/include -g -fwrapv -fno-strict-aliasing ./target/native/debug/test/lib/lib.blackbox_test.c ./target/native/debug/test/runtime.o ./target/native/debug/test/lib/liblib.a -lm ccflags fasd cclinkflags
        "#]],
    );

    check(
        get_stdout_with_envs(
            &dir,
            [
                "run",
                "main",
                "--target",
                "native",
                "--dry-run",
                "--sort-input",
            ],
            [("MOON_CC", "x86_64-unknown-fake_os-fake_libc-gcc")],
        ),
        expect![[r#"
            x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/release/build/lib/stub.o -I$MOON_HOME/include -c -fwrapv -fno-strict-aliasing ./lib/stub.c stubccflags
            x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/release/build/runtime.o -I$MOON_HOME/include -g -c -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c
            moonc build-package ./lib/hello.mbt -o ./target/native/release/build/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moon_new/lib:./lib -target native -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/native/release/build/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/release/build/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target native -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/build/lib/lib.core ./target/native/release/build/main/main.core -main moon_new/main -o ./target/native/release/build/main/main.c -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native
            x86_64-unknown-fake_os-fake_libc-ar -r -c -s ./target/native/release/build/lib/liblib.a ./target/native/release/build/lib/stub.o
            x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/release/build/main/main.exe -I$MOON_HOME/include -fwrapv -fno-strict-aliasing ./target/native/release/build/main/main.c ./target/native/release/build/runtime.o ./target/native/release/build/lib/liblib.a -lm ccflags fasd cclinkflags
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/build/lib/lib.core -main moon_new/lib -o ./target/native/release/build/lib/lib.c -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native
            x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/release/build/lib/lib.exe -I$MOON_HOME/include -fwrapv -fno-strict-aliasing ./target/native/release/build/lib/lib.c ./target/native/release/build/runtime.o ./target/native/release/build/lib/liblib.a -lm ccflags fasd cclinkflags
            ./target/native/release/build/lib/lib.exe
            ./target/native/release/build/main/main.exe
        "#]],
    );

    check(
        get_stdout_with_envs(
            &dir,
            ["build", "--target", "native", "--dry-run", "--sort-input"],
            [
                (
                    "MOON_CC",
                    "/some/path/A/x86_64-unknown-fake_os-fake_libc-gcc",
                ),
                (
                    "MOON_AR",
                    "/other/path/B/x86_64-unknown-fake_os-fake_libc-ar",
                ),
            ],
        ),
        expect![[r#"
            /some/path/A/x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/release/build/lib/stub.o -I$MOON_HOME/include -c -fwrapv -fno-strict-aliasing ./lib/stub.c stubccflags
            /some/path/A/x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/release/build/runtime.o -I$MOON_HOME/include -g -c -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c
            moonc build-package ./lib/hello.mbt -o ./target/native/release/build/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moon_new/lib:./lib -target native -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/native/release/build/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/release/build/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target native -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/build/lib/lib.core ./target/native/release/build/main/main.core -main moon_new/main -o ./target/native/release/build/main/main.c -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native
            /other/path/B/x86_64-unknown-fake_os-fake_libc-ar -r -c -s ./target/native/release/build/lib/liblib.a ./target/native/release/build/lib/stub.o
            /some/path/A/x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/release/build/main/main.exe -I$MOON_HOME/include -fwrapv -fno-strict-aliasing ./target/native/release/build/main/main.c ./target/native/release/build/runtime.o ./target/native/release/build/lib/liblib.a -lm ccflags fasd cclinkflags
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/build/lib/lib.core -main moon_new/lib -o ./target/native/release/build/lib/lib.c -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native
            /some/path/A/x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/release/build/lib/lib.exe -I$MOON_HOME/include -fwrapv -fno-strict-aliasing ./target/native/release/build/lib/lib.c ./target/native/release/build/runtime.o ./target/native/release/build/lib/liblib.a -lm ccflags fasd cclinkflags
        "#]],
    );

    check(
        get_stdout_with_envs(
            &dir,
            ["test", "--target", "native", "--dry-run", "--sort-input"],
            [
                (
                    "MOON_CC",
                    "/some/path/A/x86_64-unknown-fake_os-fake_libc-gcc",
                ),
                (
                    "MOON_AR",
                    "/other/path/B/x86_64-unknown-fake_os-fake_libc-ar",
                ),
            ],
        ),
        expect![[r#"
            /some/path/A/x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/debug/test/runtime.o -I$MOON_HOME/include -g -c -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c
            moonc build-package ./lib/hello.mbt -o ./target/native/debug/test/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moon_new/lib:./lib -target native -g -O0 -workspace-path .
            moon generate-test-driver --output-driver ./target/native/debug/test/main/__generated_driver_for_internal_test.mbt --output-metadata ./target/native/debug/test/main/__internal_test_info.json ./main/main.mbt --target native --pkg-name moon_new/main --driver-kind internal
            moonc build-package ./main/main.mbt ./target/native/debug/test/main/__generated_driver_for_internal_test.mbt -o ./target/native/debug/test/main/main.internal_test.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/test/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target native -g -O0 -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.core ./target/native/debug/test/main/main.internal_test.core -main moon_new/main -o ./target/native/debug/test/main/main.internal_test.c -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
            /some/path/A/x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/debug/test/lib/stub.o -I$MOON_HOME/include -g -c -fwrapv -fno-strict-aliasing ./lib/stub.c stubccflags
            /other/path/B/x86_64-unknown-fake_os-fake_libc-ar -r -c -s ./target/native/debug/test/lib/liblib.a ./target/native/debug/test/lib/stub.o
            /some/path/A/x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/debug/test/main/main.internal_test.exe -I$MOON_HOME/include -g -fwrapv -fno-strict-aliasing ./target/native/debug/test/main/main.internal_test.c ./target/native/debug/test/runtime.o ./target/native/debug/test/lib/liblib.a -lm ccflags fasd cclinkflags
            moonc build-package ./main/main.mbt -o ./target/native/debug/test/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/test/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target native -g -O0 -workspace-path .
            moon generate-test-driver --output-driver ./target/native/debug/test/main/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/native/debug/test/main/__blackbox_test_info.json --doctest-only ./main/main.mbt --target native --pkg-name moon_new/main --driver-kind blackbox
            moonc build-package ./target/native/debug/test/main/__generated_driver_for_blackbox_test.mbt -doctest-only ./main/main.mbt -o ./target/native/debug/test/main/main.blackbox_test.core -pkg moon_new/main_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/test/main/main.mi:main -i ./target/native/debug/test/lib/lib.mi:lib -pkg-sources moon_new/main_blackbox_test:./main -target native -g -O0 -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.core ./target/native/debug/test/main/main.core ./target/native/debug/test/main/main.blackbox_test.core -main moon_new/main_blackbox_test -o ./target/native/debug/test/main/main.blackbox_test.c -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moon_new/main_blackbox_test:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
            /some/path/A/x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/debug/test/main/main.blackbox_test.exe -I$MOON_HOME/include -g -fwrapv -fno-strict-aliasing ./target/native/debug/test/main/main.blackbox_test.c ./target/native/debug/test/runtime.o ./target/native/debug/test/lib/liblib.a -lm ccflags fasd cclinkflags
            moon generate-test-driver --output-driver ./target/native/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./target/native/debug/test/lib/__internal_test_info.json ./lib/hello.mbt --target native --pkg-name moon_new/lib --driver-kind internal
            moonc build-package ./lib/hello.mbt ./target/native/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/native/debug/test/lib/lib.internal_test.core -pkg moon_new/lib -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moon_new/lib:./lib -target native -g -O0 -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.internal_test.core -main moon_new/lib -o ./target/native/debug/test/lib/lib.internal_test.c -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
            /some/path/A/x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/debug/test/lib/lib.internal_test.exe -I$MOON_HOME/include -g -fwrapv -fno-strict-aliasing ./target/native/debug/test/lib/lib.internal_test.c ./target/native/debug/test/runtime.o ./target/native/debug/test/lib/liblib.a -lm ccflags fasd cclinkflags
            moon generate-test-driver --output-driver ./target/native/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/native/debug/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target native --pkg-name moon_new/lib --driver-kind blackbox
            moonc build-package ./target/native/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./target/native/debug/test/lib/lib.blackbox_test.core -pkg moon_new/lib_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/test/lib/lib.mi:lib -pkg-sources moon_new/lib_blackbox_test:./lib -target native -g -O0 -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.core ./target/native/debug/test/lib/lib.blackbox_test.core -main moon_new/lib_blackbox_test -o ./target/native/debug/test/lib/lib.blackbox_test.c -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/lib_blackbox_test:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
            /some/path/A/x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/debug/test/lib/lib.blackbox_test.exe -I$MOON_HOME/include -g -fwrapv -fno-strict-aliasing ./target/native/debug/test/lib/lib.blackbox_test.c ./target/native/debug/test/runtime.o ./target/native/debug/test/lib/liblib.a -lm ccflags fasd cclinkflags
        "#]],
    );

    check(
        get_stdout_with_envs(
            &dir,
            [
                "run",
                "main",
                "--target",
                "native",
                "--dry-run",
                "--sort-input",
            ],
            [
                (
                    "MOON_CC",
                    "/some/path/A/x86_64-unknown-fake_os-fake_libc-gcc",
                ),
                (
                    "MOON_AR",
                    "/other/path/B/x86_64-unknown-fake_os-fake_libc-ar",
                ),
            ],
        ),
        expect![[r#"
            /some/path/A/x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/release/build/lib/stub.o -I$MOON_HOME/include -c -fwrapv -fno-strict-aliasing ./lib/stub.c stubccflags
            /some/path/A/x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/release/build/runtime.o -I$MOON_HOME/include -g -c -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c
            moonc build-package ./lib/hello.mbt -o ./target/native/release/build/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moon_new/lib:./lib -target native -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/native/release/build/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/release/build/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target native -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/build/lib/lib.core ./target/native/release/build/main/main.core -main moon_new/main -o ./target/native/release/build/main/main.c -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native
            /other/path/B/x86_64-unknown-fake_os-fake_libc-ar -r -c -s ./target/native/release/build/lib/liblib.a ./target/native/release/build/lib/stub.o
            /some/path/A/x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/release/build/main/main.exe -I$MOON_HOME/include -fwrapv -fno-strict-aliasing ./target/native/release/build/main/main.c ./target/native/release/build/runtime.o ./target/native/release/build/lib/liblib.a -lm ccflags fasd cclinkflags
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/build/lib/lib.core -main moon_new/lib -o ./target/native/release/build/lib/lib.c -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native
            /some/path/A/x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/release/build/lib/lib.exe -I$MOON_HOME/include -fwrapv -fno-strict-aliasing ./target/native/release/build/lib/lib.c ./target/native/release/build/runtime.o ./target/native/release/build/lib/liblib.a -lm ccflags fasd cclinkflags
            ./target/native/release/build/lib/lib.exe
            ./target/native/release/build/main/main.exe
        "#]],
    );
}

#[test]
#[cfg(unix)]
fn test_native_backend_tcc_run() {
    let dir = TestDir::new("native_backend_tcc_run.in");
    check(
        get_stdout(
            &dir,
            ["build", "--target", "native", "--dry-run", "--sort-input"],
        ),
        expect![[r#"
            stubcc -o ./target/native/release/build/lib/stub.o -I$MOON_HOME/include -c -fwrapv -fno-strict-aliasing ./lib/stub.c stubccflags
            cc -o ./target/native/release/build/runtime.o -I$MOON_HOME/include -g -c -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c
            moonc build-package ./lib/hello.mbt -o ./target/native/release/build/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moon_new/lib:./lib -target native -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/native/release/build/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/release/build/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target native -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/build/lib/lib.core ./target/native/release/build/main/main.core -main moon_new/main -o ./target/native/release/build/main/main.c -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native
            stubar -r -c -s ./target/native/release/build/lib/liblib.a ./target/native/release/build/lib/stub.o
            cc -o ./target/native/release/build/main/main.exe -I$MOON_HOME/include -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/libmoonbitrun.o ./target/native/release/build/main/main.c ./target/native/release/build/runtime.o ./target/native/release/build/lib/liblib.a -lm
        "#]],
    );

    #[cfg(target_os = "macos")]
    check(
        get_stdout(
            &dir,
            ["test", "--target", "native", "--dry-run", "--sort-input"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/native/debug/test/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moon_new/lib:./lib -target native -g -O0 -workspace-path .
            moon generate-test-driver --output-driver ./target/native/debug/test/main/__generated_driver_for_internal_test.mbt --output-metadata ./target/native/debug/test/main/__internal_test_info.json ./main/main.mbt --target native --pkg-name moon_new/main --driver-kind internal
            moonc build-package ./main/main.mbt ./target/native/debug/test/main/__generated_driver_for_internal_test.mbt -o ./target/native/debug/test/main/main.internal_test.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/test/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target native -g -O0 -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.core ./target/native/debug/test/main/main.internal_test.core -main moon_new/main -o ./target/native/debug/test/main/main.internal_test.c -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
            moonc build-package ./main/main.mbt -o ./target/native/debug/test/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/test/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target native -g -O0 -workspace-path .
            moon generate-test-driver --output-driver ./target/native/debug/test/main/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/native/debug/test/main/__blackbox_test_info.json --doctest-only ./main/main.mbt --target native --pkg-name moon_new/main --driver-kind blackbox
            moonc build-package ./target/native/debug/test/main/__generated_driver_for_blackbox_test.mbt -doctest-only ./main/main.mbt -o ./target/native/debug/test/main/main.blackbox_test.core -pkg moon_new/main_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/test/main/main.mi:main -i ./target/native/debug/test/lib/lib.mi:lib -pkg-sources moon_new/main_blackbox_test:./main -target native -g -O0 -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.core ./target/native/debug/test/main/main.core ./target/native/debug/test/main/main.blackbox_test.core -main moon_new/main_blackbox_test -o ./target/native/debug/test/main/main.blackbox_test.c -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moon_new/main_blackbox_test:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
            cc -o ./target/native/debug/test/libruntime.dylib -I$MOON_HOME/include -g -shared -fPIC -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c -lm
            stubcc -o ./target/native/debug/test/lib/stub.o -I$MOON_HOME/include -g -c -fwrapv -fno-strict-aliasing -fPIC -DMOONBIT_USE_SHARED_RUNTIME ./lib/stub.c stubccflags
            stubcc -o ./target/native/debug/test/lib/liblib.dylib -L./target/native/debug/test -shared -fPIC ./target/native/debug/test/lib/stub.o -lm -lruntime -Wl,-rpath,./target/native/debug/test stubcclinkflags
            moon generate-test-driver --output-driver ./target/native/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./target/native/debug/test/lib/__internal_test_info.json ./lib/hello.mbt --target native --pkg-name moon_new/lib --driver-kind internal
            moonc build-package ./lib/hello.mbt ./target/native/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/native/debug/test/lib/lib.internal_test.core -pkg moon_new/lib -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moon_new/lib:./lib -target native -g -O0 -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.internal_test.core -main moon_new/lib -o ./target/native/debug/test/lib/lib.internal_test.c -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
            moon generate-test-driver --output-driver ./target/native/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/native/debug/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target native --pkg-name moon_new/lib --driver-kind blackbox
            moonc build-package ./target/native/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./target/native/debug/test/lib/lib.blackbox_test.core -pkg moon_new/lib_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/test/lib/lib.mi:lib -pkg-sources moon_new/lib_blackbox_test:./lib -target native -g -O0 -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.core ./target/native/debug/test/lib/lib.blackbox_test.core -main moon_new/lib_blackbox_test -o ./target/native/debug/test/lib/lib.blackbox_test.c -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/lib_blackbox_test:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
        "#]],
    );

    #[cfg(target_os = "linux")]
    check(
        get_stdout(
            &dir,
            ["test", "--target", "native", "--dry-run", "--sort-input"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/native/debug/test/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moon_new/lib:./lib -target native -g -O0 -workspace-path .
            moon generate-test-driver --output-driver ./target/native/debug/test/main/__generated_driver_for_internal_test.mbt --output-metadata ./target/native/debug/test/main/__internal_test_info.json ./main/main.mbt --target native --pkg-name moon_new/main --driver-kind internal
            moonc build-package ./main/main.mbt ./target/native/debug/test/main/__generated_driver_for_internal_test.mbt -o ./target/native/debug/test/main/main.internal_test.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/test/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target native -g -O0 -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.core ./target/native/debug/test/main/main.internal_test.core -main moon_new/main -o ./target/native/debug/test/main/main.internal_test.c -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
            moonc build-package ./main/main.mbt -o ./target/native/debug/test/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/test/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target native -g -O0 -workspace-path .
            moon generate-test-driver --output-driver ./target/native/debug/test/main/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/native/debug/test/main/__blackbox_test_info.json --doctest-only ./main/main.mbt --target native --pkg-name moon_new/main --driver-kind blackbox
            moonc build-package ./target/native/debug/test/main/__generated_driver_for_blackbox_test.mbt -doctest-only ./main/main.mbt -o ./target/native/debug/test/main/main.blackbox_test.core -pkg moon_new/main_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/test/main/main.mi:main -i ./target/native/debug/test/lib/lib.mi:lib -pkg-sources moon_new/main_blackbox_test:./main -target native -g -O0 -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.core ./target/native/debug/test/main/main.core ./target/native/debug/test/main/main.blackbox_test.core -main moon_new/main_blackbox_test -o ./target/native/debug/test/main/main.blackbox_test.c -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moon_new/main_blackbox_test:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
            cc -o ./target/native/debug/test/libruntime.so -I$MOON_HOME/include -g -shared -fPIC -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c -lm
            stubcc -o ./target/native/debug/test/lib/stub.o -I$MOON_HOME/include -g -c -fwrapv -fno-strict-aliasing -fPIC -DMOONBIT_USE_SHARED_RUNTIME ./lib/stub.c stubccflags
            stubcc -o ./target/native/debug/test/lib/liblib.so -L./target/native/debug/test -shared -fPIC ./target/native/debug/test/lib/stub.o -lm -lruntime -Wl,-rpath,./target/native/debug/test stubcclinkflags
            moon generate-test-driver --output-driver ./target/native/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./target/native/debug/test/lib/__internal_test_info.json ./lib/hello.mbt --target native --pkg-name moon_new/lib --driver-kind internal
            moonc build-package ./lib/hello.mbt ./target/native/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/native/debug/test/lib/lib.internal_test.core -pkg moon_new/lib -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moon_new/lib:./lib -target native -g -O0 -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.internal_test.core -main moon_new/lib -o ./target/native/debug/test/lib/lib.internal_test.c -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
            moon generate-test-driver --output-driver ./target/native/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/native/debug/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target native --pkg-name moon_new/lib --driver-kind blackbox
            moonc build-package ./target/native/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./target/native/debug/test/lib/lib.blackbox_test.core -pkg moon_new/lib_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/test/lib/lib.mi:lib -pkg-sources moon_new/lib_blackbox_test:./lib -target native -g -O0 -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.core ./target/native/debug/test/lib/lib.blackbox_test.core -main moon_new/lib_blackbox_test -o ./target/native/debug/test/lib/lib.blackbox_test.c -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/lib_blackbox_test:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
        "#]],
    );
}

#[test]
fn test_moon_check_filter_package() {
    let dir = TestDir::new("test_check_filter.in");

    check(
        get_stdout(&dir, ["check", "-p", "A", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc check ./A/hello.mbt ./A/test.mbt ./A/hello_wbtest.mbt -o ./target/wasm-gc/release/check/A/A.whitebox_test.mi -pkg username/hello/A -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/A:./A -target wasm-gc -whitebox-test -workspace-path .
            moonc check ./A/hello.mbt ./A/test.mbt -o ./target/wasm-gc/release/check/A/A.mi -pkg username/hello/A -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/A:./A -target wasm-gc -workspace-path .
            moonc check ./A/hello_test.mbt -doctest-only ./A/hello.mbt -doctest-only ./A/test.mbt -include-doctests -o ./target/wasm-gc/release/check/A/A.blackbox_test.mi -pkg username/hello/A_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/A/A.mi:A -pkg-sources username/hello/A_blackbox_test:./A -target wasm-gc -blackbox-test -workspace-path .
        "#]],
    );

    check(
        get_stdout(&dir, ["check", "-p", "main", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc check ./lib2/lib.mbt -o ./target/wasm-gc/release/check/lib2/lib2.mi -pkg username/hello/lib2 -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib2:./lib2 -target wasm-gc -workspace-path .
            moonc check ./lib/hello.mbt -o ./target/wasm-gc/release/check/lib/lib.mi -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib2/lib2.mi:lib2 -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path .
            moonc check ./main/main.mbt -o ./target/wasm-gc/release/check/main/main.mi -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm-gc -workspace-path .
            moonc check -doctest-only ./main/main.mbt -include-doctests -o ./target/wasm-gc/release/check/main/main.blackbox_test.mi -pkg username/hello/main_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/main/main.mi:main -i ./target/wasm-gc/release/check/lib/lib.mi:lib -pkg-sources username/hello/main_blackbox_test:./main -target wasm-gc -blackbox-test -workspace-path .
            moonc check -doctest-only ./lib2/lib.mbt -include-doctests -o ./target/wasm-gc/release/check/lib2/lib2.blackbox_test.mi -pkg username/hello/lib2_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib2/lib2.mi:lib2 -pkg-sources username/hello/lib2_blackbox_test:./lib2 -target wasm-gc -blackbox-test -workspace-path .
            moonc check -doctest-only ./lib/hello.mbt -include-doctests -o ./target/wasm-gc/release/check/lib/lib.blackbox_test.mi -pkg username/hello/lib_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib/lib.mi:lib -i ./target/wasm-gc/release/check/lib2/lib2.mi:lib2 -pkg-sources username/hello/lib_blackbox_test:./lib -target wasm-gc -blackbox-test -workspace-path .
        "#]],
    );

    check(
        get_stdout(&dir, ["check", "-p", "lib", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc check ./lib2/lib.mbt -o ./target/wasm-gc/release/check/lib2/lib2.mi -pkg username/hello/lib2 -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib2:./lib2 -target wasm-gc -workspace-path .
            moonc check -doctest-only ./lib2/lib.mbt -include-doctests -o ./target/wasm-gc/release/check/lib2/lib2.blackbox_test.mi -pkg username/hello/lib2_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib2/lib2.mi:lib2 -pkg-sources username/hello/lib2_blackbox_test:./lib2 -target wasm-gc -blackbox-test -workspace-path .
            moonc check ./lib/hello.mbt -o ./target/wasm-gc/release/check/lib/lib.mi -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib2/lib2.mi:lib2 -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path .
            moonc check -doctest-only ./lib/hello.mbt -include-doctests -o ./target/wasm-gc/release/check/lib/lib.blackbox_test.mi -pkg username/hello/lib_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib/lib.mi:lib -i ./target/wasm-gc/release/check/lib2/lib2.mi:lib2 -pkg-sources username/hello/lib_blackbox_test:./lib -target wasm-gc -blackbox-test -workspace-path .
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
            moonc check ./A/hello.mbt ./A/test.mbt ./A/hello_wbtest.mbt -o ./target/wasm-gc/release/check/A/A.whitebox_test.mi -pkg username/hello/A -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/A:./A -target wasm-gc -whitebox-test -workspace-path .
            moonc check -patch-file /path/to/patch.json ./A/hello.mbt ./A/test.mbt -o ./target/wasm-gc/release/check/A/A.mi -pkg username/hello/A -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/A:./A -target wasm-gc -workspace-path .
            moonc check ./A/hello_test.mbt -doctest-only ./A/hello.mbt -doctest-only ./A/test.mbt -include-doctests -o ./target/wasm-gc/release/check/A/A.blackbox_test.mi -pkg username/hello/A_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/A/A.mi:A -pkg-sources username/hello/A_blackbox_test:./A -target wasm-gc -blackbox-test -workspace-path .
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
            moonc check -patch-file /path/to/patch_wbtest.json ./A/hello.mbt ./A/test.mbt ./A/hello_wbtest.mbt -o ./target/wasm-gc/release/check/A/A.whitebox_test.mi -pkg username/hello/A -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/A:./A -target wasm-gc -whitebox-test -workspace-path .
            moonc check ./A/hello.mbt ./A/test.mbt -o ./target/wasm-gc/release/check/A/A.mi -pkg username/hello/A -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/A:./A -target wasm-gc -workspace-path .
            moonc check ./A/hello_test.mbt -doctest-only ./A/hello.mbt -doctest-only ./A/test.mbt -include-doctests -o ./target/wasm-gc/release/check/A/A.blackbox_test.mi -pkg username/hello/A_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/A/A.mi:A -pkg-sources username/hello/A_blackbox_test:./A -target wasm-gc -blackbox-test -workspace-path .
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
            moonc check ./A/hello.mbt ./A/test.mbt ./A/hello_wbtest.mbt -o ./target/wasm-gc/release/check/A/A.whitebox_test.mi -pkg username/hello/A -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/A:./A -target wasm-gc -whitebox-test -workspace-path .
            moonc check ./A/hello.mbt ./A/test.mbt -o ./target/wasm-gc/release/check/A/A.mi -pkg username/hello/A -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/A:./A -target wasm-gc -workspace-path .
            moonc check -patch-file /path/to/patch_test.json ./A/hello_test.mbt -doctest-only ./A/hello.mbt -doctest-only ./A/test.mbt -include-doctests -o ./target/wasm-gc/release/check/A/A.blackbox_test.mi -pkg username/hello/A_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/A/A.mi:A -pkg-sources username/hello/A_blackbox_test:./A -target wasm-gc -blackbox-test -workspace-path .
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
            moonc check ./lib2/lib.mbt -o ./target/wasm-gc/release/check/lib2/lib2.mi -pkg username/hello/lib2 -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib2:./lib2 -target wasm-gc -workspace-path .
            moonc check -doctest-only ./lib2/lib.mbt -include-doctests -o ./target/wasm-gc/release/check/lib2/lib2.blackbox_test.mi -pkg username/hello/lib2_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib2/lib2.mi:lib2 -pkg-sources username/hello/lib2_blackbox_test:./lib2 -target wasm-gc -blackbox-test -workspace-path .
            moonc check -patch-file /path/to/patch.json ./lib/hello.mbt -o ./target/wasm-gc/release/check/lib/lib.mi -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib2/lib2.mi:lib2 -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path .
            moonc check -doctest-only ./lib/hello.mbt -include-doctests -o ./target/wasm-gc/release/check/lib/lib.blackbox_test.mi -pkg username/hello/lib_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib/lib.mi:lib -i ./target/wasm-gc/release/check/lib2/lib2.mi:lib2 -pkg-sources username/hello/lib_blackbox_test:./lib -target wasm-gc -blackbox-test -workspace-path .
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
            moonc check ./lib2/lib.mbt -o ./target/wasm-gc/release/check/lib2/lib2.mi -pkg username/hello/lib2 -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib2:./lib2 -target wasm-gc -workspace-path .
            moonc check -doctest-only ./lib2/lib.mbt -include-doctests -o ./target/wasm-gc/release/check/lib2/lib2.blackbox_test.mi -pkg username/hello/lib2_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib2/lib2.mi:lib2 -pkg-sources username/hello/lib2_blackbox_test:./lib2 -target wasm-gc -blackbox-test -workspace-path .
            moonc check ./lib/hello.mbt -o ./target/wasm-gc/release/check/lib/lib.mi -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib2/lib2.mi:lib2 -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path .
            moonc check -patch-file /path/to/patch_test.json -doctest-only ./lib/hello.mbt -include-doctests -o ./target/wasm-gc/release/check/lib/lib.blackbox_test.mi -pkg username/hello/lib_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib/lib.mi:lib -i ./target/wasm-gc/release/check/lib2/lib2.mi:lib2 -pkg-sources username/hello/lib_blackbox_test:./lib -target wasm-gc -blackbox-test -workspace-path .
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
            moonc check ./lib2/lib.mbt -o ./target/wasm-gc/release/check/lib2/lib2.mi -pkg username/hello/lib2 -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib2:./lib2 -target wasm-gc -workspace-path .
            moonc check ./lib/hello.mbt -o ./target/wasm-gc/release/check/lib/lib.mi -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib2/lib2.mi:lib2 -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path .
            moonc check -patch-file /path/to/patch.json -no-mi ./main/main.mbt -o ./target/wasm-gc/release/check/main/main.mi -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm-gc -workspace-path .
            moonc check -no-mi -doctest-only ./main/main.mbt -include-doctests -o ./target/wasm-gc/release/check/main/main.blackbox_test.mi -pkg username/hello/main_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/main/main.mi:main -i ./target/wasm-gc/release/check/lib/lib.mi:lib -pkg-sources username/hello/main_blackbox_test:./main -target wasm-gc -blackbox-test -workspace-path .
            moonc check -doctest-only ./lib2/lib.mbt -include-doctests -o ./target/wasm-gc/release/check/lib2/lib2.blackbox_test.mi -pkg username/hello/lib2_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib2/lib2.mi:lib2 -pkg-sources username/hello/lib2_blackbox_test:./lib2 -target wasm-gc -blackbox-test -workspace-path .
            moonc check -doctest-only ./lib/hello.mbt -include-doctests -o ./target/wasm-gc/release/check/lib/lib.blackbox_test.mi -pkg username/hello/lib_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib/lib.mi:lib -i ./target/wasm-gc/release/check/lib2/lib2.mi:lib2 -pkg-sources username/hello/lib_blackbox_test:./lib -target wasm-gc -blackbox-test -workspace-path .
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
fn test_moon_test_patch() {
    let dir = TestDir::new("moon_test/patch");
    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "moon_new/lib",
                "-f",
                "hello_0.mbt",
                "--patch-file",
                "./patch.json",
                "--dry-run",
                "--sort-input",
                "--nostd",
            ],
        ),
        expect![[r#"
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_whitebox_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__whitebox_test_info.json ./lib/hello_wbtest.mbt --target wasm-gc --pkg-name moon_new/lib --driver-kind whitebox
            moonc build-package ./lib/hello.mbt ./lib/hello_wbtest.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_whitebox_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.whitebox_test.core -pkg moon_new/lib -is-main -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -source-map -whitebox-test -no-mi -test-mode -workspace-path .
            moonc link-core ./target/wasm-gc/debug/test/lib/lib.whitebox_test.core -main moon_new/lib -o ./target/wasm-gc/debug/test/lib/lib.whitebox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__internal_test_info.json ./lib/hello.mbt --patch-file ./patch.json --target wasm-gc --pkg-name moon_new/lib --driver-kind internal
            moonc build-package ./lib/hello.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.internal_test.core -pkg moon_new/lib -is-main -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -source-map -no-mi -patch-file ./patch.json -test-mode -workspace-path .
            moonc link-core ./target/wasm-gc/debug/test/lib/lib.internal_test.core -main moon_new/lib -o ./target/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/test/lib/lib.core -pkg moon_new/lib -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -source-map -workspace-path .
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__blackbox_test_info.json ./lib/hello_test.mbt --doctest-only ./lib/hello.mbt --target wasm-gc --pkg-name moon_new/lib --driver-kind blackbox
            moonc build-package ./lib/hello_test.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -pkg moon_new/lib_blackbox_test -is-main -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources moon_new/lib_blackbox_test:./lib -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core ./target/wasm-gc/debug/test/lib/lib.core ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -main moon_new/lib_blackbox_test -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/lib_blackbox_test:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "moon_new/lib",
                "-f",
                "hello_0.mbt",
                "--patch-file",
                "./patch.json",
            ],
        ),
        expect![[r#"
            hello from patch.json
            hello from lib/hello.mbt
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "moon_new/lib",
                "-f",
                "hello_1_wbtest.mbt",
                "--patch-file",
                "./patch_wbtest.json",
                "--dry-run",
                "--sort-input",
                "--nostd",
            ],
        ),
        expect![[r#"
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_whitebox_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__whitebox_test_info.json ./lib/hello_wbtest.mbt --patch-file ./patch_wbtest.json --target wasm-gc --pkg-name moon_new/lib --driver-kind whitebox
            moonc build-package ./lib/hello.mbt ./lib/hello_wbtest.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_whitebox_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.whitebox_test.core -pkg moon_new/lib -is-main -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -source-map -whitebox-test -no-mi -patch-file ./patch_wbtest.json -test-mode -workspace-path .
            moonc link-core ./target/wasm-gc/debug/test/lib/lib.whitebox_test.core -main moon_new/lib -o ./target/wasm-gc/debug/test/lib/lib.whitebox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__internal_test_info.json ./lib/hello.mbt --target wasm-gc --pkg-name moon_new/lib --driver-kind internal
            moonc build-package ./lib/hello.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.internal_test.core -pkg moon_new/lib -is-main -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core ./target/wasm-gc/debug/test/lib/lib.internal_test.core -main moon_new/lib -o ./target/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/test/lib/lib.core -pkg moon_new/lib -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -source-map -workspace-path .
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__blackbox_test_info.json ./lib/hello_test.mbt --doctest-only ./lib/hello.mbt --target wasm-gc --pkg-name moon_new/lib --driver-kind blackbox
            moonc build-package ./lib/hello_test.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -pkg moon_new/lib_blackbox_test -is-main -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources moon_new/lib_blackbox_test:./lib -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core ./target/wasm-gc/debug/test/lib/lib.core ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -main moon_new/lib_blackbox_test -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/lib_blackbox_test:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "moon_new/lib",
                "-f",
                "hello_1_wbtest.mbt",
                "--patch-file",
                "./patch_wbtest.json",
            ],
        ),
        expect![[r#"
            hello from patch_wbtest.json
            hello from lib/hello.mbt
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "moon_new/lib",
                "-f",
                "hello_2_test.mbt",
                "--patch-file",
                "./patch_test.json",
                "--dry-run",
                "--sort-input",
                "--nostd",
            ],
        ),
        expect![[r#"
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_whitebox_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__whitebox_test_info.json ./lib/hello_wbtest.mbt --target wasm-gc --pkg-name moon_new/lib --driver-kind whitebox
            moonc build-package ./lib/hello.mbt ./lib/hello_wbtest.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_whitebox_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.whitebox_test.core -pkg moon_new/lib -is-main -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -source-map -whitebox-test -no-mi -test-mode -workspace-path .
            moonc link-core ./target/wasm-gc/debug/test/lib/lib.whitebox_test.core -main moon_new/lib -o ./target/wasm-gc/debug/test/lib/lib.whitebox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__internal_test_info.json ./lib/hello.mbt --target wasm-gc --pkg-name moon_new/lib --driver-kind internal
            moonc build-package ./lib/hello.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.internal_test.core -pkg moon_new/lib -is-main -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core ./target/wasm-gc/debug/test/lib/lib.internal_test.core -main moon_new/lib -o ./target/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/test/lib/lib.core -pkg moon_new/lib -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -source-map -workspace-path .
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__blackbox_test_info.json ./lib/hello_test.mbt --doctest-only ./lib/hello.mbt --patch-file ./patch_test.json --target wasm-gc --pkg-name moon_new/lib --driver-kind blackbox
            moonc build-package ./lib/hello_test.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -pkg moon_new/lib_blackbox_test -is-main -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources moon_new/lib_blackbox_test:./lib -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -patch-file ./patch_test.json -test-mode -workspace-path .
            moonc link-core ./target/wasm-gc/debug/test/lib/lib.core ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -main moon_new/lib_blackbox_test -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/lib_blackbox_test:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "moon_new/lib",
                "-f",
                "hello_2_test.mbt",
                "--patch-file",
                "./patch_test.json",
            ],
        ),
        expect![[r#"
            hello from patch_test.json
            hello from lib/hello.mbt
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    // no _test.mbt and _wbtest.mbt in original package
    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "moon_new/lib2",
                "-f",
                "hello_2_test.mbt",
                "--patch-file",
                "./2.patch_test.json",
            ],
        ),
        expect![[r#"
            hello from 2.patch_test.json
            hello from lib2/hello.mbt
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "moon_new/lib2",
                "-f",
                "hello_2_wbtest.mbt",
                "--patch-file",
                "./2.patch_wbtest.json",
            ],
        ),
        expect![[r#"
            hello from 2.patch_wbtest.json
            hello from lib2/hello.mbt
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
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
               │                  ╰────────────── Warning: Unused variable 'unused_in_patch_test_json'
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
               │                   ╰─────────────── Warning: Unused variable 'unused_in_patch_wbtest_json'
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
               │                ╰─────────── Warning: Unused variable 'unused_in_patch_json'
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
               │                  ╰────────────── Warning: Unused variable 'unused_in_patch_test_json'
               │ 
               │ Help: # E0002
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
            moonc check ./lib/hello.mbt -o ./target/wasm-gc/release/check/lib/lib.mi -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path .
            moonc check ./main/main.mbt -o ./target/wasm-gc/release/check/main/main.mi -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm-gc -workspace-path .
            moonc check -doctest-only ./main/main.mbt -include-doctests -o ./target/wasm-gc/release/check/main/main.blackbox_test.mi -pkg username/hello/main_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/main/main.mi:main -i ./target/wasm-gc/release/check/lib/lib.mi:lib -pkg-sources username/hello/main_blackbox_test:./main -target wasm-gc -blackbox-test -workspace-path .
            moonc check ./lib3/hello.mbt -o ./target/wasm-gc/release/check/lib3/lib3.mi -pkg username/hello/lib3 -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib3:./lib3 -target wasm-gc -workspace-path .
            moonc check ./lib3/hello_test.mbt -doctest-only ./lib3/hello.mbt -include-doctests -o ./target/wasm-gc/release/check/lib3/lib3.blackbox_test.mi -pkg username/hello/lib3_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib3/lib3.mi:lib3 -pkg-sources username/hello/lib3_blackbox_test:./lib3 -target wasm-gc -blackbox-test -workspace-path .
            moonc check ./lib2/hello.mbt -o ./target/wasm-gc/release/check/lib2/lib2.mi -pkg username/hello/lib2 -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib2:./lib2 -target wasm-gc -workspace-path .
            moonc check ./lib2/hello_test.mbt -doctest-only ./lib2/hello.mbt -include-doctests -o ./target/wasm-gc/release/check/lib2/lib2.blackbox_test.mi -pkg username/hello/lib2_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib2/lib2.mi:lib2 -pkg-sources username/hello/lib2_blackbox_test:./lib2 -target wasm-gc -blackbox-test -workspace-path .
            moonc check ./lib/hello_test.mbt -doctest-only ./lib/hello.mbt -include-doctests -o ./target/wasm-gc/release/check/lib/lib.blackbox_test.mi -pkg username/hello/lib_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib/lib.mi:lll -pkg-sources username/hello/lib_blackbox_test:./lib -target wasm-gc -blackbox-test -workspace-path .
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

#[test]
fn test_run_doc_test() {
    let dir = TestDir::new("run_doc_test.in");

    // `moon test --doc` run doc test only
    check(
        get_err_stdout(&dir, ["test", "--sort-input", "--doc"]),
        expect![[r#"
            hello from hello_test.mbt
            doc_test 1 from hello.mbt
            doc_test 2 from hello.mbt
            doc_test 3 from hello.mbt
            doc_test
            doc_test 1 from greet.mbt
            test block 1
            test block 2
            test block 3
            doc_test 3 from greet.mbt
            test block 4
            test block 5
            doc_test 5 from greet.mbt
            [username/hello] test lib/hello.mbt:9 (#1) failed
            expect test failed at $ROOT/src/lib/hello.mbt:12:5-12:18
            Diff: (- expected, + actual)
            ----
            +1256
            ----

            [username/hello] test lib/hello.mbt:19 (#2) failed: $ROOT/src/lib/hello.mbt:22:5-22:30 FAILED: this is a failure
            [username/hello] test lib/greet.mbt:18 (#2) failed
            expect test failed at $ROOT/src/lib/greet.mbt:23:7-23:20
            Diff: (- expected, + actual)
            ----
            +1256
            ----

            [username/hello] test lib/greet.mbt:30 (#3) failed: $ROOT/src/lib/greet.mbt:34:7-34:30 FAILED: another failure
            [username/hello] test lib/greet.mbt:95 (#8) failed
            expect test failed at $ROOT/src/lib/greet.mbt:99:5-99:40
            Diff: (- expected, + actual)
            ----
            +b"T/x00e/x00s/x00t/x00"
            ----

            Total tests: 16, passed: 11, failed: 5.
        "#]],
    );

    check(
        get_err_stdout(&dir, ["test", "--sort-input", "--update"]),
        expect![[r#"
            hello from hello_test.mbt
            doc_test 1 from hello.mbt
            doc_test 2 from hello.mbt
            doc_test 3 from hello.mbt
            doc_test
            doc_test 1 from greet.mbt
            test block 1
            test block 2
            test block 3
            doc_test 3 from greet.mbt
            test block 4
            test block 5
            doc_test 5 from greet.mbt

            Auto updating expect tests and retesting ...

            doc_test 2 from hello.mbt
            doc_test 2 from hello.mbt
            [username/hello] test lib/hello.mbt:19 (#2) failed: $ROOT/src/lib/hello.mbt:22:5-22:30 FAILED: this is a failure
            test block 2
            test block 2
            [username/hello] test lib/greet.mbt:30 (#3) failed: $ROOT/src/lib/greet.mbt:34:7-34:30 FAILED: another failure
            Total tests: 16, passed: 14, failed: 2.
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

    let mut _1 = PathBuf::from("");
    let mut _2 = PathBuf::from("");
    let mut _3 = PathBuf::from("");
    let mut _4 = PathBuf::from("");
    let mut _5 = PathBuf::from("");

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
    // ignore some cl warnings
    let mut lines = content.lines().rev().take(5).collect::<Vec<_>>();
    lines.reverse();
    check(
        lines.join("\n"),
        expect![[r#"
            main-js
            lib Hello, world!
            ()
            Executed 1 pre-build task, now up to date
            Finished. moon: ran 3 tasks, now up to date"#]],
    );
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
fn test_tracing_value_for_main_func() {
    let dir = TestDir::new("tracing_value.in");

    // main.mbt in package
    check(
        get_stdout(
            &dir,
            [
                "run",
                "./main/main.mbt",
                "--enable-value-tracing",
                "--dry-run",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target wasm-gc -enable-value-tracing -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main moon_new/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc
            moonrun ./target/wasm-gc/release/build/main/main.wasm
        "#]],
    );
    let content = get_stdout(&dir, ["run", "./main/main.mbt", "--enable-value-tracing"])
        .split("######MOONBIT_VALUE_TRACING_START######")
        .filter(|l| !(l.contains("__generated_driver_for")))
        .collect::<Vec<_>>()
        .join("\n");
    check(
        content,
        expect![[r#"
            Hello, world!


            {"name": "a", "line": 3, "start_column": 7, "end_column": 8, "filepath": "$ROOT/main/main.mbt", "value": "$placeholder"}
            ######MOONBIT_VALUE_TRACING_CONTENT_START######
            1
            ######MOONBIT_VALUE_TRACING_CONTENT_END######
            ######MOONBIT_VALUE_TRACING_END######


            {"name": "b", "line": 4, "start_column": 7, "end_column": 8, "filepath": "$ROOT/main/main.mbt", "value": "$placeholder"}
            ######MOONBIT_VALUE_TRACING_CONTENT_START######
            2
            ######MOONBIT_VALUE_TRACING_CONTENT_END######
            ######MOONBIT_VALUE_TRACING_END######


            {"name": "c", "line": 5, "start_column": 7, "end_column": 8, "filepath": "$ROOT/main/main.mbt", "value": "$placeholder"}
            ######MOONBIT_VALUE_TRACING_CONTENT_START######
            3
            ######MOONBIT_VALUE_TRACING_CONTENT_END######
            ######MOONBIT_VALUE_TRACING_END######
            3
        "#]],
    );

    // single file
    check(
        get_stdout(
            &dir,
            ["run", "./main.mbt", "--enable-value-tracing", "--dry-run"],
        ),
        expect![[r#"
            moonc build-package $ROOT/main.mbt -o $ROOT/target/main.core -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -is-main -pkg moon/run/single -g -O0 -source-map -target wasm-gc -enable-value-tracing
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core $ROOT/target/main.core -o $ROOT/target/main.wasm -pkg-sources moon/run/single:$ROOT -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -g -O0 -source-map -target wasm-gc
            moonrun $ROOT/target/main.wasm
        "#]],
    );
    let content = get_stdout(&dir, ["run", "./main.mbt", "--enable-value-tracing"])
        .split("######MOONBIT_VALUE_TRACING_START######")
        .filter(|l| !(l.contains("__generated_driver_for")))
        .collect::<Vec<_>>()
        .join("\n");
    check(
        content,
        expect![[r#"


            {"name": "a", "line": 2, "start_column": 7, "end_column": 8, "filepath": "moon/run/single/main.mbt", "value": "$placeholder"}
            ######MOONBIT_VALUE_TRACING_CONTENT_START######
            1
            ######MOONBIT_VALUE_TRACING_CONTENT_END######
            ######MOONBIT_VALUE_TRACING_END######


            {"name": "b", "line": 3, "start_column": 7, "end_column": 8, "filepath": "moon/run/single/main.mbt", "value": "$placeholder"}
            ######MOONBIT_VALUE_TRACING_CONTENT_START######
            2
            ######MOONBIT_VALUE_TRACING_CONTENT_END######
            ######MOONBIT_VALUE_TRACING_END######


            {"name": "c", "line": 4, "start_column": 7, "end_column": 8, "filepath": "moon/run/single/main.mbt", "value": "$placeholder"}
            ######MOONBIT_VALUE_TRACING_CONTENT_START######
            3
            ######MOONBIT_VALUE_TRACING_CONTENT_END######
            ######MOONBIT_VALUE_TRACING_END######
            3
        "#]],
    );
}

#[test]
fn test_tracing_value_for_test_block() {
    let dir = TestDir::new("tracing_value_for_test_block.in");

    check(
        get_stdout(
            &dir,
            [
                "test",
                "--enable-value-tracing",
                "-p",
                "moon_new/lib1",
                "--dry-run",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib2/hello.mbt -o ./target/wasm-gc/debug/test/lib2/lib2.core -pkg moon_new/lib2 -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moon_new/lib2:./lib2 -target wasm-gc -g -O0 -source-map -workspace-path .
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib1/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib1/__internal_test_info.json ./lib1/hello.mbt --target wasm-gc --pkg-name moon_new/lib1 --driver-kind internal
            moonc build-package ./lib1/hello.mbt ./target/wasm-gc/debug/test/lib1/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib1/lib1.internal_test.core -pkg moon_new/lib1 -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/lib2/lib2.mi:lib2 -pkg-sources moon_new/lib1:./lib1 -target wasm-gc -g -O0 -source-map -no-mi -test-mode -enable-value-tracing -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib2/lib2.core ./target/wasm-gc/debug/test/lib1/lib1.internal_test.core -main moon_new/lib1 -o ./target/wasm-gc/debug/test/lib1/lib1.internal_test.wasm -test-mode -pkg-config-path ./lib1/moon.pkg.json -pkg-sources moon_new/lib2:./lib2 -pkg-sources moon_new/lib1:./lib1 -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moonc build-package ./lib1/hello.mbt -o ./target/wasm-gc/debug/test/lib1/lib1.core -pkg moon_new/lib1 -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/lib2/lib2.mi:lib2 -pkg-sources moon_new/lib1:./lib1 -target wasm-gc -g -O0 -source-map -enable-value-tracing -workspace-path .
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib1/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib1/__blackbox_test_info.json --doctest-only ./lib1/hello.mbt --target wasm-gc --pkg-name moon_new/lib1 --driver-kind blackbox
            moonc build-package ./target/wasm-gc/debug/test/lib1/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib1/hello.mbt -o ./target/wasm-gc/debug/test/lib1/lib1.blackbox_test.core -pkg moon_new/lib1_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/lib1/lib1.mi:lib1 -i ./target/wasm-gc/debug/test/lib2/lib2.mi:lib2 -pkg-sources moon_new/lib1_blackbox_test:./lib1 -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -enable-value-tracing -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib2/lib2.core ./target/wasm-gc/debug/test/lib1/lib1.core ./target/wasm-gc/debug/test/lib1/lib1.blackbox_test.core -main moon_new/lib1_blackbox_test -o ./target/wasm-gc/debug/test/lib1/lib1.blackbox_test.wasm -test-mode -pkg-config-path ./lib1/moon.pkg.json -pkg-sources moon_new/lib2:./lib2 -pkg-sources moon_new/lib1:./lib1 -pkg-sources moon_new/lib1_blackbox_test:./lib1 -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
        "#]],
    );

    let content = get_stdout(
        &dir,
        ["test", "-p", "moon_new/lib1", "--enable-value-tracing"],
    )
    .split("######MOONBIT_VALUE_TRACING_START######")
    .filter(|l| !(l.contains("__generated_driver_for")))
    .collect::<Vec<_>>()
    .join("\n");
    check(
        content,
        expect![[r#"


            {"name": "a", "line": 6, "start_column": 7, "end_column": 8, "filepath": "$ROOT/lib1/hello.mbt", "value": "$placeholder"}
            ######MOONBIT_VALUE_TRACING_CONTENT_START######
            1
            ######MOONBIT_VALUE_TRACING_CONTENT_END######
            ######MOONBIT_VALUE_TRACING_END######


            {"name": "b", "line": 7, "start_column": 7, "end_column": 8, "filepath": "$ROOT/lib1/hello.mbt", "value": "$placeholder"}
            ######MOONBIT_VALUE_TRACING_CONTENT_START######
            2
            ######MOONBIT_VALUE_TRACING_CONTENT_END######
            ######MOONBIT_VALUE_TRACING_END######


            {"name": "c", "line": 8, "start_column": 7, "end_column": 8, "filepath": "$ROOT/lib1/hello.mbt", "value": "$placeholder"}
            ######MOONBIT_VALUE_TRACING_CONTENT_START######
            3
            ######MOONBIT_VALUE_TRACING_CONTENT_END######
            ######MOONBIT_VALUE_TRACING_END######
            Hello, world!
        "#]],
    );
}

#[test]
fn test_diff_mbti() {
    let dir = TestDir::new("diff_mbti.in");
    let content = get_err_stdout(&dir, ["info", "--target", "all"]);
    assert!(content.contains("$ROOT/target/wasm-gc/release/check/lib/lib.mbti"));
    assert!(content.contains("$ROOT/target/js/release/check/lib/lib.mbti"));
    assert!(content.contains("-fn aaa() -> String"));
    assert!(content.contains("+fn a() -> String"));
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
        !dir.join("target")
            .join("native")
            .join("release")
            .join("build")
            .join("lib")
            .join("lib.c")
            .exists()
    );
    let lib2_c = read(
        dir.join("target")
            .join("native")
            .join("release")
            .join("build")
            .join("lib2")
            .join("lib2.c"),
    );
    assert!(lib2_c.contains("$username$hello$lib2$hello()"));

    // alias not works
    let lib3_c = read(
        dir.join("target")
            .join("native")
            .join("release")
            .join("build")
            .join("lib3")
            .join("lib3.c"),
    );
    assert!(lib3_c.contains("$username$hello$lib3$hello()"));
}

#[test]
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
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources hello/main:./main -target wasm-gc -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/main/main.core -main hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc
        "#]],
    );
}

#[test]
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
              inspect("\x0b", content="\u{b}")
              inspect("a\x0b", content="a\u{b}")
              inspect("a\x00b", content="a\u{0}b")
              inspect("a\x00b\x19", content="a\u{0}b\u{19}")
              inspect("\na\n\x00\nb\n\x19", content=
                "\u{a}a\u{a}\u{0}\u{a}b\u{a}\u{19}")
              inspect("\n\"a\n\x00\nb\"\n\x19", content=
                "\u{a}\"a\u{a}\u{0}\u{a}b\"\u{a}\u{19}")
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
              inspect("\x0b\"a\nb\nc\"", content=
                "\u{b}\"a\u{a}b\u{a}c\"")
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
fn test_run_md_test() {
    let dir = TestDir::new("run_md_test.in");

    check(
        get_stderr(&dir, ["check", "--sort-input"]),
        expect![[r#"
            Warning: [0002]
                ╭─[ $ROOT/src/lib/1.mbt.md:42:9 ]
                │
             42 │     let a = 1
                │         ┬  
                │         ╰── Warning: Unused variable 'a'
            ────╯
            Finished. moon: ran 4 tasks, now up to date (1 warnings, 0 errors)
        "#]],
    );

    check(
        get_err_stdout(&dir, ["test", "--sort-input"]),
        expect![[r#"
            hello from hello_test.mbt
            fn in md test
            hello from hello_test.mbt
            Hello, world 1!
            Hello, world 3!
            ```moonbit
            fn main {
              println("Hello")
            }
            ```
            Hello, world 2!
            [username/hello] test lib/hello_test.mbt:10 ("inspect in bbtest") failed
            expect test failed at $ROOT/src/lib/hello_test.mbt:12:3-12:31
            Diff: (- expected, + actual)
            ----
            +inspect in bbtest
            ----

            [username/hello] test lib/1.mbt.md:32 (#2) failed
            expect test failed at $ROOT/src/lib/1.mbt.md:44:5-44:20
            Diff: (- expected, + actual)
            ----
            +4234
            ----

            [username/hello] test lib/1.mbt.md:53 (#3) failed
            expect test failed at $ROOT/src/lib/1.mbt.md:61:5-61:15
            Diff: (- expected, + actual)
            ----
            + all
            + wishes
            +
            + come
            + true
            ----

            Total tests: 7, passed: 4, failed: 3.
        "#]],
    );

    // test filter in md test
    check(
        get_stdout(
            &dir,
            [
                "test",
                "--sort-input",
                "-p",
                "lib",
                "-f",
                "1.mbt.md",
                "-i",
                "1",
            ],
        ),
        expect![[r#"
            Hello, world 3!
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout(&dir, ["test", "--update", "--sort-input"]),
        expect![[r#"
            hello from hello_test.mbt
            fn in md test
            hello from hello_test.mbt
            Hello, world 1!
            Hello, world 3!
            ```moonbit
            fn main {
              println("Hello")
            }
            ```
            Hello, world 2!

            Auto updating expect tests and retesting ...

            fn in md test
            fn in md test
            fn in md test
            ```moonbit
            fn main {
              println("Hello")
            }
            ```
            Hello, world 2!
            ```moonbit
            fn main {
              println("Hello")
            }
            ```
            Hello, world 2!
            Total tests: 7, passed: 7, failed: 0.
        "#]],
    );

    #[cfg(unix)]
    {
        get_stdout(&dir, ["check", "--sort-input"]);
        let p = dir.join("target/packages.json");
        check(
            replace_dir(&std::fs::read_to_string(p).unwrap(), &dir),
            expect![[r#"
                {
                  "source_dir": "$ROOT",
                  "name": "username/hello",
                  "packages": [
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/src/lib",
                      "root": "username/hello",
                      "rel": "lib",
                      "files": {
                        "$ROOT/src/lib/hello.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        }
                      },
                      "wbtest-files": {},
                      "test-files": {
                        "$ROOT/src/lib/hello_test.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        }
                      },
                      "mbt-md-files": {
                        "$ROOT/src/lib/1.mbt.md": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        },
                        "$ROOT/src/lib/2.mbt.md": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        }
                      },
                      "deps": [],
                      "wbtest-deps": [],
                      "test-deps": [],
                      "artifact": "$ROOT/target/wasm-gc/release/check/lib/lib.mi"
                    },
                    {
                      "is-main": true,
                      "is-third-party": false,
                      "root-path": "$ROOT/src/main",
                      "root": "username/hello",
                      "rel": "main",
                      "files": {
                        "$ROOT/src/main/main.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        }
                      },
                      "wbtest-files": {},
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [
                        {
                          "path": "username/hello/lib",
                          "alias": "lib",
                          "fspath": "$ROOT/src/lib"
                        }
                      ],
                      "wbtest-deps": [],
                      "test-deps": [],
                      "artifact": "$ROOT/target/wasm-gc/release/check/main/main.mi"
                    }
                  ],
                  "deps": [],
                  "backend": "wasm-gc",
                  "opt_level": "release",
                  "source": "src"
                }"#]],
        );
    }
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
fn native_backend_test_filter() {
    let dir = TestDir::new("native_backend_test_filter.in");

    check(
        get_stdout(
            &dir,
            [
                "test",
                "--target",
                "native",
                "-p",
                "lib",
                "-f",
                "hello.mbt",
                "-i",
                "3",
                "--sort-input",
            ],
        ),
        expect![[r#"
            test C
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "test",
                "--target",
                "native",
                "-p",
                "lib",
                "-f",
                "hello.mbt",
                "-i",
                "2",
                "-u",
                "--sort-input",
            ],
        ),
        expect![[r#"

            Auto updating expect tests and retesting ...

            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "--target",
                "native",
                "-p",
                "lib",
                "-f",
                "hello_wbtest.mbt",
                "-i",
                "1",
                "-u",
                "--sort-input",
            ],
        ),
        expect![[r#"

            Auto updating expect tests and retesting ...

            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "test",
                "--target",
                "native",
                "-p",
                "lib",
                "-f",
                "hello_wbtest.mbt",
                "-i",
                "0",
                "--sort-input",
            ],
        ),
        expect![[r#"
            test hello_0
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_err_stdout(
            &dir,
            [
                "test",
                "--target",
                "native",
                "-p",
                "lib",
                "-f",
                "hello.mbt",
                "-i",
                "4",
                "--sort-input",
            ],
        ),
        expect![[r#"
            [username/hello] test lib/hello.mbt:22 ("D") failed
            expect test failed at $ROOT/lib/hello.mbt:24:3
            Diff: (- expected, + actual)
            ----
            +test D
            ----

            Total tests: 1, passed: 0, failed: 1.
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "test",
                "--target",
                "native",
                "-p",
                "lib",
                "-f",
                "hello.mbt",
                "-i",
                "4",
                "-u",
                "--sort-input",
            ],
        ),
        expect![[r#"

            Auto updating expect tests and retesting ...

            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
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
    check(
        get_err_stderr(&dir, ["build"]),
        expect![[r#"
            error: Ambiguous package name: my/name/is/ambiguous
            Candidates:
              ambiguous in my/name/is ($ROOT/deps/ambiguous/src/ambiguous)
              is/ambiguous in my/name ($ROOT/src/is/ambiguous)
        "#]],
    );
}

#[test]
fn test_virtual_pkg() {
    let dir = TestDir::new("virtual_pkg");

    let virtual_pkg = dir.join("virtual");

    check(
        get_stdout(&virtual_pkg, ["run", "main", "--dry-run"]),
        expect![[r#"
            moonc build-interface ./lib3/pkg.mbti -o ./target/wasm-gc/release/build/lib3/lib3.mi -pkg username/hello/lib3 -pkg-sources username/hello/lib3:./lib3 -virtual -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -error-format=json
            moonc build-interface ./lib1/pkg.mbti -o ./target/wasm-gc/release/build/lib1/lib1.mi -pkg username/hello/lib1 -pkg-sources username/hello/lib1:./lib1 -virtual -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -error-format=json
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/lib1/lib1.mi:lib1 -i ./target/wasm-gc/release/build/lib3/lib3.mi:lib3 -pkg-sources username/hello/main:./main -target wasm-gc -workspace-path .
            moonc build-package ./lib4/hello.mbt -o ./target/wasm-gc/release/build/lib4/lib4.core -pkg username/hello/lib4 -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib4:./lib4 -target wasm-gc -check-mi ./target/wasm-gc/release/build/lib3/lib3.mi -impl-virtual -no-mi -pkg-sources username/hello/lib3:./lib3 -workspace-path .
            moonc build-package ./dummy_lib/hello.mbt -o ./target/wasm-gc/release/build/dummy_lib/dummy_lib.core -pkg username/hello/dummy_lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/dummy_lib:./dummy_lib -target wasm-gc -workspace-path .
            moonc build-package ./lib2/hello.mbt -o ./target/wasm-gc/release/build/lib2/lib2.core -pkg username/hello/lib2 -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/dummy_lib/dummy_lib.mi:dummy_lib -pkg-sources username/hello/lib2:./lib2 -target wasm-gc -check-mi ./target/wasm-gc/release/build/lib1/lib1.mi -impl-virtual -no-mi -pkg-sources username/hello/lib1:./lib1 -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/dummy_lib/dummy_lib.core ./target/wasm-gc/release/build/lib2/lib2.core ./target/wasm-gc/release/build/lib4/lib4.core ./target/wasm-gc/release/build/main/main.core -main username/hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/dummy_lib:./dummy_lib -pkg-sources username/hello/lib2:./lib2 -pkg-sources username/hello/lib4:./lib4 -pkg-sources username/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc
            moonrun ./target/wasm-gc/release/build/main/main.wasm
        "#]],
    );
    check(
        get_stdout(&virtual_pkg, ["run", "main"]),
        expect![[r#"
            another impl for f1 in lib2: 1
            another impl for f2 in lib2: 2
            another impl for f3 in lib4
        "#]],
    );
    check(
        get_stdout(&virtual_pkg, ["test", "--no-parallelize"]),
        expect![[r#"
            internal test
            default impl for f1 in lib1: 1
            another impl for f3 in lib4
            wb test
            default impl for f1 in lib1: 1
            another impl for f3 in lib4
            bb test
            default impl for f1 in lib1: 1
            another impl for f3 in lib4
            Total tests: 3, passed: 3, failed: 0.
        "#]],
    );

    let user = dir.join("user");
    check(
        get_stdout(&user, ["run", "main"]),
        expect![[r#"
            user impl for f1 in lib: 1
            user impl for f2 in lib: 2
            another impl for f3 in lib4
        "#]],
    );
    check(
        get_err_stdout(&user, ["test", "--no-parallelize"])
            .lines()
            .take(10)
            .collect::<Vec<_>>()
            .join("\n")
            + "\n",
        expect![[r#"
            internal test
            default impl for f1 in lib1: 1
            another impl for f3 in lib4
            wb test
            default impl for f1 in lib1: 1
            another impl for f3 in lib4
            bb test
            default impl for f1 in lib1: 1
            another impl for f3 in lib4
            [username/xxx] test lib2/hello_test.mbt:2 (#0) failed: Error
        "#]],
    );

    let err = dir.join("err");
    let content = get_err_stderr(&err, ["check"]);
    assert!(content.contains("$ROOT/lib1/pkg.mbti:5:1"));
    assert!(content.contains("$ROOT/lib1/pkg.mbti:3:1"));

    // moon build will not build default impl for lib1 if no pkg depend on this default impl
    // so here just report error for missing impl for f2(diy impl in lib2), no report error for missing impl for f1(default impl in lib1)
    check(
        get_err_stderr(&err, ["build"]),
        expect![[r#"
            Error: [4159]
               ╭─[ $ROOT/lib1/pkg.mbti:5:1 ]
               │
             5 │ fn f2(String) -> Unit
               │ ──────────┬──────────  
               │           ╰──────────── Missing implementation for function f2.
            ───╯
            Failed with 0 warnings, 1 errors.
            error: failed when building project
        "#]],
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
        check(
            get_stdout(&dir, ["test", &single_mbt, "-i", "1", "-u"]),
            expect![[r#"

                Auto updating expect tests and retesting ...

                ------------------ 22222222 ------------------
                Total tests: 1, passed: 1, failed: 0.
            "#]],
        );

        check(
            get_stderr(&dir, ["check", "single.mbt"]),
            expect![[r#"
                Warning: [0002]
                   ╭─[ $ROOT/single.mbt:8:7 ]
                   │
                 8 │   let single_mbt = 1
                   │       ─────┬────  
                   │            ╰────── Warning: Unused variable 'single_mbt'
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
                   │            ╰────── Warning: Unused variable 'single_mbt'
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
        check(
            get_stdout(&dir, ["test", "111.mbt.md", "-i", "0"]),
            expect![[r#"
                111
                Total tests: 1, passed: 1, failed: 0.
            "#]],
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
        check(
            get_stdout(&dir, ["test", &single_mbt_md, "-i", "1", "-u"]),
            expect![[r#"
    
                Auto updating expect tests and retesting ...
    
                222
                Total tests: 1, passed: 1, failed: 0.
            "#]],
        );

        // rel path
        check(
            get_stderr(&dir, ["check", "111.mbt.md"]),
            expect![[r#"
                Warning: [0002]
                    ╭─[ $ROOT/111.mbt.md:28:9 ]
                    │
                 28 │     let single_mbt_md = 1
                    │         ──────┬──────  
                    │               ╰──────── Warning: Unused variable 'single_mbt_md'
                ────╯
                Finished. moon: ran 20 tasks, now up to date (1 warnings, 0 errors)
            "#]],
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
                    │               ╰──────── Warning: Unused variable 'single_mbt_md'
                ────╯
                Finished. moon: ran 1 task, now up to date (1 warnings, 0 errors)
            "#]],
        );
    }

    // check single file (with or without main func)
    {
        let with_main = dir.join("with_main.mbt").display().to_string();
        check(
            get_stderr(&dir, ["check", &with_main]),
            expect![[r#"
                Warning: [0002]
                   ╭─[ $ROOT/with_main.mbt:2:7 ]
                   │
                 2 │   let with_main = 1
                   │       ────┬────  
                   │           ╰────── Warning: Unused variable 'with_main'
                ───╯
                Finished. moon: ran 1 task, now up to date (1 warnings, 0 errors)
            "#]],
        );
        let without_main = dir.join("without_main.mbt").display().to_string();
        check(
            get_stderr(&dir, ["check", &without_main]),
            expect![[r#"
                Warning: [0001]
                   ╭─[ $ROOT/without_main.mbt:1:4 ]
                   │
                 1 │ fn func() -> Unit {
                   │    ──┬─  
                   │      ╰─── Warning: Unused function 'func'
                ───╯
                Warning: [0002]
                   ╭─[ $ROOT/without_main.mbt:2:7 ]
                   │
                 2 │   let without_main = 1
                   │       ──────┬─────  
                   │             ╰─────── Warning: Unused variable 'without_main'
                ───╯
                Finished. moon: ran 1 task, now up to date (2 warnings, 0 errors)
            "#]],
        );
    }
}

#[test]
fn test_sub_package() {
    let dir = TestDir::new("test_sub_package.in");

    check(
        get_stdout(&dir, ["test", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./dep/hello.mbt -o ./target/wasm-gc/debug/test/dep/dep.core -pkg moon_new/dep -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moon_new/dep:./dep -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package ./sub_pkg/111.mbt ./sub_pkg/dir/222.mbt -o ./target/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.core -pkg moon_new/sub_pkg -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/dep/dep.mi:dep -pkg-sources moon_new/sub_pkg:./sub_pkg -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package ./dep2/hello.mbt -o ./target/wasm-gc/debug/test/dep2/dep2.core -pkg moon_new/dep2 -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/dep2:./dep2 -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package ./sub_pkg/111.mbt ./sub_pkg/hello.mbt -o ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.core -pkg moon_new/sub_pkg -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/dep2/dep2.mi:dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -target wasm-gc -g -O0 -source-map -workspace-path .
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/test/__generated_driver_for_whitebox_test.mbt --output-metadata ./target/wasm-gc/debug/test/test/__whitebox_test_info.json ./test/hello_wbtest.mbt --target wasm-gc --pkg-name moon_new/test --driver-kind whitebox
            moonc build-package ./test/hello.mbt ./test/hello_wbtest.mbt ./target/wasm-gc/debug/test/test/__generated_driver_for_whitebox_test.mbt -o ./target/wasm-gc/debug/test/test/test.whitebox_test.core -pkg moon_new/test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.mi:sub_pkg -pkg-sources moon_new/test:./test -target wasm-gc -g -O0 -source-map -whitebox-test -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/dep2/dep2.core ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.core ./target/wasm-gc/debug/test/test/test.whitebox_test.core -main moon_new/test -o ./target/wasm-gc/debug/test/test/test.whitebox_test.wasm -test-mode -pkg-config-path ./test/moon.pkg.json -pkg-sources moon_new/dep2:./dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources moon_new/test:./test -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/test/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/test/__internal_test_info.json ./test/hello.mbt --target wasm-gc --pkg-name moon_new/test --driver-kind internal
            moonc build-package ./test/hello.mbt ./target/wasm-gc/debug/test/test/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/test/test.internal_test.core -pkg moon_new/test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/test:./test -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/dep2/dep2.core ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.core ./target/wasm-gc/debug/test/test/test.internal_test.core -main moon_new/test -o ./target/wasm-gc/debug/test/test/test.internal_test.wasm -test-mode -pkg-config-path ./test/moon.pkg.json -pkg-sources moon_new/dep2:./dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources moon_new/test:./test -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moonc build-package ./test/hello.mbt -o ./target/wasm-gc/debug/test/test/test.core -pkg moon_new/test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/test:./test -target wasm-gc -g -O0 -source-map -workspace-path .
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/test/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/test/__blackbox_test_info.json ./test/hello_test.mbt --doctest-only ./test/hello.mbt --target wasm-gc --pkg-name moon_new/test --driver-kind blackbox
            moonc build-package ./test/hello_test.mbt ./target/wasm-gc/debug/test/test/__generated_driver_for_blackbox_test.mbt -doctest-only ./test/hello.mbt -o ./target/wasm-gc/debug/test/test/test.blackbox_test.core -pkg moon_new/test_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/test/test.mi:test -i ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.mi:sub_pkg -pkg-sources moon_new/test_blackbox_test:./test -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/dep2/dep2.core ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.core ./target/wasm-gc/debug/test/test/test.core ./target/wasm-gc/debug/test/test/test.blackbox_test.core -main moon_new/test_blackbox_test -o ./target/wasm-gc/debug/test/test/test.blackbox_test.wasm -test-mode -pkg-config-path ./test/moon.pkg.json -pkg-sources moon_new/dep2:./dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources moon_new/test:./test -pkg-sources moon_new/test_blackbox_test:./test -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/sub_pkg_sub/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/sub_pkg_sub/__internal_test_info.json ./sub_pkg/111.mbt ./sub_pkg/dir/222.mbt --target wasm-gc --pkg-name moon_new/sub_pkg_sub --driver-kind internal
            moonc build-package ./sub_pkg/111.mbt ./sub_pkg/dir/222.mbt ./target/wasm-gc/debug/test/sub_pkg_sub/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.internal_test.core -pkg moon_new/sub_pkg -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/dep/dep.mi:dep -pkg-sources moon_new/sub_pkg:./sub_pkg -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/dep/dep.core ./target/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.internal_test.core -main moon_new/sub_pkg -o ./target/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.internal_test.wasm -test-mode -pkg-config-path ./sub_pkg/moon.pkg.json -pkg-sources moon_new/dep:./dep -pkg-sources moon_new/sub_pkg_sub:./sub_pkg -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/sub_pkg_sub/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/sub_pkg_sub/__blackbox_test_info.json --doctest-only ./sub_pkg/111.mbt --doctest-only ./sub_pkg/dir/222.mbt --target wasm-gc --pkg-name moon_new/sub_pkg_sub --driver-kind blackbox
            moonc build-package ./target/wasm-gc/debug/test/sub_pkg_sub/__generated_driver_for_blackbox_test.mbt -doctest-only ./sub_pkg/111.mbt -doctest-only ./sub_pkg/dir/222.mbt -o ./target/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.blackbox_test.core -pkg moon_new/sub_pkg_sub_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.mi:sub_pkg_sub -i ./target/wasm-gc/debug/test/dep/dep.mi:dep -pkg-sources moon_new/sub_pkg_sub_blackbox_test:./sub_pkg -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/dep/dep.core ./target/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.core ./target/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.blackbox_test.core -main moon_new/sub_pkg_blackbox_test -o ./target/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.blackbox_test.wasm -test-mode -pkg-config-path ./sub_pkg/moon.pkg.json -pkg-sources moon_new/dep:./dep -pkg-sources moon_new/sub_pkg_sub:./sub_pkg -pkg-sources moon_new/sub_pkg_sub_blackbox_test:./sub_pkg -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/sub_pkg/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/sub_pkg/__internal_test_info.json ./sub_pkg/111.mbt ./sub_pkg/hello.mbt --target wasm-gc --pkg-name moon_new/sub_pkg --driver-kind internal
            moonc build-package ./sub_pkg/111.mbt ./sub_pkg/hello.mbt ./target/wasm-gc/debug/test/sub_pkg/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.internal_test.core -pkg moon_new/sub_pkg -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/dep2/dep2.mi:dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/dep2/dep2.core ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.internal_test.core -main moon_new/sub_pkg -o ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.internal_test.wasm -test-mode -pkg-config-path ./sub_pkg/moon.pkg.json -pkg-sources moon_new/dep2:./dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/sub_pkg/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/sub_pkg/__blackbox_test_info.json --doctest-only ./sub_pkg/111.mbt --doctest-only ./sub_pkg/hello.mbt --target wasm-gc --pkg-name moon_new/sub_pkg --driver-kind blackbox
            moonc build-package ./target/wasm-gc/debug/test/sub_pkg/__generated_driver_for_blackbox_test.mbt -doctest-only ./sub_pkg/111.mbt -doctest-only ./sub_pkg/hello.mbt -o ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.blackbox_test.core -pkg moon_new/sub_pkg_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.mi:sub_pkg -i ./target/wasm-gc/debug/test/dep2/dep2.mi:dep2 -pkg-sources moon_new/sub_pkg_blackbox_test:./sub_pkg -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/dep2/dep2.core ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.core ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.blackbox_test.core -main moon_new/sub_pkg_blackbox_test -o ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.blackbox_test.wasm -test-mode -pkg-config-path ./sub_pkg/moon.pkg.json -pkg-sources moon_new/dep2:./dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources moon_new/sub_pkg_blackbox_test:./sub_pkg -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/main/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/main/__internal_test_info.json ./main/main.mbt --target wasm-gc --pkg-name moon_new/main --driver-kind internal
            moonc build-package ./main/main.mbt ./target/wasm-gc/debug/test/main/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/main/main.internal_test.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/main:./main -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/dep2/dep2.core ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.core ./target/wasm-gc/debug/test/main/main.internal_test.core -main moon_new/main -o ./target/wasm-gc/debug/test/main/main.internal_test.wasm -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/dep2:./dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/test/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/main:./main -target wasm-gc -g -O0 -source-map -workspace-path .
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/main/__blackbox_test_info.json --doctest-only ./main/main.mbt --target wasm-gc --pkg-name moon_new/main --driver-kind blackbox
            moonc build-package ./target/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt -doctest-only ./main/main.mbt -o ./target/wasm-gc/debug/test/main/main.blackbox_test.core -pkg moon_new/main_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/main/main.mi:main -i ./target/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/main_blackbox_test:./main -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/dep2/dep2.core ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.core ./target/wasm-gc/debug/test/main/main.core ./target/wasm-gc/debug/test/main/main.blackbox_test.core -main moon_new/main_blackbox_test -o ./target/wasm-gc/debug/test/main/main.blackbox_test.wasm -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/dep2:./dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources moon_new/main:./main -pkg-sources moon_new/main_blackbox_test:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__internal_test_info.json ./lib/hello.mbt --target wasm-gc --pkg-name moon_new/lib --driver-kind internal
            moonc build-package ./lib/hello.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.internal_test.core -pkg moon_new/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.mi:sub_pkg -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/dep2/dep2.core ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.core ./target/wasm-gc/debug/test/lib/lib.internal_test.core -main moon_new/lib -o ./target/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/dep2:./dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/test/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.mi:sub_pkg -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -source-map -workspace-path .
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target wasm-gc --pkg-name moon_new/lib --driver-kind blackbox
            moonc build-package ./target/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -pkg moon_new/lib_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -i ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.mi:sub_pkg -pkg-sources moon_new/lib_blackbox_test:./lib -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/dep2/dep2.core ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.core ./target/wasm-gc/debug/test/lib/lib.core ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -main moon_new/lib_blackbox_test -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/dep2:./dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/lib_blackbox_test:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/dep2/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/dep2/__internal_test_info.json ./dep2/hello.mbt --target wasm-gc --pkg-name moon_new/dep2 --driver-kind internal
            moonc build-package ./dep2/hello.mbt ./target/wasm-gc/debug/test/dep2/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/dep2/dep2.internal_test.core -pkg moon_new/dep2 -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/dep2:./dep2 -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.core ./target/wasm-gc/debug/test/dep2/dep2.internal_test.core -main moon_new/dep2 -o ./target/wasm-gc/debug/test/dep2/dep2.internal_test.wasm -test-mode -pkg-config-path ./dep2/moon.pkg.json -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources moon_new/dep2:./dep2 -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/dep2/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/dep2/__blackbox_test_info.json --doctest-only ./dep2/hello.mbt --target wasm-gc --pkg-name moon_new/dep2 --driver-kind blackbox
            moonc build-package ./target/wasm-gc/debug/test/dep2/__generated_driver_for_blackbox_test.mbt -doctest-only ./dep2/hello.mbt -o ./target/wasm-gc/debug/test/dep2/dep2.blackbox_test.core -pkg moon_new/dep2_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/dep2/dep2.mi:dep2 -i ./target/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/dep2_blackbox_test:./dep2 -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/sub_pkg/sub_pkg.core ./target/wasm-gc/debug/test/dep2/dep2.core ./target/wasm-gc/debug/test/dep2/dep2.blackbox_test.core -main moon_new/dep2_blackbox_test -o ./target/wasm-gc/debug/test/dep2/dep2.blackbox_test.wasm -test-mode -pkg-config-path ./dep2/moon.pkg.json -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources moon_new/dep2:./dep2 -pkg-sources moon_new/dep2_blackbox_test:./dep2 -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/dep/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/dep/__internal_test_info.json ./dep/hello.mbt --target wasm-gc --pkg-name moon_new/dep --driver-kind internal
            moonc build-package ./dep/hello.mbt ./target/wasm-gc/debug/test/dep/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/dep/dep.internal_test.core -pkg moon_new/dep -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moon_new/dep:./dep -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/dep/dep.internal_test.core -main moon_new/dep -o ./target/wasm-gc/debug/test/dep/dep.internal_test.wasm -test-mode -pkg-config-path ./dep/moon.pkg.json -pkg-sources moon_new/dep:./dep -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/dep/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/dep/__blackbox_test_info.json --doctest-only ./dep/hello.mbt --target wasm-gc --pkg-name moon_new/dep --driver-kind blackbox
            moonc build-package ./target/wasm-gc/debug/test/dep/__generated_driver_for_blackbox_test.mbt -doctest-only ./dep/hello.mbt -o ./target/wasm-gc/debug/test/dep/dep.blackbox_test.core -pkg moon_new/dep_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/dep/dep.mi:dep -pkg-sources moon_new/dep_blackbox_test:./dep -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/dep/dep.core ./target/wasm-gc/debug/test/dep/dep.blackbox_test.core -main moon_new/dep_blackbox_test -o ./target/wasm-gc/debug/test/dep/dep.blackbox_test.wasm -test-mode -pkg-config-path ./dep/moon.pkg.json -pkg-sources moon_new/dep:./dep -pkg-sources moon_new/dep_blackbox_test:./dep -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
        "#]],
    );
    check(
        get_stdout(&dir, ["check", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc check ./dep/hello.mbt -o ./target/wasm-gc/release/check/dep/dep.mi -pkg moon_new/dep -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moon_new/dep:./dep -target wasm-gc -workspace-path .
            moonc check ./sub_pkg/111.mbt ./sub_pkg/dir/222.mbt -o ./target/wasm-gc/release/check/sub_pkg/sub_pkg_sub.mi -pkg moon_new/sub_pkg -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/dep/dep.mi:dep -pkg-sources moon_new/sub_pkg:./sub_pkg -target wasm-gc -workspace-path .
            moonc check ./dep2/hello.mbt -o ./target/wasm-gc/release/check/dep2/dep2.mi -pkg moon_new/dep2 -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/dep2:./dep2 -target wasm-gc -workspace-path .
            moonc check ./sub_pkg/111.mbt ./sub_pkg/hello.mbt -o ./target/wasm-gc/release/check/sub_pkg/sub_pkg.mi -pkg moon_new/sub_pkg -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/dep2/dep2.mi:dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -target wasm-gc -workspace-path .
            moonc check ./test/hello.mbt ./test/hello_wbtest.mbt -o ./target/wasm-gc/release/check/test/test.whitebox_test.mi -pkg moon_new/test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/sub_pkg/sub_pkg.mi:sub_pkg -pkg-sources moon_new/test:./test -target wasm-gc -whitebox-test -workspace-path .
            moonc check ./test/hello.mbt -o ./target/wasm-gc/release/check/test/test.mi -pkg moon_new/test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/test:./test -target wasm-gc -workspace-path .
            moonc check ./test/hello_test.mbt -doctest-only ./test/hello.mbt -include-doctests -o ./target/wasm-gc/release/check/test/test.blackbox_test.mi -pkg moon_new/test_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/test/test.mi:test -i ./target/wasm-gc/release/check/sub_pkg/sub_pkg.mi:sub_pkg -pkg-sources moon_new/test_blackbox_test:./test -target wasm-gc -blackbox-test -workspace-path .
            moonc check -doctest-only ./sub_pkg/111.mbt -doctest-only ./sub_pkg/dir/222.mbt -include-doctests -o ./target/wasm-gc/release/check/sub_pkg/sub_pkg_sub.blackbox_test.mi -pkg moon_new/sub_pkg_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/sub_pkg/sub_pkg_sub.mi:sub_pkg_sub -i ./target/wasm-gc/release/check/dep/dep.mi:dep -pkg-sources moon_new/sub_pkg_blackbox_test:./sub_pkg -target wasm-gc -blackbox-test -workspace-path .
            moonc check -doctest-only ./sub_pkg/111.mbt -doctest-only ./sub_pkg/hello.mbt -include-doctests -o ./target/wasm-gc/release/check/sub_pkg/sub_pkg.blackbox_test.mi -pkg moon_new/sub_pkg_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/sub_pkg/sub_pkg.mi:sub_pkg -i ./target/wasm-gc/release/check/dep2/dep2.mi:dep2 -pkg-sources moon_new/sub_pkg_blackbox_test:./sub_pkg -target wasm-gc -blackbox-test -workspace-path .
            moonc check ./main/main.mbt -o ./target/wasm-gc/release/check/main/main.mi -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/main:./main -target wasm-gc -workspace-path .
            moonc check -doctest-only ./main/main.mbt -include-doctests -o ./target/wasm-gc/release/check/main/main.blackbox_test.mi -pkg moon_new/main_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/main/main.mi:main -i ./target/wasm-gc/release/check/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/main_blackbox_test:./main -target wasm-gc -blackbox-test -workspace-path .
            moonc check ./lib/hello.mbt -o ./target/wasm-gc/release/check/lib/lib.mi -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/sub_pkg/sub_pkg.mi:sub_pkg -pkg-sources moon_new/lib:./lib -target wasm-gc -workspace-path .
            moonc check -doctest-only ./lib/hello.mbt -include-doctests -o ./target/wasm-gc/release/check/lib/lib.blackbox_test.mi -pkg moon_new/lib_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib/lib.mi:lib -i ./target/wasm-gc/release/check/sub_pkg/sub_pkg.mi:sub_pkg -pkg-sources moon_new/lib_blackbox_test:./lib -target wasm-gc -blackbox-test -workspace-path .
            moonc check -doctest-only ./dep2/hello.mbt -include-doctests -o ./target/wasm-gc/release/check/dep2/dep2.blackbox_test.mi -pkg moon_new/dep2_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/dep2/dep2.mi:dep2 -i ./target/wasm-gc/release/check/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/dep2_blackbox_test:./dep2 -target wasm-gc -blackbox-test -workspace-path .
            moonc check -doctest-only ./dep/hello.mbt -include-doctests -o ./target/wasm-gc/release/check/dep/dep.blackbox_test.mi -pkg moon_new/dep_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/dep/dep.mi:dep -pkg-sources moon_new/dep_blackbox_test:./dep -target wasm-gc -blackbox-test -workspace-path .
        "#]],
    );
    check(
        get_stdout(&dir, ["build", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./dep/hello.mbt -o ./target/wasm-gc/release/build/dep/dep.core -pkg moon_new/dep -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moon_new/dep:./dep -target wasm-gc -workspace-path .
            moonc build-package ./sub_pkg/111.mbt ./sub_pkg/dir/222.mbt -o ./target/wasm-gc/release/build/sub_pkg/sub_pkg_sub.core -pkg moon_new/sub_pkg -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/dep/dep.mi:dep -pkg-sources moon_new/sub_pkg:./sub_pkg -target wasm-gc -workspace-path .
            moonc build-package ./dep2/hello.mbt -o ./target/wasm-gc/release/build/dep2/dep2.core -pkg moon_new/dep2 -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/dep2:./dep2 -target wasm-gc -workspace-path .
            moonc build-package ./sub_pkg/111.mbt ./sub_pkg/hello.mbt -o ./target/wasm-gc/release/build/sub_pkg/sub_pkg.core -pkg moon_new/sub_pkg -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/dep2/dep2.mi:dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -target wasm-gc -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/main:./main -target wasm-gc -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/dep2/dep2.core ./target/wasm-gc/release/build/sub_pkg/sub_pkg.core ./target/wasm-gc/release/build/main/main.core -main moon_new/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/dep2:./dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc
        "#]],
    );
    check(
        get_stdout(&dir, ["run", "main", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./dep/hello.mbt -o ./target/wasm-gc/release/build/dep/dep.core -pkg moon_new/dep -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moon_new/dep:./dep -target wasm-gc -workspace-path .
            moonc build-package ./sub_pkg/111.mbt ./sub_pkg/dir/222.mbt -o ./target/wasm-gc/release/build/sub_pkg/sub_pkg_sub.core -pkg moon_new/sub_pkg -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/dep/dep.mi:dep -pkg-sources moon_new/sub_pkg:./sub_pkg -target wasm-gc -workspace-path .
            moonc build-package ./dep2/hello.mbt -o ./target/wasm-gc/release/build/dep2/dep2.core -pkg moon_new/dep2 -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/dep2:./dep2 -target wasm-gc -workspace-path .
            moonc build-package ./sub_pkg/111.mbt ./sub_pkg/hello.mbt -o ./target/wasm-gc/release/build/sub_pkg/sub_pkg.core -pkg moon_new/sub_pkg -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/dep2/dep2.mi:dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -target wasm-gc -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/main:./main -target wasm-gc -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/dep2/dep2.core ./target/wasm-gc/release/build/sub_pkg/sub_pkg.core ./target/wasm-gc/release/build/main/main.core -main moon_new/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/dep2:./dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc
            moonrun ./target/wasm-gc/release/build/main/main.wasm
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
               │       ╰── Warning: Unused variable 'a'
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

    check(
        get_stderr(&dir, ["check"]),
        expect![[r#"
            Warning: [0002]
               ╭─[ $ROOT/lib/README.mbt.md:5:9 ]
               │
             5 │     let unused_in_lib_md_test = 1
               │         ──────────┬──────────  
               │                   ╰──────────── Warning: Unused variable 'unused_in_lib_md_test'
            ───╯
            Warning: [0002]
               ╭─[ $ROOT/lib/hello.mbt:3:11 ]
               │
             3 │ ///   let unused_in_lib_doc_test = 1
               │           ───────────┬──────────  
               │                      ╰──────────── Warning: Unused variable 'unused_in_lib_doc_test'
            ───╯
            Finished. moon: ran 4 tasks, now up to date (2 warnings, 0 errors)
        "#]],
    );

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
                at $panic ($ROOT/target/js/debug/test/lib/lib.blackbox_test.js:3:9)
                at username$hello$lib_blackbox_test$$__test_68656c6c6f5f746573742e6d6274_0 ($ROOT/src/lib/hello_test.mbt:3:5)
                at username$hello$lib_blackbox_test$$moonbit_test_driver_internal_do_execute ($ROOT/src/lib/__generated_driver_for_blackbox_test.mbt:177:15)
            Total tests: 1, passed: 0, failed: 1."#]],
    );
}
