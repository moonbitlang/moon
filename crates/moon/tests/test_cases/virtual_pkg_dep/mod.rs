use super::*;

#[test]
fn test_indirect_depend_virtual() {
    let dir = TestDir::new("virtual_pkg_dep/indirect_depend_virtual");
    // Omit native toolchain commands and their MSVC environment because they
    // are platform dependent.
    let dry_run = get_stdout_with_envs(
        &dir,
        ["run", "src/main", "--target", "native", "--dry-run"],
        [("MOONBIT_NEW_NATIVE", "0")],
    )
    .lines()
    .filter(|line| {
        !(line.contains("cc")
            || line.contains("cl.exe")
            || line.contains("dsymutil")
            || line.contains("libruntime")
            || *line == "  env:"
            || line.starts_with("    LIB=")
            || line.starts_with("    PATH=")
            || line.starts_with("    INCLUDE="))
    })
    .collect::<Vec<_>>()
    .join("\n");

    snapbox::assert_data_eq!(
        dry_run,
        snapbox::str![[r#"
moonc build-interface ./src/virtual/pkg.mbti -o ./_build/native/debug/build/virtual/virtual.mi -i '$MOON_HOME/lib/core/_build/native/release/bundle/builtin/builtin.mi:moonbitlang/core/builtin' -i '$MOON_HOME/lib/core/_build/native/release/bundle/prelude/prelude.mi:prelude' -pkg indirect_depend_virtual/virtual -pkg-sources indirect_depend_virtual/virtual:./src/virtual -virtual -error-format json
moonc build-package ./src/middle/p.mbt -o ./_build/native/debug/build/middle/middle.core -pkg indirect_depend_virtual/middle -pkg-type library -i '$MOON_HOME/lib/core/_build/native/release/bundle/builtin/builtin.mi:moonbitlang/core/builtin' -i '$MOON_HOME/lib/core/_build/native/release/bundle/prelude/prelude.mi:prelude' -i ./_build/native/debug/build/virtual/virtual.mi:virtual -pkg-sources indirect_depend_virtual/middle:./src/middle -target native -g -O0 -workspace-path . -all-pkgs ./_build/native/debug/build/all_pkgs.json
moonc build-package ./src/main/main.mbt -o ./_build/native/debug/build/main/main.core -pkg indirect_depend_virtual/main -pkg-type executable -i ./_build/native/debug/build/middle/middle.mi:middle -i '$MOON_HOME/lib/core/_build/native/release/bundle/builtin/builtin.mi:moonbitlang/core/builtin' -i '$MOON_HOME/lib/core/_build/native/release/bundle/prelude/prelude.mi:prelude' -i ./_build/native/debug/build/virtual/virtual.mi:virtual -pkg-sources indirect_depend_virtual/main:./src/main -target native -g -O0 -workspace-path . -all-pkgs ./_build/native/debug/build/all_pkgs.json
moonc build-package ./src/impl/p.mbt -o ./_build/native/debug/build/impl/impl.core -pkg indirect_depend_virtual/impl -pkg-type library -i '$MOON_HOME/lib/core/_build/native/release/bundle/builtin/builtin.mi:moonbitlang/core/builtin' -i '$MOON_HOME/lib/core/_build/native/release/bundle/prelude/prelude.mi:prelude' -pkg-sources indirect_depend_virtual/impl:./src/impl -target native -g -O0 -check-mi ./_build/native/debug/build/virtual/virtual.mi -impl-virtual -pkg-sources indirect_depend_virtual/virtual:./src/virtual -workspace-path . -all-pkgs ./_build/native/debug/build/all_pkgs.json
moonc link-core '$MOON_HOME/lib/core/_build/native/release/bundle/builtin/builtin.core' '$MOON_HOME/lib/core/_build/native/release/bundle/char/char.core' '$MOON_HOME/lib/core/_build/native/release/bundle/float/float.core' '$MOON_HOME/lib/core/_build/native/release/bundle/int16/int16.core' '$MOON_HOME/lib/core/_build/native/release/bundle/debug/debug.core' '$MOON_HOME/lib/core/_build/native/release/bundle/buffer/buffer.core' '$MOON_HOME/lib/core/_build/native/release/bundle/double/double.core' '$MOON_HOME/lib/core/_build/native/release/bundle/test/test.core' '$MOON_HOME/lib/core/_build/native/release/bundle/uint64/uint64.core' '$MOON_HOME/lib/core/_build/native/release/bundle/internal/strconv/strconv.core' '$MOON_HOME/lib/core/_build/native/release/bundle/v128/v128.core' '$MOON_HOME/lib/core/_build/native/release/bundle/json/json.core' '$MOON_HOME/lib/core/_build/native/release/bundle/uint/uint.core' '$MOON_HOME/lib/core/_build/native/release/bundle/uint16/uint16.core' '$MOON_HOME/lib/core/_build/native/release/bundle/bigint/bigint.core' '$MOON_HOME/lib/core/_build/native/release/bundle/lazy/lazy.core' '$MOON_HOME/lib/core/_build/native/release/bundle/ref/ref.core' '$MOON_HOME/lib/core/_build/native/release/bundle/set/set.core' '$MOON_HOME/lib/core/_build/native/release/bundle/hashmap/hashmap.core' '$MOON_HOME/lib/core/_build/native/release/bundle/internal/regex_engine/shared_types/rechar_set/rechar_set.core' '$MOON_HOME/lib/core/_build/native/release/bundle/internal/regex_engine/shared_types/shared_types.core' '$MOON_HOME/lib/core/_build/native/release/bundle/string/internal/regex_engine/ast/ast.core' '$MOON_HOME/lib/core/_build/native/release/bundle/sorted_set/sorted_set.core' '$MOON_HOME/lib/core/_build/native/release/bundle/string/internal/regex_engine/symbol_map/symbol_map.core' '$MOON_HOME/lib/core/_build/native/release/bundle/string/internal/regex_engine/regex_engine.core' '$MOON_HOME/lib/core/_build/native/release/bundle/string/internal/regex_parser/regex_parser.core' '$MOON_HOME/lib/core/_build/native/release/bundle/string/string.core' '$MOON_HOME/lib/core/_build/native/release/bundle/prelude/prelude.core' ./_build/native/debug/build/impl/impl.core ./_build/native/debug/build/middle/middle.core ./_build/native/debug/build/main/main.core -main indirect_depend_virtual/main -o ./_build/native/debug/build/main/main.c -pkg-config-path ./src/main/moon.pkg -pkg-sources indirect_depend_virtual/impl:./src/impl -pkg-sources indirect_depend_virtual/middle:./src/middle -pkg-sources indirect_depend_virtual/main:./src/main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target native -g -O0
./_build/native/debug/build/main/main[..]
"#]],
    );
    check(
        get_stdout(&dir, ["run", "src/main", "--target", "native"]),
        expect![[r#"
            43
            45
        "#]],
    );
}
