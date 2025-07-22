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

use super::*;

#[test]
fn test_moon_coverage_analyze() {
    let dir = TestDir::new("test_coverage.in");
    check(
        get_stdout(&dir, ["coverage", "analyze", "--test-flag=--sort-input"]),
        expect![[r#"
            warning: in lib2/hello.mbt
                               | fn hello_uncovered_1() -> String {
            [UNCOVERED line] 2 |   "Hello, world!"
                               | }
                               | 
                               | fn hello_uncovered_2() -> String {


            warning: in lib2/hello.mbt
                               | 
                               | fn hello_uncovered_2() -> String {
            [UNCOVERED line] 6 |   "Hello, world!"
                               | }
                               | 


            warning: in lib2/hello.mbt
                                | }
                                | 
                                | fn hello_uncovered_3() -> String {
            [UNCOVERED line] 10 |   "Hello, world!"
                                | }


            warning: in main/main.mbt
                               | fn main {
            [UNCOVERED line] 2 |   println("main")
                               | }


        "#]],
    );
}

#[test]
fn test_moon_coverage_analyze_dry_run() {
    let dir = TestDir::new("test_coverage.in");
    check(
        get_stdout(
            &dir,
            [
                "coverage",
                "analyze",
                "--dry-run",
                "--test-flag=--nostd",
                "--test-flag=--sort-input",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/test/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target wasm-gc -g -O0 -source-map -enable-coverage
            moonc build-package ./lib2/hello.mbt -o ./target/wasm-gc/debug/test/lib2/lib2.core -pkg username/hello/lib2 -pkg-sources username/hello/lib2:./lib2 -target wasm-gc -g -O0 -source-map -enable-coverage
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/main/__blackbox_test_info.json --doctest-only ./main/main.mbt --target wasm-gc --pkg-name username/hello/main --enable-coverage --driver-kind blackbox
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/test/main/main.core -pkg username/hello/main -is-main -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -i ./target/wasm-gc/debug/test/lib2/lib2.mi:lib2 -pkg-sources username/hello/main:./main -target wasm-gc -g -O0 -source-map -enable-coverage
            moonc build-package ./target/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt -doctest-only ./main/main.mbt -o ./target/wasm-gc/debug/test/main/main.blackbox_test.core -pkg username/hello/main_blackbox_test -is-main -i ./target/wasm-gc/debug/test/main/main.mi:main -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -i ./target/wasm-gc/debug/test/lib2/lib2.mi:lib2 -pkg-sources username/hello/main_blackbox_test:./main -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core ./target/wasm-gc/debug/test/lib/lib.core ./target/wasm-gc/debug/test/lib2/lib2.core ./target/wasm-gc/debug/test/main/main.core ./target/wasm-gc/debug/test/main/main.blackbox_test.core -main username/hello/main_blackbox_test -o ./target/wasm-gc/debug/test/main/main.blackbox_test.wasm -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/lib2:./lib2 -pkg-sources username/hello/main:./main -pkg-sources username/hello/main_blackbox_test:./main -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib2/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib2/__blackbox_test_info.json ./lib2/hello_test.mbt --doctest-only ./lib2/hello.mbt --target wasm-gc --pkg-name username/hello/lib2 --enable-coverage --driver-kind blackbox
            moonc build-package ./lib2/hello_test.mbt ./target/wasm-gc/debug/test/lib2/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib2/hello.mbt -o ./target/wasm-gc/debug/test/lib2/lib2.blackbox_test.core -pkg username/hello/lib2_blackbox_test -is-main -i ./target/wasm-gc/debug/test/lib2/lib2.mi:lib2 -pkg-sources username/hello/lib2_blackbox_test:./lib2 -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core ./target/wasm-gc/debug/test/lib2/lib2.core ./target/wasm-gc/debug/test/lib2/lib2.blackbox_test.core -main username/hello/lib2_blackbox_test -o ./target/wasm-gc/debug/test/lib2/lib2.blackbox_test.wasm -test-mode -pkg-config-path ./lib2/moon.pkg.json -pkg-sources username/hello/lib2:./lib2 -pkg-sources username/hello/lib2_blackbox_test:./lib2 -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target wasm-gc --pkg-name username/hello/lib --enable-coverage --driver-kind blackbox
            moonc build-package ./target/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -pkg username/hello/lib_blackbox_test -is-main -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources username/hello/lib_blackbox_test:./lib -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core ./target/wasm-gc/debug/test/lib/lib.core ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -main username/hello/lib_blackbox_test -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/lib_blackbox_test:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/lib/__generated_driver_for_whitebox_test.mbt --output-metadata ./target/wasm-gc/debug/test/lib/__whitebox_test_info.json ./lib/hello_wbtest.mbt --target wasm-gc --pkg-name username/hello/lib --enable-coverage --driver-kind whitebox
            moonc build-package ./lib/hello.mbt ./lib/hello_wbtest.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_whitebox_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.whitebox_test.core -pkg username/hello/lib -is-main -pkg-sources username/hello/lib:./lib -target wasm-gc -g -O0 -source-map -enable-coverage -whitebox-test -no-mi -test-mode
            moonc link-core ./target/wasm-gc/debug/test/lib/lib.whitebox_test.core -main username/hello/lib -o ./target/wasm-gc/debug/test/lib/lib.whitebox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            (cd $ROOT && moon_cove_report -f=caret)
        "#]],
    );
}
