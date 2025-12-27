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

use moonbuild_debug::graph::ENV_VAR;

use crate::build_graph::compare_graphs;

use super::*;

#[test]
fn test_moon_coverage_analyze() {
    let dir = TestDir::new("test_coverage.in");
    check(
        get_stdout(
            &dir,
            [
                "coverage",
                "analyze",
                "--test-flag=--sort-input",
                "--",
                "-f=caret",
            ],
        ),
        expect![[r#"
            warning: this line has no test coverage
             --> lib2/hello.mbt:2
            1 | fn hello_uncovered_1() -> String {
            2 |   "Hello, world!"
              |   ^^^^^^^^^^^^^^^
            3 | }
            4 | 
            5 | fn hello_uncovered_2() -> String {


            warning: this line has no test coverage
             --> lib2/hello.mbt:6
            4 | 
            5 | fn hello_uncovered_2() -> String {
            6 |   "Hello, world!"
              |   ^^^^^^^^^^^^^^^
            7 | }
            8 | 


            warning: this line has no test coverage
             --> lib2/hello.mbt:10
             7 | }
             8 | 
             9 | fn hello_uncovered_3() -> String {
            10 |   "Hello, world!"
               |   ^^^^^^^^^^^^^^^
            11 | }


            warning: this line has no test coverage
             --> main/main.mbt:2
            1 | fn main {
            2 |   println("main")
              |   ^^^^^^^^^^^^^^^
            3 | }


        "#]],
    );
}

#[test]
fn test_moon_coverage_analyze_dry_run() {
    let dir = TestDir::new("test_coverage.in");
    let dump_file = dir.join("coverage_analyze_dry_run.jsonl");
    let _stdout = get_stdout_with_envs(
        &dir,
        [
            "coverage",
            "analyze",
            "--dry-run",
            "--test-flag=--nostd",
            "--test-flag=--sort-input",
        ],
        [(ENV_VAR, dump_file.to_str().unwrap())],
    );

    // The expect part is just a dump, it is not compared line-by-line
    compare_graphs(
        &dump_file,
        expect![[r#"
            {"command":"moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./_build/wasm-gc/debug/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target wasm-gc --pkg-name username/hello/lib --enable-coverage --driver-kind blackbox","inputs":["./lib/hello.mbt"],"outputs":["./_build/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt","./_build/wasm-gc/debug/test/lib/__blackbox_test_info.json"]}
            {"command":"moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./_build/wasm-gc/debug/test/lib/__internal_test_info.json ./lib/hello.mbt --target wasm-gc --pkg-name username/hello/lib --enable-coverage --driver-kind internal","inputs":["./lib/hello.mbt"],"outputs":["./_build/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt","./_build/wasm-gc/debug/test/lib/__internal_test_info.json"]}
            {"command":"moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/lib/__generated_driver_for_whitebox_test.mbt --output-metadata ./_build/wasm-gc/debug/test/lib/__whitebox_test_info.json ./lib/hello_wbtest.mbt --target wasm-gc --pkg-name username/hello/lib --enable-coverage --driver-kind whitebox","inputs":["./lib/hello_wbtest.mbt"],"outputs":["./_build/wasm-gc/debug/test/lib/__generated_driver_for_whitebox_test.mbt","./_build/wasm-gc/debug/test/lib/__whitebox_test_info.json"]}
            {"command":"moonc build-package ./_build/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./_build/wasm-gc/debug/test/lib/lib.blackbox_test.core -pkg username/hello/lib_blackbox_test -is-main -i ./_build/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources username/hello/lib_blackbox_test:./lib -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path . -all-pkgs ./_build/wasm-gc/debug/test/all_pkgs.json","inputs":["./_build/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt","./_build/wasm-gc/debug/test/lib/lib.mi","./lib/hello.mbt","./lib/moon.pkg.json"],"outputs":["./_build/wasm-gc/debug/test/lib/lib.blackbox_test.core"]}
            {"command":"moonc link-core ./_build/wasm-gc/debug/test/lib/lib.core ./_build/wasm-gc/debug/test/lib/lib.blackbox_test.core -main username/hello/lib_blackbox_test -o ./_build/wasm-gc/debug/test/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/lib_blackbox_test:./lib -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map","inputs":["./_build/wasm-gc/debug/test/lib/lib.blackbox_test.core","./_build/wasm-gc/debug/test/lib/lib.core","./lib/moon.pkg.json"],"outputs":["./_build/wasm-gc/debug/test/lib/lib.blackbox_test.wasm"]}
            {"command":"moonc build-package ./lib/hello.mbt ./_build/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./_build/wasm-gc/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -pkg-sources username/hello/lib:./lib -target wasm-gc -g -O0 -source-map -enable-coverage -no-mi -test-mode -workspace-path . -all-pkgs ./_build/wasm-gc/debug/test/all_pkgs.json","inputs":["./_build/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt","./lib/hello.mbt","./lib/moon.pkg.json"],"outputs":["./_build/wasm-gc/debug/test/lib/lib.internal_test.core"]}
            {"command":"moonc link-core ./_build/wasm-gc/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./_build/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map","inputs":["./_build/wasm-gc/debug/test/lib/lib.internal_test.core","./lib/moon.pkg.json"],"outputs":["./_build/wasm-gc/debug/test/lib/lib.internal_test.wasm"]}
            {"command":"moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/debug/test/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target wasm-gc -g -O0 -source-map -enable-coverage -workspace-path . -all-pkgs ./_build/wasm-gc/debug/test/all_pkgs.json","inputs":["./lib/hello.mbt","./lib/moon.pkg.json"],"outputs":["./_build/wasm-gc/debug/test/lib/lib.mi","./_build/wasm-gc/debug/test/lib/lib.core"]}
            {"command":"moonc build-package ./lib/hello.mbt ./lib/hello_wbtest.mbt ./_build/wasm-gc/debug/test/lib/__generated_driver_for_whitebox_test.mbt -o ./_build/wasm-gc/debug/test/lib/lib.whitebox_test.core -pkg username/hello/lib -is-main -pkg-sources username/hello/lib:./lib -target wasm-gc -g -O0 -source-map -enable-coverage -whitebox-test -no-mi -test-mode -workspace-path . -all-pkgs ./_build/wasm-gc/debug/test/all_pkgs.json","inputs":["./_build/wasm-gc/debug/test/lib/__generated_driver_for_whitebox_test.mbt","./lib/hello.mbt","./lib/hello_wbtest.mbt","./lib/moon.pkg.json"],"outputs":["./_build/wasm-gc/debug/test/lib/lib.whitebox_test.core"]}
            {"command":"moonc link-core ./_build/wasm-gc/debug/test/lib/lib.whitebox_test.core -main username/hello/lib -o ./_build/wasm-gc/debug/test/lib/lib.whitebox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map","inputs":["./_build/wasm-gc/debug/test/lib/lib.whitebox_test.core","./lib/moon.pkg.json"],"outputs":["./_build/wasm-gc/debug/test/lib/lib.whitebox_test.wasm"]}
            {"command":"moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/lib2/__generated_driver_for_blackbox_test.mbt --output-metadata ./_build/wasm-gc/debug/test/lib2/__blackbox_test_info.json ./lib2/hello_test.mbt --doctest-only ./lib2/hello.mbt --target wasm-gc --pkg-name username/hello/lib2 --enable-coverage --driver-kind blackbox","inputs":["./lib2/hello.mbt","./lib2/hello_test.mbt"],"outputs":["./_build/wasm-gc/debug/test/lib2/__generated_driver_for_blackbox_test.mbt","./_build/wasm-gc/debug/test/lib2/__blackbox_test_info.json"]}
            {"command":"moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/lib2/__generated_driver_for_internal_test.mbt --output-metadata ./_build/wasm-gc/debug/test/lib2/__internal_test_info.json ./lib2/hello.mbt --target wasm-gc --pkg-name username/hello/lib2 --enable-coverage --driver-kind internal","inputs":["./lib2/hello.mbt"],"outputs":["./_build/wasm-gc/debug/test/lib2/__generated_driver_for_internal_test.mbt","./_build/wasm-gc/debug/test/lib2/__internal_test_info.json"]}
            {"command":"moonc build-package ./lib2/hello_test.mbt ./_build/wasm-gc/debug/test/lib2/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib2/hello.mbt -o ./_build/wasm-gc/debug/test/lib2/lib2.blackbox_test.core -pkg username/hello/lib2_blackbox_test -is-main -i ./_build/wasm-gc/debug/test/lib2/lib2.mi:lib2 -pkg-sources username/hello/lib2_blackbox_test:./lib2 -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path . -all-pkgs ./_build/wasm-gc/debug/test/all_pkgs.json","inputs":["./_build/wasm-gc/debug/test/lib2/__generated_driver_for_blackbox_test.mbt","./_build/wasm-gc/debug/test/lib2/lib2.mi","./lib2/hello.mbt","./lib2/hello_test.mbt","./lib2/moon.pkg.json"],"outputs":["./_build/wasm-gc/debug/test/lib2/lib2.blackbox_test.core"]}
            {"command":"moonc link-core ./_build/wasm-gc/debug/test/lib2/lib2.core ./_build/wasm-gc/debug/test/lib2/lib2.blackbox_test.core -main username/hello/lib2_blackbox_test -o ./_build/wasm-gc/debug/test/lib2/lib2.blackbox_test.wasm -test-mode -pkg-config-path ./lib2/moon.pkg.json -pkg-sources username/hello/lib2:./lib2 -pkg-sources username/hello/lib2_blackbox_test:./lib2 -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map","inputs":["./_build/wasm-gc/debug/test/lib2/lib2.blackbox_test.core","./_build/wasm-gc/debug/test/lib2/lib2.core","./lib2/moon.pkg.json"],"outputs":["./_build/wasm-gc/debug/test/lib2/lib2.blackbox_test.wasm"]}
            {"command":"moonc build-package ./lib2/hello.mbt ./_build/wasm-gc/debug/test/lib2/__generated_driver_for_internal_test.mbt -o ./_build/wasm-gc/debug/test/lib2/lib2.internal_test.core -pkg username/hello/lib2 -is-main -pkg-sources username/hello/lib2:./lib2 -target wasm-gc -g -O0 -source-map -enable-coverage -no-mi -test-mode -workspace-path . -all-pkgs ./_build/wasm-gc/debug/test/all_pkgs.json","inputs":["./_build/wasm-gc/debug/test/lib2/__generated_driver_for_internal_test.mbt","./lib2/hello.mbt","./lib2/moon.pkg.json"],"outputs":["./_build/wasm-gc/debug/test/lib2/lib2.internal_test.core"]}
            {"command":"moonc link-core ./_build/wasm-gc/debug/test/lib2/lib2.internal_test.core -main username/hello/lib2 -o ./_build/wasm-gc/debug/test/lib2/lib2.internal_test.wasm -test-mode -pkg-config-path ./lib2/moon.pkg.json -pkg-sources username/hello/lib2:./lib2 -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map","inputs":["./_build/wasm-gc/debug/test/lib2/lib2.internal_test.core","./lib2/moon.pkg.json"],"outputs":["./_build/wasm-gc/debug/test/lib2/lib2.internal_test.wasm"]}
            {"command":"moonc build-package ./lib2/hello.mbt -o ./_build/wasm-gc/debug/test/lib2/lib2.core -pkg username/hello/lib2 -pkg-sources username/hello/lib2:./lib2 -target wasm-gc -g -O0 -source-map -enable-coverage -workspace-path . -all-pkgs ./_build/wasm-gc/debug/test/all_pkgs.json","inputs":["./lib2/hello.mbt","./lib2/moon.pkg.json"],"outputs":["./_build/wasm-gc/debug/test/lib2/lib2.mi","./_build/wasm-gc/debug/test/lib2/lib2.core"]}
            {"command":"moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt --output-metadata ./_build/wasm-gc/debug/test/main/__blackbox_test_info.json --doctest-only ./main/main.mbt --target wasm-gc --pkg-name username/hello/main --enable-coverage --driver-kind blackbox","inputs":["./main/main.mbt"],"outputs":["./_build/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt","./_build/wasm-gc/debug/test/main/__blackbox_test_info.json"]}
            {"command":"moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/main/__generated_driver_for_internal_test.mbt --output-metadata ./_build/wasm-gc/debug/test/main/__internal_test_info.json ./main/main.mbt --target wasm-gc --pkg-name username/hello/main --enable-coverage --driver-kind internal","inputs":["./main/main.mbt"],"outputs":["./_build/wasm-gc/debug/test/main/__generated_driver_for_internal_test.mbt","./_build/wasm-gc/debug/test/main/__internal_test_info.json"]}
            {"command":"moonc build-package ./_build/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt -doctest-only ./main/main.mbt -o ./_build/wasm-gc/debug/test/main/main.blackbox_test.core -pkg username/hello/main_blackbox_test -is-main -i ./_build/wasm-gc/debug/test/lib/lib.mi:lib -i ./_build/wasm-gc/debug/test/lib2/lib2.mi:lib2 -i ./_build/wasm-gc/debug/test/main/main.mi:main -pkg-sources username/hello/main_blackbox_test:./main -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path . -all-pkgs ./_build/wasm-gc/debug/test/all_pkgs.json","inputs":["./_build/wasm-gc/debug/test/lib/lib.mi","./_build/wasm-gc/debug/test/lib2/lib2.mi","./_build/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt","./_build/wasm-gc/debug/test/main/main.mi","./main/main.mbt","./main/moon.pkg.json"],"outputs":["./_build/wasm-gc/debug/test/main/main.blackbox_test.core"]}
            {"command":"moonc link-core ./_build/wasm-gc/debug/test/lib/lib.core ./_build/wasm-gc/debug/test/lib2/lib2.core ./_build/wasm-gc/debug/test/main/main.core ./_build/wasm-gc/debug/test/main/main.blackbox_test.core -main username/hello/main_blackbox_test -o ./_build/wasm-gc/debug/test/main/main.blackbox_test.wasm -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/lib2:./lib2 -pkg-sources username/hello/main:./main -pkg-sources username/hello/main_blackbox_test:./main -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map","inputs":["./_build/wasm-gc/debug/test/lib/lib.core","./_build/wasm-gc/debug/test/lib2/lib2.core","./_build/wasm-gc/debug/test/main/main.blackbox_test.core","./_build/wasm-gc/debug/test/main/main.core","./main/moon.pkg.json"],"outputs":["./_build/wasm-gc/debug/test/main/main.blackbox_test.wasm"]}
            {"command":"moonc build-package ./main/main.mbt ./_build/wasm-gc/debug/test/main/__generated_driver_for_internal_test.mbt -o ./_build/wasm-gc/debug/test/main/main.internal_test.core -pkg username/hello/main -is-main -i ./_build/wasm-gc/debug/test/lib/lib.mi:lib -i ./_build/wasm-gc/debug/test/lib2/lib2.mi:lib2 -pkg-sources username/hello/main:./main -target wasm-gc -g -O0 -source-map -enable-coverage -no-mi -test-mode -workspace-path . -all-pkgs ./_build/wasm-gc/debug/test/all_pkgs.json","inputs":["./_build/wasm-gc/debug/test/lib/lib.mi","./_build/wasm-gc/debug/test/lib2/lib2.mi","./_build/wasm-gc/debug/test/main/__generated_driver_for_internal_test.mbt","./main/main.mbt","./main/moon.pkg.json"],"outputs":["./_build/wasm-gc/debug/test/main/main.internal_test.core"]}
            {"command":"moonc link-core ./_build/wasm-gc/debug/test/lib/lib.core ./_build/wasm-gc/debug/test/lib2/lib2.core ./_build/wasm-gc/debug/test/main/main.internal_test.core -main username/hello/main -o ./_build/wasm-gc/debug/test/main/main.internal_test.wasm -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/lib2:./lib2 -pkg-sources username/hello/main:./main -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map","inputs":["./_build/wasm-gc/debug/test/lib/lib.core","./_build/wasm-gc/debug/test/lib2/lib2.core","./_build/wasm-gc/debug/test/main/main.internal_test.core","./main/moon.pkg.json"],"outputs":["./_build/wasm-gc/debug/test/main/main.internal_test.wasm"]}
            {"command":"moonc build-package ./main/main.mbt -o ./_build/wasm-gc/debug/test/main/main.core -pkg username/hello/main -is-main -i ./_build/wasm-gc/debug/test/lib/lib.mi:lib -i ./_build/wasm-gc/debug/test/lib2/lib2.mi:lib2 -pkg-sources username/hello/main:./main -target wasm-gc -g -O0 -source-map -enable-coverage -workspace-path . -all-pkgs ./_build/wasm-gc/debug/test/all_pkgs.json","inputs":["./_build/wasm-gc/debug/test/lib/lib.mi","./_build/wasm-gc/debug/test/lib2/lib2.mi","./main/main.mbt","./main/moon.pkg.json"],"outputs":["./_build/wasm-gc/debug/test/main/main.mi","./_build/wasm-gc/debug/test/main/main.core"]}
        "#]],
    );
}
