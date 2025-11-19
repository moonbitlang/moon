use crate::test_cases::*;
use crate::util::check;

#[test]
#[cfg(unix)]
fn test_native_backend_tcc_run() {
    let dir = TestDir::new("native_backend/tcc_run");
    check(
        get_stdout(
            &dir,
            ["build", "--target", "native", "--dry-run", "--sort-input"],
        ),
        expect![[r#"
            stubcc -o ./target/native/release/build/lib/stub.o -I$MOON_HOME/include -c -fwrapv -fno-strict-aliasing ./lib/stub.c stubccflags
            cc -o ./target/native/release/build/runtime.o -I$MOON_HOME/include -g -c -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c
            moonc build-package ./lib/hello.mbt -o ./target/native/release/build/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moon_new/lib:./lib -target native -workspace-path .
            moonc build-package ./main/main.mbt -o ./target/native/release/build/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/release/build/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target native -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/build/lib/lib.core ./target/native/release/build/main/main.core -main moon_new/main -o ./target/native/release/build/main/main.c -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native
            stubar -r -c -s ./target/native/release/build/lib/liblib.a ./target/native/release/build/lib/stub.o
            cc -o ./target/native/release/build/main/main.exe -I$MOON_HOME/include -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/libmoonbitrun.o ./target/native/release/build/main/main.c ./target/native/release/build/runtime.o ./target/native/release/build/lib/liblib.a -lm
        "#]],
    );

    #[cfg(target_os = "macos")]
    check(
        get_stdout(
            &dir,
            ["test", "--target", "native", "--dry-run", "--sort-input"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/native/debug/test/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moon_new/lib:./lib -target native -g -O0 -workspace-path .
            moon generate-test-driver --output-driver ./target/native/debug/test/main/__generated_driver_for_internal_test.mbt --output-metadata ./target/native/debug/test/main/__internal_test_info.json ./main/main.mbt --target native --pkg-name moon_new/main --driver-kind internal
            moonc build-package ./main/main.mbt ./target/native/debug/test/main/__generated_driver_for_internal_test.mbt -o ./target/native/debug/test/main/main.internal_test.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/test/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target native -g -O0 -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.core ./target/native/debug/test/main/main.internal_test.core -main moon_new/main -o ./target/native/debug/test/main/main.internal_test.c -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
            moonc build-package ./main/main.mbt -o ./target/native/debug/test/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/test/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target native -g -O0 -workspace-path .
            moon generate-test-driver --output-driver ./target/native/debug/test/main/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/native/debug/test/main/__blackbox_test_info.json --doctest-only ./main/main.mbt --target native --pkg-name moon_new/main --driver-kind blackbox
            moonc build-package ./target/native/debug/test/main/__generated_driver_for_blackbox_test.mbt -doctest-only ./main/main.mbt -o ./target/native/debug/test/main/main.blackbox_test.core -pkg moon_new/main_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/test/lib/lib.mi:lib -i ./target/native/debug/test/main/main.mi:main -pkg-sources moon_new/main_blackbox_test:./main -target native -g -O0 -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.core ./target/native/debug/test/main/main.core ./target/native/debug/test/main/main.blackbox_test.core -main moon_new/main_blackbox_test -o ./target/native/debug/test/main/main.blackbox_test.c -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moon_new/main_blackbox_test:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
            cc -o ./target/native/debug/test/libruntime.dylib -I$MOON_HOME/include -g -shared -fPIC -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c -lm
            stubcc -o ./target/native/debug/test/lib/stub.o -I$MOON_HOME/include -g -c -fwrapv -fno-strict-aliasing -fPIC -DMOONBIT_USE_SHARED_RUNTIME ./lib/stub.c stubccflags
            stubcc -o ./target/native/debug/test/lib/liblib.dylib -L./target/native/debug/test -shared -fPIC ./target/native/debug/test/lib/stub.o -lm -lruntime -Wl,-rpath,./target/native/debug/test stubcclinkflags
            moon generate-test-driver --output-driver ./target/native/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./target/native/debug/test/lib/__internal_test_info.json ./lib/hello.mbt --target native --pkg-name moon_new/lib --driver-kind internal
            moonc build-package ./lib/hello.mbt ./target/native/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/native/debug/test/lib/lib.internal_test.core -pkg moon_new/lib -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moon_new/lib:./lib -target native -g -O0 -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.internal_test.core -main moon_new/lib -o ./target/native/debug/test/lib/lib.internal_test.c -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
            moon generate-test-driver --output-driver ./target/native/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/native/debug/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target native --pkg-name moon_new/lib --driver-kind blackbox
            moonc build-package ./target/native/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./target/native/debug/test/lib/lib.blackbox_test.core -pkg moon_new/lib_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/test/lib/lib.mi:lib -pkg-sources moon_new/lib_blackbox_test:./lib -target native -g -O0 -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.core ./target/native/debug/test/lib/lib.blackbox_test.core -main moon_new/lib_blackbox_test -o ./target/native/debug/test/lib/lib.blackbox_test.c -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/lib_blackbox_test:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
        "#]],
    );

    #[cfg(target_os = "linux")]
    check(
        get_stdout(
            &dir,
            ["test", "--target", "native", "--dry-run", "--sort-input"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/native/debug/test/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moon_new/lib:./lib -target native -g -O0 -workspace-path .
            moon generate-test-driver --output-driver ./target/native/debug/test/main/__generated_driver_for_internal_test.mbt --output-metadata ./target/native/debug/test/main/__internal_test_info.json ./main/main.mbt --target native --pkg-name moon_new/main --driver-kind internal
            moonc build-package ./main/main.mbt ./target/native/debug/test/main/__generated_driver_for_internal_test.mbt -o ./target/native/debug/test/main/main.internal_test.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/test/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target native -g -O0 -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.core ./target/native/debug/test/main/main.internal_test.core -main moon_new/main -o ./target/native/debug/test/main/main.internal_test.c -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
            moonc build-package ./main/main.mbt -o ./target/native/debug/test/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/test/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target native -g -O0 -workspace-path .
            moon generate-test-driver --output-driver ./target/native/debug/test/main/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/native/debug/test/main/__blackbox_test_info.json --doctest-only ./main/main.mbt --target native --pkg-name moon_new/main --driver-kind blackbox
            moonc build-package ./target/native/debug/test/main/__generated_driver_for_blackbox_test.mbt -doctest-only ./main/main.mbt -o ./target/native/debug/test/main/main.blackbox_test.core -pkg moon_new/main_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/test/lib/lib.mi:lib -i ./target/native/debug/test/main/main.mi:main -pkg-sources moon_new/main_blackbox_test:./main -target native -g -O0 -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.core ./target/native/debug/test/main/main.core ./target/native/debug/test/main/main.blackbox_test.core -main moon_new/main_blackbox_test -o ./target/native/debug/test/main/main.blackbox_test.c -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moon_new/main_blackbox_test:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
            cc -o ./target/native/debug/test/libruntime.so -I$MOON_HOME/include -g -shared -fPIC -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c -lm
            stubcc -o ./target/native/debug/test/lib/stub.o -I$MOON_HOME/include -g -c -fwrapv -fno-strict-aliasing -fPIC -DMOONBIT_USE_SHARED_RUNTIME ./lib/stub.c stubccflags
            stubcc -o ./target/native/debug/test/lib/liblib.so -L./target/native/debug/test -shared -fPIC ./target/native/debug/test/lib/stub.o -lm -lruntime -Wl,-rpath,./target/native/debug/test stubcclinkflags
            moon generate-test-driver --output-driver ./target/native/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./target/native/debug/test/lib/__internal_test_info.json ./lib/hello.mbt --target native --pkg-name moon_new/lib --driver-kind internal
            moonc build-package ./lib/hello.mbt ./target/native/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/native/debug/test/lib/lib.internal_test.core -pkg moon_new/lib -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moon_new/lib:./lib -target native -g -O0 -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.internal_test.core -main moon_new/lib -o ./target/native/debug/test/lib/lib.internal_test.c -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
            moon generate-test-driver --output-driver ./target/native/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/native/debug/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target native --pkg-name moon_new/lib --driver-kind blackbox
            moonc build-package ./target/native/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./target/native/debug/test/lib/lib.blackbox_test.core -pkg moon_new/lib_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/test/lib/lib.mi:lib -pkg-sources moon_new/lib_blackbox_test:./lib -target native -g -O0 -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.core ./target/native/debug/test/lib/lib.blackbox_test.core -main moon_new/lib_blackbox_test -o ./target/native/debug/test/lib/lib.blackbox_test.c -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/lib_blackbox_test:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
        "#]],
    );
}
