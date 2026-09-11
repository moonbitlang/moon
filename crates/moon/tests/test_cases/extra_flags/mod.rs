// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

use super::*;
use crate::get_err_stderr;
use expect_test::expect;

#[test]
fn test_extra_flags() {
    let dir = TestDir::new("extra_flags");
    check(
        get_stdout(
            &dir,
            ["build", "--target", "wasm-gc", "--dry-run", "--nostd"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/debug/build/lib/lib.core -pkg hello/lib -pkg-type library -pkg-sources hello/lib:./lib -target wasm-gc -g -O0 -source-map -g -no-builtin -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/debug/build/main/main.core -pkg hello/main -pkg-type executable -i ./_build/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map -g -no-builtin -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc link-core ./_build/wasm-gc/debug/build/lib/lib.core ./_build/wasm-gc/debug/build/main/main.core -main hello/main -o ./_build/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map -no-builtin
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "build",
                "--target",
                "wasm-gc",
                "--dry-run",
                "--debug",
                "--nostd",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/debug/build/lib/lib.core -pkg hello/lib -pkg-type library -pkg-sources hello/lib:./lib -target wasm-gc -g -O0 -source-map -g -no-builtin -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/debug/build/main/main.core -pkg hello/main -pkg-type executable -i ./_build/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map -g -no-builtin -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc link-core ./_build/wasm-gc/debug/build/lib/lib.core ./_build/wasm-gc/debug/build/main/main.core -main hello/main -o ./_build/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map -no-builtin
        "#]],
    );

    check(
        get_stdout(
            &dir,
            ["run", "--target", "wasm-gc", "main", "--dry-run", "--nostd"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/debug/build/lib/lib.core -pkg hello/lib -pkg-type library -pkg-sources hello/lib:./lib -target wasm-gc -g -O0 -source-map -g -no-builtin -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/debug/build/main/main.core -pkg hello/main -pkg-type executable -i ./_build/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map -g -no-builtin -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc link-core ./_build/wasm-gc/debug/build/lib/lib.core ./_build/wasm-gc/debug/build/main/main.core -main hello/main -o ./_build/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map -no-builtin
            '$MOONRUN_OVERRIDE' ./_build/wasm-gc/debug/build/main/main.wasm --
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "run",
                "--target",
                "wasm-gc",
                "main",
                "--dry-run",
                "--debug",
                "--nostd",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/debug/build/lib/lib.core -pkg hello/lib -pkg-type library -pkg-sources hello/lib:./lib -target wasm-gc -g -O0 -source-map -g -no-builtin -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/debug/build/main/main.core -pkg hello/main -pkg-type executable -i ./_build/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map -g -no-builtin -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc link-core ./_build/wasm-gc/debug/build/lib/lib.core ./_build/wasm-gc/debug/build/main/main.core -main hello/main -o ./_build/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map -no-builtin
            '$MOONRUN_OVERRIDE' ./_build/wasm-gc/debug/build/main/main.wasm --
        "#]],
    );

    let stderr = get_err_stderr(&dir, ["build", "--std", "--nostd"]);
    assert!(stderr.contains("cannot be used with"), "stderr: {stderr}");
    assert!(stderr.contains("--std"), "stderr: {stderr}");
    assert!(stderr.contains("--nostd"), "stderr: {stderr}");
    assert!(!stderr.contains("panicked"), "stderr: {stderr}");
}
