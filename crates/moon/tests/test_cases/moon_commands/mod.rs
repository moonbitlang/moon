mod cram;
mod delegation;
mod explain;
mod help_and_version;
mod moonx;
mod run;
mod runwasm;
mod tool;
mod whoami;

use crate::util::{check, moon_bin, moonrun_bin};
use crate::{TestDir, get_err_stderr_with_envs, get_stdout, moon_cmd};
use expect_test::expect;

#[test]
fn test_moon_cmd() {
    let dir = TestDir::new("moon_commands");
    check(
        get_stdout(
            &dir,
            ["build", "--target", "wasm-gc", "--dry-run", "--sort-input"],
        ),
        expect![[r#"
            moonc build-package ./lib/list/lib.mbt -o ./_build/wasm-gc/debug/build/lib/list/list.core -pkg design/lib/list -pkg-type library -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/builtin/builtin.mi:moonbitlang/core/builtin' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources design/lib/list:./lib/list -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./lib/queue/lib.mbt -o ./_build/wasm-gc/debug/build/lib/queue/queue.core -pkg design/lib/queue -pkg-type library -i ./_build/wasm-gc/debug/build/lib/list/list.mi:list -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/builtin/builtin.mi:moonbitlang/core/builtin' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources design/lib/queue:./lib/queue -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./main2/main.mbt -o ./_build/wasm-gc/debug/build/main2/main2.core -pkg design/main2 -pkg-type executable -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/builtin/builtin.mi:moonbitlang/core/builtin' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -i ./_build/wasm-gc/debug/build/lib/queue/queue.mi:queue -pkg-sources design/main2:./main2 -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/builtin/builtin.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/char/char.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/float/float.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/int16/int16.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/buffer/buffer.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/double/double.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/uint64/uint64.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/internal/strconv/strconv.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/v128/v128.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/json/json.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/debug/debug.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/uint/uint.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/uint16/uint16.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/bigint/bigint.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/bytes/internal/regex_engine/shared_types/byte_set/byte_set.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/bytes/internal/regex_engine/shared_types/shared_types.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/bytes/internal/regex_engine/ast/ast.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/bytes/internal/regex_engine/symbol_map/symbol_map.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/test/test.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/hashmap/hashmap.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/bytes/internal/regex_engine/regex_engine.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/set/set.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/bytes/internal/regex_parser/regex_parser.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/bytes/bytes.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/lazy/lazy.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/ref/ref.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/string/internal/regex_engine/shared_types/rechar_set/rechar_set.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/string/internal/regex_engine/shared_types/shared_types.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/string/internal/regex_engine/ast/ast.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/sorted_set/sorted_set.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/string/internal/regex_engine/symbol_map/symbol_map.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/string/internal/regex_engine/regex_engine.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/string/internal/regex_parser/regex_parser.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/string/string.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.core' ./_build/wasm-gc/debug/build/lib/list/list.core ./_build/wasm-gc/debug/build/lib/queue/queue.core ./_build/wasm-gc/debug/build/main2/main2.core -main design/main2 -o ./_build/wasm-gc/debug/build/main2/main2.wasm -pkg-config-path ./main2/moon.pkg -pkg-sources design/lib/list:./lib/list -pkg-sources design/lib/queue:./lib/queue -pkg-sources design/main2:./main2 -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm-gc -g -O0 -source-map
            moonc build-package ./main1/main.mbt -o ./_build/wasm-gc/debug/build/main1/main1.core -pkg design/main1 -pkg-type executable -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/builtin/builtin.mi:moonbitlang/core/builtin' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -i ./_build/wasm-gc/debug/build/lib/queue/queue.mi:queue -pkg-sources design/main1:./main1 -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/builtin/builtin.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/char/char.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/float/float.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/int16/int16.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/buffer/buffer.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/double/double.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/uint64/uint64.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/internal/strconv/strconv.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/v128/v128.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/json/json.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/debug/debug.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/uint/uint.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/uint16/uint16.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/bigint/bigint.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/bytes/internal/regex_engine/shared_types/byte_set/byte_set.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/bytes/internal/regex_engine/shared_types/shared_types.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/bytes/internal/regex_engine/ast/ast.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/bytes/internal/regex_engine/symbol_map/symbol_map.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/test/test.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/hashmap/hashmap.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/bytes/internal/regex_engine/regex_engine.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/set/set.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/bytes/internal/regex_parser/regex_parser.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/bytes/bytes.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/lazy/lazy.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/ref/ref.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/string/internal/regex_engine/shared_types/rechar_set/rechar_set.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/string/internal/regex_engine/shared_types/shared_types.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/string/internal/regex_engine/ast/ast.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/sorted_set/sorted_set.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/string/internal/regex_engine/symbol_map/symbol_map.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/string/internal/regex_engine/regex_engine.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/string/internal/regex_parser/regex_parser.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/string/string.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.core' ./_build/wasm-gc/debug/build/lib/list/list.core ./_build/wasm-gc/debug/build/lib/queue/queue.core ./_build/wasm-gc/debug/build/main1/main1.core -main design/main1 -o ./_build/wasm-gc/debug/build/main1/main1.wasm -pkg-config-path ./main1/moon.pkg -pkg-sources design/lib/list:./lib/list -pkg-sources design/lib/queue:./lib/queue -pkg-sources design/main1:./main1 -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm-gc -g -O0 -source-map
        "#]],
    );
}

#[test]
fn test_test_with_explicit_target_fails_on_test_executable_exit_code() {
    let dir = TestDir::new("moon_run_with_cli_args.in");
    let stderr = get_err_stderr_with_envs(
        &dir,
        ["test", "main", "--target", "wasm", "--filter", "exit-7"],
        [("MOONRUN_OVERRIDE", moonrun_bin())],
    );
    snapbox::assert_data_eq!(
        stderr,
        snapbox::str![[r#"
...
Error: failed to run test for target Wasm

Caused by:
    Failed to run the test: $ROOT/_build/wasm/debug/test/main/main.blackbox_test.wasm
    The test executable exited with exit [..]: 7
    Active test at executable exit:
      - $ROOT/main/exit_wasm_test.mbt:1 "exit-7"

"#]],
    );
}

#[cfg(any(unix, windows))]
fn path_with_fake_bin(fake_bin_dir: &std::path::Path) -> std::ffi::OsString {
    std::env::join_paths(
        std::iter::once(fake_bin_dir.to_path_buf()).chain(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        )),
    )
    .expect("failed to build PATH")
}

#[test]
fn test_moon_doc_query_warns_and_succeeds() {
    let dir = TestDir::new_empty();

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["doc", "@json"])
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
package "moonbitlang/core/json"
...
"#]])
        .stderr_eq(
            "Warning: `moon doc <SYMBOL>` is deprecated; use `moon ide doc <SYMBOL>` instead.\n",
        );
}

#[test]
fn test_manifest_path_is_not_supported() {
    let dir = TestDir::new("moon_commands");
    moon_cmd(&dir)
        .args(["check", "--manifest-path", "moon.mod", "--dry-run"])
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
error: unexpected argument '--manifest-path' found

  tip: to pass '--manifest-path' as a value, use '-- --manifest-path'

Usage: moon[EXE] check [OPTIONS] [PATH]...

For more information, try '--help'.

"#]]);
}

#[test]
#[ignore]
#[cfg(unix)]
fn test_bench4() {
    let dir = TestDir::new_empty();
    get_stdout(&dir, ["generate-build-matrix", "-n", "4", "-o", "bench4"]);
    check(
        get_stdout(
            &dir,
            [
                "-C",
                "./bench4",
                "run",
                "--target-dir",
                "./bench4/target",
                "main",
            ],
        ),
        expect![[r#"
            ok
        "#]],
    );

    get_stdout(
        &dir,
        [
            "-C",
            "./bench4",
            "run",
            "--target-dir",
            "./bench4/target",
            "--trace",
            "main",
        ],
    );

    let trace_file = dunce::canonicalize(dir.join("./trace.json")).unwrap();
    let t = std::fs::read_to_string(trace_file).unwrap();
    assert!(t.contains("moonbit::build::read"));
    assert!(t.contains(r#""name":"work.run""#));
    assert!(t.contains(r#""name":"run""#));
    assert!(t.contains(r#""name":"main""#));
}
