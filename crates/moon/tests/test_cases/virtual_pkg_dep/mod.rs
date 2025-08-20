use super::*;

#[test]
fn test_indirect_depend_virtual() {
    let dir = TestDir::new("virtual_pkg_dep/indirect_depend_virtual");
    // need to omit the command generated for cc, because it's platform dependent
    check(
        get_stdout(&dir, ["run", "src/main", "--target", "native", "--dry-run"])
            .lines()
            .collect::<Vec<_>>()[0..5]
            .join("\n"),
        expect![[r#"
            moonc build-interface ./src/virtual/pkg.mbti -o ./target/native/release/build/virtual/virtual.mi -pkg indirect_depend_virtual/virtual -pkg-sources indirect_depend_virtual/virtual:./src/virtual -virtual -std-path $MOON_HOME/lib/core/target/native/release/bundle -error-format=json
            moonc build-package ./src/middle/p.mbt -o ./target/native/release/build/middle/middle.core -pkg indirect_depend_virtual/middle -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/release/build/virtual/virtual.mi:virtual -pkg-sources indirect_depend_virtual/middle:./src/middle -target native
            moonc build-package ./src/impl/p.mbt -o ./target/native/release/build/impl/impl.core -pkg indirect_depend_virtual/impl -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources indirect_depend_virtual/impl:./src/impl -target native -check-mi ./target/native/release/build/virtual/virtual.mi -impl-virtual -no-mi -pkg-sources indirect_depend_virtual/virtual:./src/virtual
            moonc build-package ./src/main/main.mbt -o ./target/native/release/build/main/main.core -pkg indirect_depend_virtual/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/release/build/middle/middle.mi:middle -i ./target/native/release/build/virtual/virtual.mi:virtual -pkg-sources indirect_depend_virtual/main:./src/main -target native
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/build/impl/impl.core ./target/native/release/build/middle/middle.core ./target/native/release/build/main/main.core -main indirect_depend_virtual/main -o ./target/native/release/build/main/main.c -pkg-config-path ./src/main/moon.pkg.json -pkg-sources indirect_depend_virtual/impl:./src/impl -pkg-sources indirect_depend_virtual/middle:./src/middle -pkg-sources indirect_depend_virtual/main:./src/main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native"#]],
    );
    check(
        get_stdout(&dir, ["run", "src/main", "--target", "native"]),
        expect![[r#"
            43
            45
        "#]],
    );
}
