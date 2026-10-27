use super::*;

/// The implicit `moonbitlang/core/builtin` and `moonbitlang/core/prelude`
/// imports must not take the short aliases `builtin` and `prelude`: local
/// packages with those names (whose blackbox tests import them by that alias)
/// and imports aliased `@builtin` or `@prelude` all need them.
#[test]
fn test_implicit_stdlib_alias_does_not_collide() {
    let dir = TestDir::new("implicit_stdlib_alias/implicit_stdlib_alias.in");

    assert_command_matches(
        get_stdout(&dir, ["check", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc check ./util/util.mbt -o ./_build/wasm/debug/check/util/util.mi -pkg username/hello/util -pkg-type library -i '$MOON_HOME/lib/core/_build/wasm/release/bundle/builtin/builtin.mi:moonbitlang/core/builtin' -i '$MOON_HOME/lib/core/_build/wasm/release/bundle/prelude/prelude.mi:moonbitlang/core/prelude' -pkg-sources username/hello/util:./util -target wasm -workspace-path . -all-pkgs ./_build/wasm/debug/check/all_pkgs.json
            mooninfo '-format=text' ./_build/wasm/debug/check/util/util.mi '-o=./_build/wasm/debug/check/util/util.mbti'
            moonc check -doctest-only ./util/util.mbt -include-doctests -o ./_build/wasm/debug/check/util/util.blackbox_test.mi -pkg username/hello/util_blackbox_test -pkg-type library -i '$MOON_HOME/lib/core/_build/wasm/release/bundle/builtin/builtin.mi:moonbitlang/core/builtin' -i '$MOON_HOME/lib/core/_build/wasm/release/bundle/prelude/prelude.mi:moonbitlang/core/prelude' -i ./_build/wasm/debug/check/util/util.mi:util -pkg-sources username/hello/util_blackbox_test:./util -target wasm -blackbox-test -workspace-path . -all-pkgs ./_build/wasm/debug/check/all_pkgs.json
            moonc check ./prelude/prelude.mbt -o ./_build/wasm/debug/check/prelude/prelude.mi -pkg username/hello/prelude -pkg-type library -i '$MOON_HOME/lib/core/_build/wasm/release/bundle/builtin/builtin.mi:moonbitlang/core/builtin' -i '$MOON_HOME/lib/core/_build/wasm/release/bundle/prelude/prelude.mi:moonbitlang/core/prelude' -pkg-sources username/hello/prelude:./prelude -target wasm -workspace-path . -all-pkgs ./_build/wasm/debug/check/all_pkgs.json
            mooninfo '-format=text' ./_build/wasm/debug/check/prelude/prelude.mi '-o=./_build/wasm/debug/check/prelude/prelude.mbti'
            moonc check ./prelude/prelude_test.mbt -doctest-only ./prelude/prelude.mbt -include-doctests -o ./_build/wasm/debug/check/prelude/prelude.blackbox_test.mi -pkg username/hello/prelude_blackbox_test -pkg-type library -i '$MOON_HOME/lib/core/_build/wasm/release/bundle/builtin/builtin.mi:moonbitlang/core/builtin' -i '$MOON_HOME/lib/core/_build/wasm/release/bundle/prelude/prelude.mi:moonbitlang/core/prelude' -i ./_build/wasm/debug/check/prelude/prelude.mi:prelude -pkg-sources username/hello/prelude_blackbox_test:./prelude -target wasm -blackbox-test -workspace-path . -all-pkgs ./_build/wasm/debug/check/all_pkgs.json
            moonc check ./builtin/builtin.mbt -o ./_build/wasm/debug/check/builtin/builtin.mi -pkg username/hello/builtin -pkg-type library -i '$MOON_HOME/lib/core/_build/wasm/release/bundle/builtin/builtin.mi:moonbitlang/core/builtin' -i '$MOON_HOME/lib/core/_build/wasm/release/bundle/prelude/prelude.mi:moonbitlang/core/prelude' -pkg-sources username/hello/builtin:./builtin -target wasm -workspace-path . -all-pkgs ./_build/wasm/debug/check/all_pkgs.json
            mooninfo '-format=text' ./_build/wasm/debug/check/builtin/builtin.mi '-o=./_build/wasm/debug/check/builtin/builtin.mbti'
            moonc check ./builtin/builtin_test.mbt -doctest-only ./builtin/builtin.mbt -include-doctests -o ./_build/wasm/debug/check/builtin/builtin.blackbox_test.mi -pkg username/hello/builtin_blackbox_test -pkg-type library -i ./_build/wasm/debug/check/builtin/builtin.mi:builtin -i '$MOON_HOME/lib/core/_build/wasm/release/bundle/builtin/builtin.mi:moonbitlang/core/builtin' -i '$MOON_HOME/lib/core/_build/wasm/release/bundle/prelude/prelude.mi:moonbitlang/core/prelude' -pkg-sources username/hello/builtin_blackbox_test:./builtin -target wasm -blackbox-test -workspace-path . -all-pkgs ./_build/wasm/debug/check/all_pkgs.json
            moonc check ./app/app.mbt -o ./_build/wasm/debug/check/app/app.mi -pkg username/hello/app -pkg-type library -i ./_build/wasm/debug/check/util/util.mi:builtin -i '$MOON_HOME/lib/core/_build/wasm/release/bundle/builtin/builtin.mi:moonbitlang/core/builtin' -i '$MOON_HOME/lib/core/_build/wasm/release/bundle/prelude/prelude.mi:moonbitlang/core/prelude' -i ./_build/wasm/debug/check/builtin/builtin.mi:prelude -pkg-sources username/hello/app:./app -target wasm -workspace-path . -all-pkgs ./_build/wasm/debug/check/all_pkgs.json
            mooninfo '-format=text' ./_build/wasm/debug/check/app/app.mi '-o=./_build/wasm/debug/check/app/app.mbti'
            moonc check -doctest-only ./app/app.mbt -include-doctests -o ./_build/wasm/debug/check/app/app.blackbox_test.mi -pkg username/hello/app_blackbox_test -pkg-type library -i ./_build/wasm/debug/check/app/app.mi:app -i ./_build/wasm/debug/check/util/util.mi:builtin -i '$MOON_HOME/lib/core/_build/wasm/release/bundle/builtin/builtin.mi:moonbitlang/core/builtin' -i '$MOON_HOME/lib/core/_build/wasm/release/bundle/prelude/prelude.mi:moonbitlang/core/prelude' -i ./_build/wasm/debug/check/builtin/builtin.mi:prelude -pkg-sources username/hello/app_blackbox_test:./app -target wasm -blackbox-test -workspace-path . -all-pkgs ./_build/wasm/debug/check/all_pkgs.json
        "#]],
    );

    let stdout = get_stdout(&dir, ["test"]);
    check(
        &stdout,
        expect![[r#"
            Total tests: 3, passed: 3, failed: 0.
        "#]],
    );
}
