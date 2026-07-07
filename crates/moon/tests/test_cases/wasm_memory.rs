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

use moonutil::constants::BUILD_DIR;

use super::*;

#[test]
fn test_export_memory_name() {
    let dir = TestDir::new("export_memory.in");

    // Check the commands
    // build wasm-gc/wasm should have this flag
    check(
        get_stdout(&dir, ["build", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/debug/build/lib/lib.core -pkg username/hello/lib -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./lib -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/debug/build/main/main.core -pkg username/hello/main -is-main -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/build/lib/lib.mi:lib -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main:./main -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/build/lib/lib.core ./_build/wasm-gc/debug/build/main/main.core -main username/hello/main -o ./_build/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm-gc -g -O0 -source-map -export-memory-name awesome_memory
        "#]],
    );

    check(
        get_stdout(
            &dir,
            ["build", "--dry-run", "--sort-input", "--target", "wasm"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm/debug/build/lib/lib.core -pkg username/hello/lib -std-path '$MOON_HOME/lib/core/_build/wasm/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./lib -target wasm -g -O0 -workspace-path . -all-pkgs ./_build/wasm/debug/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm/debug/build/main/main.core -pkg username/hello/main -is-main -std-path '$MOON_HOME/lib/core/_build/wasm/release/bundle' -i ./_build/wasm/debug/build/lib/lib.mi:lib -i '$MOON_HOME/lib/core/_build/wasm/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main:./main -target wasm -g -O0 -workspace-path . -all-pkgs ./_build/wasm/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/_build/wasm/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/wasm/release/bundle/core.core' ./_build/wasm/debug/build/lib/lib.core ./_build/wasm/debug/build/main/main.core -main username/hello/main -o ./_build/wasm/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm -g -O0 -wasi -export-memory-name awesome_memory
        "#]],
    );

    // js is not wasm so should not have export-memory-name flag
    check(
        get_stdout(
            &dir,
            ["build", "--dry-run", "--sort-input", "--target", "js"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/js/debug/build/lib/lib.core -pkg username/hello/lib -std-path '$MOON_HOME/lib/core/_build/js/release/bundle' -i '$MOON_HOME/lib/core/_build/js/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./lib -target js -g -O0 -source-map -workspace-path . -all-pkgs ./_build/js/debug/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/js/debug/build/main/main.core -pkg username/hello/main -is-main -std-path '$MOON_HOME/lib/core/_build/js/release/bundle' -i ./_build/js/debug/build/lib/lib.mi:lib -i '$MOON_HOME/lib/core/_build/js/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main:./main -target js -g -O0 -source-map -workspace-path . -all-pkgs ./_build/js/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/_build/js/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/js/release/bundle/core.core' ./_build/js/debug/build/lib/lib.core ./_build/js/debug/build/main/main.core -main username/hello/main -o ./_build/js/debug/build/main/main.js -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target js -g -O0 -source-map
        "#]],
    );

    // Check the results
    let _ = get_stdout(
        &dir,
        ["build", "--target", "wasm-gc", "--release", "--output-wat"],
    );
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

    let _ = get_stdout(
        &dir,
        ["build", "--target", "wasm", "--release", "--output-wat"],
    );
    let content = std::fs::read_to_string(
        dir.join(BUILD_DIR)
            .join("wasm")
            .join("release")
            .join("build")
            .join("main")
            .join("main.wat"),
    )
    .unwrap();
    assert!(content.contains(r#"(export "memory" (memory $moonbit.memory))"#));
    assert!(content.contains(r#"(export "awesome_memory" (memory $moonbit.memory))"#));
}

#[test]
fn test_wasi_link() {
    let dir = TestDir::new("need_link.in");

    check(
        get_stdout(
            &dir,
            [
                "build",
                "--target",
                "wasm",
                "--dry-run",
                "--nostd",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm/debug/build/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target wasm -g -O0 -workspace-path . -all-pkgs ./_build/wasm/debug/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm/debug/build/main/main.core -pkg username/hello/main -is-main -i ./_build/wasm/debug/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm -g -O0 -workspace-path . -all-pkgs ./_build/wasm/debug/build/all_pkgs.json
            moonc link-core ./_build/wasm/debug/build/lib/lib.core ./_build/wasm/debug/build/main/main.core -main username/hello/main -o ./_build/wasm/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -target wasm -g -O0 -wasi
            moonc link-core ./_build/wasm/debug/build/lib/lib.core -main username/hello/lib -o ./_build/wasm/debug/build/lib/lib.wasm -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -target wasm -g -O0
        "#]],
    );

    let dir = TestDir::new("moon_run_with_cli_args.in");
    let output = get_stdout(&dir, ["run", "main", "--target", "wasm", "--dry-run"]);
    assert!(output.contains("-wasi"));

    let output = get_stdout_with_envs(
        &dir,
        ["run", "main", "--target", "wasm", "--dry-run"],
        [("MOON_WASI_LINK", "0")],
    );
    assert!(!output.contains("-wasi"));

    let dir = TestDir::new("no_export_when_test.in");
    let output = get_stdout(&dir, ["test", "--target", "wasm", "--dry-run"]);
    assert_eq!(output.matches("-wasi").count(), 2);
    assert!(output.contains(
        "-exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish'"
    ));

    let dir = TestDir::new("export_memory.in");
    let output = get_stdout(
        &dir,
        ["build", "--target", "wasm", "--dry-run", "--sort-input"],
    );
    assert!(output.contains("-export-memory-name awesome_memory"));
    assert!(output.contains("-wasi"));

    let dir = TestDir::new("need_link.in");
    std::fs::write(
        dir.join("main/moon.pkg.json"),
        r#"{
  "is-main": true,
  "import": [
    "username/hello/lib"
  ],
  "link": {
    "wasm": {
      "flags": [
        "-export-memory-name=raw_memory",
        "-import-memory-module",
        "raw_mod",
        "-import-memory-name",
        "raw_mem"
      ]
    }
  }
}"#,
    )
    .unwrap();
    let output = get_stdout(
        &dir,
        ["build", "--target", "wasm", "--dry-run", "--sort-input"],
    );
    assert!(output.contains("-export-memory-name=raw_memory"));
    assert!(output.contains("-import-memory-module raw_mod -import-memory-name raw_mem"));
    assert!(output.contains("-wasi"));

    let dir = TestDir::new("need_link.in");
    let output = get_stdout(
        &dir,
        ["build", "--target", "wasm-gc", "--dry-run", "--sort-input"],
    );
    assert!(!output.contains("-wasi"));
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
            moonc build-package ./lib/hello.mbt -o ./_build/wasm/debug/build/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target wasm -g -O0 -workspace-path . -all-pkgs ./_build/wasm/debug/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm/debug/build/main/main.core -pkg username/hello/main -is-main -i ./_build/wasm/debug/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm -g -O0 -workspace-path . -all-pkgs ./_build/wasm/debug/build/all_pkgs.json
            moonc link-core ./_build/wasm/debug/build/lib/lib.core ./_build/wasm/debug/build/main/main.core -main username/hello/main -o ./_build/wasm/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -target wasm -g -O0 -wasi -import-memory-module xxx -import-memory-name yyy -heap-start-address 65536
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
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/debug/build/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/debug/build/main/main.core -pkg username/hello/main -is-main -i ./_build/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc link-core ./_build/wasm-gc/debug/build/lib/lib.core ./_build/wasm-gc/debug/build/main/main.core -main username/hello/main -o ./_build/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -target wasm-gc -g -O0 -source-map -import-memory-module xxx -import-memory-name yyy
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
            moonc build-package ./lib/hello.mbt -o ./_build/wasm/debug/build/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target wasm -g -O0 -workspace-path . -all-pkgs ./_build/wasm/debug/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm/debug/build/main/main.core -pkg username/hello/main -is-main -i ./_build/wasm/debug/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm -g -O0 -workspace-path . -all-pkgs ./_build/wasm/debug/build/all_pkgs.json
            moonc link-core ./_build/wasm/debug/build/lib/lib.core ./_build/wasm/debug/build/main/main.core -main username/hello/main -o ./_build/wasm/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -target wasm -g -O0 -wasi -import-memory-module xxx -import-memory-name yyy -memory-limits-min 1 -memory-limits-max 65536 -shared-memory -heap-start-address 65536
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
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/debug/build/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/debug/build/main/main.core -pkg username/hello/main -is-main -i ./_build/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc link-core ./_build/wasm-gc/debug/build/lib/lib.core ./_build/wasm-gc/debug/build/main/main.core -main username/hello/main -o ./_build/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -target wasm-gc -g -O0 -source-map -import-memory-module xxx -import-memory-name yyy -memory-limits-min 1 -memory-limits-max 65535 -shared-memory
        "#]],
    );
}
