use crate::test_cases::*;
use crate::util::check;

#[test]
#[cfg(unix)]
#[ignore = "platform-dependent behavior"]
fn test_native_backend_cc_flags() {
    let dir = TestDir::new("native_backend/cc_flags");
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
    let dir = TestDir::new("native_backend/cc_flags");
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
            moonc build-package ./target/native/debug/test/main/__generated_driver_for_blackbox_test.mbt -doctest-only ./main/main.mbt -o ./target/native/debug/test/main/main.blackbox_test.core -pkg moon_new/main_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/test/lib/lib.mi:lib -i ./target/native/debug/test/main/main.mi:main -pkg-sources moon_new/main_blackbox_test:./main -target native -g -O0 -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
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
            moonc build-package ./target/native/debug/test/main/__generated_driver_for_blackbox_test.mbt -doctest-only ./main/main.mbt -o ./target/native/debug/test/main/main.blackbox_test.core -pkg moon_new/main_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/test/lib/lib.mi:lib -i ./target/native/debug/test/main/main.mi:main -pkg-sources moon_new/main_blackbox_test:./main -target native -g -O0 -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
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
