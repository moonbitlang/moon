use super::*;

#[test]
fn test_indirect_depend_virtual() {
    let dir = TestDir::new("virtual_pkg_dep/indirect_depend_virtual");
    // need to omit the command generated for cc, because it's platform dependent
    check(
        get_stdout(&dir, ["run", "src/main", "--target", "native", "--dry-run"])
            .lines()
            .filter(|x| !(x.contains("cc") || x.contains("cl.exe")))
            .collect::<Vec<_>>()
            .join("\n"),
        expect![[r#"
            moonc build-interface ./src/virtual/pkg.mbti -o ./_build/native/debug/build/virtual/virtual.mi -i '$MOON_HOME/lib/core/target/native/release/bundle/prelude/prelude.mi:prelude' -pkg indirect_depend_virtual/virtual -pkg-sources indirect_depend_virtual/virtual:./src/virtual -virtual -std-path '$MOON_HOME/lib/core/target/native/release/bundle' -error-format json
            moonc build-package ./src/middle/p.mbt -o ./_build/native/debug/build/middle/middle.core -pkg indirect_depend_virtual/middle -std-path '$MOON_HOME/lib/core/target/native/release/bundle' -i '$MOON_HOME/lib/core/target/native/release/bundle/prelude/prelude.mi:prelude' -i ./_build/native/debug/build/virtual/virtual.mi:virtual -pkg-sources indirect_depend_virtual/middle:./src/middle -target native -g -O0 -workspace-path . -all-pkgs ./_build/native/debug/build/all_pkgs.json
            moonc build-package ./src/main/main.mbt -o ./_build/native/debug/build/main/main.core -pkg indirect_depend_virtual/main -is-main -std-path '$MOON_HOME/lib/core/target/native/release/bundle' -i ./_build/native/debug/build/middle/middle.mi:middle -i '$MOON_HOME/lib/core/target/native/release/bundle/prelude/prelude.mi:prelude' -i ./_build/native/debug/build/virtual/virtual.mi:virtual -pkg-sources indirect_depend_virtual/main:./src/main -target native -g -O0 -workspace-path . -all-pkgs ./_build/native/debug/build/all_pkgs.json
            moonc build-package ./src/impl/p.mbt -o ./_build/native/debug/build/impl/impl.core -pkg indirect_depend_virtual/impl -std-path '$MOON_HOME/lib/core/target/native/release/bundle' -i '$MOON_HOME/lib/core/target/native/release/bundle/prelude/prelude.mi:prelude' -pkg-sources indirect_depend_virtual/impl:./src/impl -target native -g -O0 -check-mi ./_build/native/debug/build/virtual/virtual.mi -impl-virtual -pkg-sources indirect_depend_virtual/virtual:./src/virtual -workspace-path . -all-pkgs ./_build/native/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/native/release/bundle/core.core' ./_build/native/debug/build/impl/impl.core ./_build/native/debug/build/middle/middle.core ./_build/native/debug/build/main/main.core -main indirect_depend_virtual/main -o ./_build/native/debug/build/main/main.c -pkg-config-path ./src/main/moon.pkg.json -pkg-sources indirect_depend_virtual/impl:./src/impl -pkg-sources indirect_depend_virtual/middle:./src/middle -pkg-sources indirect_depend_virtual/main:./src/main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target native -g -O0
            ./_build/native/debug/build/main/main.exe"#]],
    );
    check(
        get_stdout(&dir, ["run", "src/main", "--target", "native"]),
        expect![[r#"
            43
            45
        "#]],
    );
}
