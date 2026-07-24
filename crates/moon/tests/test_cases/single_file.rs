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

#[cfg(unix)]
#[test]
fn test_moon_run_single_file_dry_run() {
    let dir = TestDir::new("run_single_mbt_file.in");

    let output = get_stdout_with_envs(
        &dir,
        ["run", "a/b/single.mbt", "--target", "native", "--dry-run"],
        [("MOONBIT_NEW_NATIVE", "0")],
    )
    // Normalize clang-only warnings to keep snapshots portable across macOS/Linux.
    .replace(" -Wno-unused-value", "");
    check(
        &output,
        expect![[r#"
            moonc build-package ./single.mbt -o ./_build/native/debug/build/single/single.core -pkg moon/test/single -pkg-type executable -std-path '$MOON_HOME/lib/core/_build/native/release/bundle' -i '$MOON_HOME/lib/core/_build/native/release/bundle/abort/abort.mi:abort' -i '$MOON_HOME/lib/core/_build/native/release/bundle/argparse/argparse.mi:argparse' -i '$MOON_HOME/lib/core/_build/native/release/bundle/array/array.mi:array' -i '$MOON_HOME/lib/core/_build/native/release/bundle/encoding/ascii/ascii.mi:ascii' -i '$MOON_HOME/lib/core/_build/native/release/bundle/encoding/base64/base64.mi:base64' -i '$MOON_HOME/lib/core/_build/native/release/bundle/bench/bench.mi:bench' -i '$MOON_HOME/lib/core/_build/native/release/bundle/bigint/bigint.mi:bigint' -i '$MOON_HOME/lib/core/_build/native/release/bundle/bool/bool.mi:bool' -i '$MOON_HOME/lib/core/_build/native/release/bundle/buffer/buffer.mi:buffer' -i '$MOON_HOME/lib/core/_build/native/release/bundle/builtin/builtin.mi:builtin' -i '$MOON_HOME/lib/core/_build/native/release/bundle/byte/byte.mi:byte' -i '$MOON_HOME/lib/core/_build/native/release/bundle/bytes/bytes.mi:bytes' -i '$MOON_HOME/lib/core/_build/native/release/bundle/char/char.mi:char' -i '$MOON_HOME/lib/core/_build/native/release/bundle/cmp/cmp.mi:cmp' -i '$MOON_HOME/lib/core/_build/native/release/bundle/coverage/coverage.mi:coverage' -i '$MOON_HOME/lib/core/_build/native/release/bundle/debug/debug.mi:debug' -i '$MOON_HOME/lib/core/_build/native/release/bundle/deque/deque.mi:deque' -i '$MOON_HOME/lib/core/_build/native/release/bundle/double/double.mi:double' -i '$MOON_HOME/lib/core/_build/native/release/bundle/env/env.mi:env' -i '$MOON_HOME/lib/core/_build/native/release/bundle/error/error.mi:error' -i '$MOON_HOME/lib/core/_build/native/release/bundle/float/float.mi:float' -i '$MOON_HOME/lib/core/_build/native/release/bundle/hashmap/hashmap.mi:hashmap' -i '$MOON_HOME/lib/core/_build/native/release/bundle/hashset/hashset.mi:hashset' -i '$MOON_HOME/lib/core/_build/native/release/bundle/immut/array/array.mi:immut/array' -i '$MOON_HOME/lib/core/_build/native/release/bundle/immut/hashmap/hashmap.mi:immut/hashmap' -i '$MOON_HOME/lib/core/_build/native/release/bundle/immut/hashset/hashset.mi:immut/hashset' -i '$MOON_HOME/lib/core/_build/native/release/bundle/immut/priority_queue/priority_queue.mi:immut/priority_queue' -i '$MOON_HOME/lib/core/_build/native/release/bundle/immut/sorted_map/sorted_map.mi:immut/sorted_map' -i '$MOON_HOME/lib/core/_build/native/release/bundle/immut/sorted_set/sorted_set.mi:immut/sorted_set' -i '$MOON_HOME/lib/core/_build/native/release/bundle/immut/vector/vector.mi:immut/vector' -i '$MOON_HOME/lib/core/_build/native/release/bundle/int/int.mi:int' -i '$MOON_HOME/lib/core/_build/native/release/bundle/int16/int16.mi:int16' -i '$MOON_HOME/lib/core/_build/native/release/bundle/int64/int64.mi:int64' -i '$MOON_HOME/lib/core/_build/native/release/bundle/json/json.mi:json' -i '$MOON_HOME/lib/core/_build/native/release/bundle/lazy/lazy.mi:lazy' -i '$MOON_HOME/lib/core/_build/native/release/bundle/lazy_list/lazy_list.mi:lazy_list' -i '$MOON_HOME/lib/core/_build/native/release/bundle/lexbuf/lexbuf.mi:lexbuf' -i '$MOON_HOME/lib/core/_build/native/release/bundle/list/list.mi:list' -i '$MOON_HOME/lib/core/_build/native/release/bundle/math/math.mi:math' -i '$MOON_HOME/lib/core/_build/native/release/bundle/option/option.mi:option' -i '$MOON_HOME/lib/core/_build/native/release/bundle/prelude/prelude.mi:prelude' -i '$MOON_HOME/lib/core/_build/native/release/bundle/priority_queue/priority_queue.mi:priority_queue' -i '$MOON_HOME/lib/core/_build/native/release/bundle/queue/queue.mi:queue' -i '$MOON_HOME/lib/core/_build/native/release/bundle/quickcheck/quickcheck.mi:quickcheck' -i '$MOON_HOME/lib/core/_build/native/release/bundle/random/random.mi:random' -i '$MOON_HOME/lib/core/_build/native/release/bundle/range/range.mi:range' -i '$MOON_HOME/lib/core/_build/native/release/bundle/ref/ref.mi:ref' -i '$MOON_HOME/lib/core/_build/native/release/bundle/result/result.mi:result' -i '$MOON_HOME/lib/core/_build/native/release/bundle/set/set.mi:set' -i '$MOON_HOME/lib/core/_build/native/release/bundle/sorted_map/sorted_map.mi:sorted_map' -i '$MOON_HOME/lib/core/_build/native/release/bundle/sorted_set/sorted_set.mi:sorted_set' -i '$MOON_HOME/lib/core/_build/native/release/bundle/quickcheck/splitmix/splitmix.mi:splitmix' -i '$MOON_HOME/lib/core/_build/native/release/bundle/strconv/strconv.mi:strconv' -i '$MOON_HOME/lib/core/_build/native/release/bundle/string/string.mi:string' -i '$MOON_HOME/lib/core/_build/native/release/bundle/test/test.mi:test' -i '$MOON_HOME/lib/core/_build/native/release/bundle/tuple/tuple.mi:tuple' -i '$MOON_HOME/lib/core/_build/native/release/bundle/uint/uint.mi:uint' -i '$MOON_HOME/lib/core/_build/native/release/bundle/uint16/uint16.mi:uint16' -i '$MOON_HOME/lib/core/_build/native/release/bundle/uint64/uint64.mi:uint64' -i '$MOON_HOME/lib/core/_build/native/release/bundle/unit/unit.mi:unit' -i '$MOON_HOME/lib/core/_build/native/release/bundle/encoding/utf16/utf16.mi:utf16' -i '$MOON_HOME/lib/core/_build/native/release/bundle/encoding/utf8/utf8.mi:utf8' -i '$MOON_HOME/lib/core/_build/native/release/bundle/v128/v128.mi:v128' -pkg-sources moon/test/single:. -target native -g -O0 -workspace-path . -all-pkgs ./_build/native/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/_build/native/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/native/release/bundle/core.core' ./_build/native/debug/build/single/single.core -main moon/test/single -o ./_build/native/debug/build/single/single.c -pkg-config-path ./moon.pkg.json -pkg-sources moon/test/single:. -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target native -g -O0
            cc -o ./_build/native/debug/build/runtime.o '-I$MOON_HOME/include' -g -c -fwrapv -fno-strict-aliasing -O2 -DMOONBIT_ALLOW_STACKTRACE -DMOONBIT_USE_SIMDUTF '$MOON_HOME/lib/runtime.c'
            cc -o ./_build/native/debug/build/single/single.exe '-I$MOON_HOME/include' -g -fwrapv -fno-strict-aliasing -Og '$MOON_HOME/lib/libmoonbitrun.o' ./_build/native/debug/build/single/single.c ./_build/native/debug/build/runtime.o '$MOON_HOME/lib/moonbit_simdutf.o' '$MOON_HOME/lib/simdutf.o' -lm '$MOON_HOME/lib/libbacktrace.a'
            ./_build/native/debug/build/single/single.exe
        "#]],
    );

    let output = get_stdout(
        &dir,
        [
            "run",
            "a/b/single.mbt",
            "--target",
            "native",
            "--dry-run",
            "--release",
        ],
    )
    // Normalize clang-only warnings to keep snapshots portable across macOS/Linux.
    .replace(" -Wno-unused-value", "");
    check(
        &output,
        expect![[r#"
            moonc build-package ./single.mbt -o ./_build/native/release/build/single/single.core -pkg moon/test/single -pkg-type executable -std-path '$MOON_HOME/lib/core/_build/native/release/bundle' -i '$MOON_HOME/lib/core/_build/native/release/bundle/abort/abort.mi:abort' -i '$MOON_HOME/lib/core/_build/native/release/bundle/argparse/argparse.mi:argparse' -i '$MOON_HOME/lib/core/_build/native/release/bundle/array/array.mi:array' -i '$MOON_HOME/lib/core/_build/native/release/bundle/encoding/ascii/ascii.mi:ascii' -i '$MOON_HOME/lib/core/_build/native/release/bundle/encoding/base64/base64.mi:base64' -i '$MOON_HOME/lib/core/_build/native/release/bundle/bench/bench.mi:bench' -i '$MOON_HOME/lib/core/_build/native/release/bundle/bigint/bigint.mi:bigint' -i '$MOON_HOME/lib/core/_build/native/release/bundle/bool/bool.mi:bool' -i '$MOON_HOME/lib/core/_build/native/release/bundle/buffer/buffer.mi:buffer' -i '$MOON_HOME/lib/core/_build/native/release/bundle/builtin/builtin.mi:builtin' -i '$MOON_HOME/lib/core/_build/native/release/bundle/byte/byte.mi:byte' -i '$MOON_HOME/lib/core/_build/native/release/bundle/bytes/bytes.mi:bytes' -i '$MOON_HOME/lib/core/_build/native/release/bundle/char/char.mi:char' -i '$MOON_HOME/lib/core/_build/native/release/bundle/cmp/cmp.mi:cmp' -i '$MOON_HOME/lib/core/_build/native/release/bundle/coverage/coverage.mi:coverage' -i '$MOON_HOME/lib/core/_build/native/release/bundle/debug/debug.mi:debug' -i '$MOON_HOME/lib/core/_build/native/release/bundle/deque/deque.mi:deque' -i '$MOON_HOME/lib/core/_build/native/release/bundle/double/double.mi:double' -i '$MOON_HOME/lib/core/_build/native/release/bundle/env/env.mi:env' -i '$MOON_HOME/lib/core/_build/native/release/bundle/error/error.mi:error' -i '$MOON_HOME/lib/core/_build/native/release/bundle/float/float.mi:float' -i '$MOON_HOME/lib/core/_build/native/release/bundle/hashmap/hashmap.mi:hashmap' -i '$MOON_HOME/lib/core/_build/native/release/bundle/hashset/hashset.mi:hashset' -i '$MOON_HOME/lib/core/_build/native/release/bundle/immut/array/array.mi:immut/array' -i '$MOON_HOME/lib/core/_build/native/release/bundle/immut/hashmap/hashmap.mi:immut/hashmap' -i '$MOON_HOME/lib/core/_build/native/release/bundle/immut/hashset/hashset.mi:immut/hashset' -i '$MOON_HOME/lib/core/_build/native/release/bundle/immut/priority_queue/priority_queue.mi:immut/priority_queue' -i '$MOON_HOME/lib/core/_build/native/release/bundle/immut/sorted_map/sorted_map.mi:immut/sorted_map' -i '$MOON_HOME/lib/core/_build/native/release/bundle/immut/sorted_set/sorted_set.mi:immut/sorted_set' -i '$MOON_HOME/lib/core/_build/native/release/bundle/immut/vector/vector.mi:immut/vector' -i '$MOON_HOME/lib/core/_build/native/release/bundle/int/int.mi:int' -i '$MOON_HOME/lib/core/_build/native/release/bundle/int16/int16.mi:int16' -i '$MOON_HOME/lib/core/_build/native/release/bundle/int64/int64.mi:int64' -i '$MOON_HOME/lib/core/_build/native/release/bundle/json/json.mi:json' -i '$MOON_HOME/lib/core/_build/native/release/bundle/lazy/lazy.mi:lazy' -i '$MOON_HOME/lib/core/_build/native/release/bundle/lazy_list/lazy_list.mi:lazy_list' -i '$MOON_HOME/lib/core/_build/native/release/bundle/lexbuf/lexbuf.mi:lexbuf' -i '$MOON_HOME/lib/core/_build/native/release/bundle/list/list.mi:list' -i '$MOON_HOME/lib/core/_build/native/release/bundle/math/math.mi:math' -i '$MOON_HOME/lib/core/_build/native/release/bundle/option/option.mi:option' -i '$MOON_HOME/lib/core/_build/native/release/bundle/prelude/prelude.mi:prelude' -i '$MOON_HOME/lib/core/_build/native/release/bundle/priority_queue/priority_queue.mi:priority_queue' -i '$MOON_HOME/lib/core/_build/native/release/bundle/queue/queue.mi:queue' -i '$MOON_HOME/lib/core/_build/native/release/bundle/quickcheck/quickcheck.mi:quickcheck' -i '$MOON_HOME/lib/core/_build/native/release/bundle/random/random.mi:random' -i '$MOON_HOME/lib/core/_build/native/release/bundle/range/range.mi:range' -i '$MOON_HOME/lib/core/_build/native/release/bundle/ref/ref.mi:ref' -i '$MOON_HOME/lib/core/_build/native/release/bundle/result/result.mi:result' -i '$MOON_HOME/lib/core/_build/native/release/bundle/set/set.mi:set' -i '$MOON_HOME/lib/core/_build/native/release/bundle/sorted_map/sorted_map.mi:sorted_map' -i '$MOON_HOME/lib/core/_build/native/release/bundle/sorted_set/sorted_set.mi:sorted_set' -i '$MOON_HOME/lib/core/_build/native/release/bundle/quickcheck/splitmix/splitmix.mi:splitmix' -i '$MOON_HOME/lib/core/_build/native/release/bundle/strconv/strconv.mi:strconv' -i '$MOON_HOME/lib/core/_build/native/release/bundle/string/string.mi:string' -i '$MOON_HOME/lib/core/_build/native/release/bundle/test/test.mi:test' -i '$MOON_HOME/lib/core/_build/native/release/bundle/tuple/tuple.mi:tuple' -i '$MOON_HOME/lib/core/_build/native/release/bundle/uint/uint.mi:uint' -i '$MOON_HOME/lib/core/_build/native/release/bundle/uint16/uint16.mi:uint16' -i '$MOON_HOME/lib/core/_build/native/release/bundle/uint64/uint64.mi:uint64' -i '$MOON_HOME/lib/core/_build/native/release/bundle/unit/unit.mi:unit' -i '$MOON_HOME/lib/core/_build/native/release/bundle/encoding/utf16/utf16.mi:utf16' -i '$MOON_HOME/lib/core/_build/native/release/bundle/encoding/utf8/utf8.mi:utf8' -i '$MOON_HOME/lib/core/_build/native/release/bundle/v128/v128.mi:v128' -pkg-sources moon/test/single:. -target native -workspace-path . -all-pkgs ./_build/native/release/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/_build/native/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/native/release/bundle/core.core' ./_build/native/release/build/single/single.core -main moon/test/single -o ./_build/native/release/build/single/single.c -pkg-config-path ./moon.pkg.json -pkg-sources moon/test/single:. -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target native
            cc -o ./_build/native/release/build/runtime.o '-I$MOON_HOME/include' -g -c -fwrapv -fno-strict-aliasing -O2 -DMOONBIT_USE_SIMDUTF '$MOON_HOME/lib/runtime.c'
            cc -o ./_build/native/release/build/single/single.exe '-I$MOON_HOME/include' -fwrapv -fno-strict-aliasing -O2 '$MOON_HOME/lib/libmoonbitrun.o' ./_build/native/release/build/single/single.c ./_build/native/release/build/runtime.o '$MOON_HOME/lib/moonbit_simdutf.o' '$MOON_HOME/lib/simdutf.o' -lm '$MOON_HOME/lib/libbacktrace.a'
            ./_build/native/release/build/single/single.exe
        "#]],
    );

    let output = get_stdout(
        &dir,
        [
            "run",
            "a/b/single.mbt",
            "--target",
            "js",
            "--build-only",
            "--dry-run",
        ],
    );
    check(
        &output,
        expect![[r#"
            moonc build-package ./single.mbt -o ./_build/js/debug/build/single/single.core -pkg moon/test/single -pkg-type executable -std-path '$MOON_HOME/lib/core/_build/js/release/bundle' -i '$MOON_HOME/lib/core/_build/js/release/bundle/abort/abort.mi:abort' -i '$MOON_HOME/lib/core/_build/js/release/bundle/argparse/argparse.mi:argparse' -i '$MOON_HOME/lib/core/_build/js/release/bundle/array/array.mi:array' -i '$MOON_HOME/lib/core/_build/js/release/bundle/encoding/ascii/ascii.mi:ascii' -i '$MOON_HOME/lib/core/_build/js/release/bundle/encoding/base64/base64.mi:base64' -i '$MOON_HOME/lib/core/_build/js/release/bundle/bench/bench.mi:bench' -i '$MOON_HOME/lib/core/_build/js/release/bundle/bigint/bigint.mi:bigint' -i '$MOON_HOME/lib/core/_build/js/release/bundle/bool/bool.mi:bool' -i '$MOON_HOME/lib/core/_build/js/release/bundle/buffer/buffer.mi:buffer' -i '$MOON_HOME/lib/core/_build/js/release/bundle/builtin/builtin.mi:builtin' -i '$MOON_HOME/lib/core/_build/js/release/bundle/byte/byte.mi:byte' -i '$MOON_HOME/lib/core/_build/js/release/bundle/bytes/bytes.mi:bytes' -i '$MOON_HOME/lib/core/_build/js/release/bundle/char/char.mi:char' -i '$MOON_HOME/lib/core/_build/js/release/bundle/cmp/cmp.mi:cmp' -i '$MOON_HOME/lib/core/_build/js/release/bundle/coverage/coverage.mi:coverage' -i '$MOON_HOME/lib/core/_build/js/release/bundle/debug/debug.mi:debug' -i '$MOON_HOME/lib/core/_build/js/release/bundle/deque/deque.mi:deque' -i '$MOON_HOME/lib/core/_build/js/release/bundle/double/double.mi:double' -i '$MOON_HOME/lib/core/_build/js/release/bundle/env/env.mi:env' -i '$MOON_HOME/lib/core/_build/js/release/bundle/error/error.mi:error' -i '$MOON_HOME/lib/core/_build/js/release/bundle/float/float.mi:float' -i '$MOON_HOME/lib/core/_build/js/release/bundle/hashmap/hashmap.mi:hashmap' -i '$MOON_HOME/lib/core/_build/js/release/bundle/hashset/hashset.mi:hashset' -i '$MOON_HOME/lib/core/_build/js/release/bundle/immut/array/array.mi:immut/array' -i '$MOON_HOME/lib/core/_build/js/release/bundle/immut/hashmap/hashmap.mi:immut/hashmap' -i '$MOON_HOME/lib/core/_build/js/release/bundle/immut/hashset/hashset.mi:immut/hashset' -i '$MOON_HOME/lib/core/_build/js/release/bundle/immut/priority_queue/priority_queue.mi:immut/priority_queue' -i '$MOON_HOME/lib/core/_build/js/release/bundle/immut/sorted_map/sorted_map.mi:immut/sorted_map' -i '$MOON_HOME/lib/core/_build/js/release/bundle/immut/sorted_set/sorted_set.mi:immut/sorted_set' -i '$MOON_HOME/lib/core/_build/js/release/bundle/immut/vector/vector.mi:immut/vector' -i '$MOON_HOME/lib/core/_build/js/release/bundle/int/int.mi:int' -i '$MOON_HOME/lib/core/_build/js/release/bundle/int16/int16.mi:int16' -i '$MOON_HOME/lib/core/_build/js/release/bundle/int64/int64.mi:int64' -i '$MOON_HOME/lib/core/_build/js/release/bundle/json/json.mi:json' -i '$MOON_HOME/lib/core/_build/js/release/bundle/lazy/lazy.mi:lazy' -i '$MOON_HOME/lib/core/_build/js/release/bundle/lazy_list/lazy_list.mi:lazy_list' -i '$MOON_HOME/lib/core/_build/js/release/bundle/lexbuf/lexbuf.mi:lexbuf' -i '$MOON_HOME/lib/core/_build/js/release/bundle/list/list.mi:list' -i '$MOON_HOME/lib/core/_build/js/release/bundle/math/math.mi:math' -i '$MOON_HOME/lib/core/_build/js/release/bundle/option/option.mi:option' -i '$MOON_HOME/lib/core/_build/js/release/bundle/prelude/prelude.mi:prelude' -i '$MOON_HOME/lib/core/_build/js/release/bundle/priority_queue/priority_queue.mi:priority_queue' -i '$MOON_HOME/lib/core/_build/js/release/bundle/queue/queue.mi:queue' -i '$MOON_HOME/lib/core/_build/js/release/bundle/quickcheck/quickcheck.mi:quickcheck' -i '$MOON_HOME/lib/core/_build/js/release/bundle/random/random.mi:random' -i '$MOON_HOME/lib/core/_build/js/release/bundle/range/range.mi:range' -i '$MOON_HOME/lib/core/_build/js/release/bundle/ref/ref.mi:ref' -i '$MOON_HOME/lib/core/_build/js/release/bundle/result/result.mi:result' -i '$MOON_HOME/lib/core/_build/js/release/bundle/set/set.mi:set' -i '$MOON_HOME/lib/core/_build/js/release/bundle/sorted_map/sorted_map.mi:sorted_map' -i '$MOON_HOME/lib/core/_build/js/release/bundle/sorted_set/sorted_set.mi:sorted_set' -i '$MOON_HOME/lib/core/_build/js/release/bundle/quickcheck/splitmix/splitmix.mi:splitmix' -i '$MOON_HOME/lib/core/_build/js/release/bundle/strconv/strconv.mi:strconv' -i '$MOON_HOME/lib/core/_build/js/release/bundle/string/string.mi:string' -i '$MOON_HOME/lib/core/_build/js/release/bundle/test/test.mi:test' -i '$MOON_HOME/lib/core/_build/js/release/bundle/tuple/tuple.mi:tuple' -i '$MOON_HOME/lib/core/_build/js/release/bundle/uint/uint.mi:uint' -i '$MOON_HOME/lib/core/_build/js/release/bundle/uint16/uint16.mi:uint16' -i '$MOON_HOME/lib/core/_build/js/release/bundle/uint64/uint64.mi:uint64' -i '$MOON_HOME/lib/core/_build/js/release/bundle/unit/unit.mi:unit' -i '$MOON_HOME/lib/core/_build/js/release/bundle/encoding/utf16/utf16.mi:utf16' -i '$MOON_HOME/lib/core/_build/js/release/bundle/encoding/utf8/utf8.mi:utf8' -i '$MOON_HOME/lib/core/_build/js/release/bundle/v128/v128.mi:v128' -pkg-sources moon/test/single:. -target js -g -O0 -source-map -workspace-path . -all-pkgs ./_build/js/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/_build/js/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/js/release/bundle/core.core' ./_build/js/debug/build/single/single.core -main moon/test/single -o ./_build/js/debug/build/single/single.js -pkg-config-path ./moon.pkg.json -pkg-sources moon/test/single:. -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target js -g -O0 -source-map
            node --enable-source-maps ./_build/js/debug/build/single/single.js
        "#]],
    );

    let output = get_stdout(
        &dir,
        ["run", "a/b/single.mbt", "--target", "wasm-gc", "--dry-run"],
    );
    check(
        &output,
        expect![[r#"
            moonc build-package ./single.mbt -o ./_build/wasm-gc/debug/build/single/single.core -pkg moon/test/single -pkg-type executable -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.mi:abort' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/argparse/argparse.mi:argparse' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/array/array.mi:array' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/encoding/ascii/ascii.mi:ascii' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/encoding/base64/base64.mi:base64' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/bench/bench.mi:bench' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/bigint/bigint.mi:bigint' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/bool/bool.mi:bool' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/buffer/buffer.mi:buffer' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/builtin/builtin.mi:builtin' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/byte/byte.mi:byte' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/bytes/bytes.mi:bytes' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/char/char.mi:char' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/cmp/cmp.mi:cmp' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/coverage/coverage.mi:coverage' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/debug/debug.mi:debug' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/deque/deque.mi:deque' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/double/double.mi:double' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/env/env.mi:env' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/error/error.mi:error' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/float/float.mi:float' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/hashmap/hashmap.mi:hashmap' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/hashset/hashset.mi:hashset' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/immut/array/array.mi:immut/array' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/immut/hashmap/hashmap.mi:immut/hashmap' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/immut/hashset/hashset.mi:immut/hashset' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/immut/priority_queue/priority_queue.mi:immut/priority_queue' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/immut/sorted_map/sorted_map.mi:immut/sorted_map' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/immut/sorted_set/sorted_set.mi:immut/sorted_set' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/immut/vector/vector.mi:immut/vector' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/int/int.mi:int' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/int16/int16.mi:int16' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/int64/int64.mi:int64' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/json/json.mi:json' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/lazy/lazy.mi:lazy' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/lazy_list/lazy_list.mi:lazy_list' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/lexbuf/lexbuf.mi:lexbuf' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/list/list.mi:list' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/math/math.mi:math' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/option/option.mi:option' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/priority_queue/priority_queue.mi:priority_queue' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/queue/queue.mi:queue' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/quickcheck/quickcheck.mi:quickcheck' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/random/random.mi:random' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/range/range.mi:range' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/ref/ref.mi:ref' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/result/result.mi:result' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/set/set.mi:set' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/sorted_map/sorted_map.mi:sorted_map' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/sorted_set/sorted_set.mi:sorted_set' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/quickcheck/splitmix/splitmix.mi:splitmix' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/strconv/strconv.mi:strconv' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/string/string.mi:string' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/test/test.mi:test' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/tuple/tuple.mi:tuple' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/uint/uint.mi:uint' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/uint16/uint16.mi:uint16' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/uint64/uint64.mi:uint64' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/unit/unit.mi:unit' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/encoding/utf16/utf16.mi:utf16' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/encoding/utf8/utf8.mi:utf8' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/v128/v128.mi:v128' -pkg-sources moon/test/single:. -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/build/single/single.core -main moon/test/single -o ./_build/wasm-gc/debug/build/single/single.wasm -pkg-config-path ./moon.pkg.json -pkg-sources moon/test/single:. -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm-gc -g -O0 -source-map
            moonrun ./_build/wasm-gc/debug/build/single/single.wasm --
        "#]],
    );

    let output = get_stdout(
        &dir,
        ["run", "a/b/single.mbt", "--target", "js", "--dry-run"],
    );
    check(
        &output,
        expect![[r#"
            moonc build-package ./single.mbt -o ./_build/js/debug/build/single/single.core -pkg moon/test/single -pkg-type executable -std-path '$MOON_HOME/lib/core/_build/js/release/bundle' -i '$MOON_HOME/lib/core/_build/js/release/bundle/abort/abort.mi:abort' -i '$MOON_HOME/lib/core/_build/js/release/bundle/argparse/argparse.mi:argparse' -i '$MOON_HOME/lib/core/_build/js/release/bundle/array/array.mi:array' -i '$MOON_HOME/lib/core/_build/js/release/bundle/encoding/ascii/ascii.mi:ascii' -i '$MOON_HOME/lib/core/_build/js/release/bundle/encoding/base64/base64.mi:base64' -i '$MOON_HOME/lib/core/_build/js/release/bundle/bench/bench.mi:bench' -i '$MOON_HOME/lib/core/_build/js/release/bundle/bigint/bigint.mi:bigint' -i '$MOON_HOME/lib/core/_build/js/release/bundle/bool/bool.mi:bool' -i '$MOON_HOME/lib/core/_build/js/release/bundle/buffer/buffer.mi:buffer' -i '$MOON_HOME/lib/core/_build/js/release/bundle/builtin/builtin.mi:builtin' -i '$MOON_HOME/lib/core/_build/js/release/bundle/byte/byte.mi:byte' -i '$MOON_HOME/lib/core/_build/js/release/bundle/bytes/bytes.mi:bytes' -i '$MOON_HOME/lib/core/_build/js/release/bundle/char/char.mi:char' -i '$MOON_HOME/lib/core/_build/js/release/bundle/cmp/cmp.mi:cmp' -i '$MOON_HOME/lib/core/_build/js/release/bundle/coverage/coverage.mi:coverage' -i '$MOON_HOME/lib/core/_build/js/release/bundle/debug/debug.mi:debug' -i '$MOON_HOME/lib/core/_build/js/release/bundle/deque/deque.mi:deque' -i '$MOON_HOME/lib/core/_build/js/release/bundle/double/double.mi:double' -i '$MOON_HOME/lib/core/_build/js/release/bundle/env/env.mi:env' -i '$MOON_HOME/lib/core/_build/js/release/bundle/error/error.mi:error' -i '$MOON_HOME/lib/core/_build/js/release/bundle/float/float.mi:float' -i '$MOON_HOME/lib/core/_build/js/release/bundle/hashmap/hashmap.mi:hashmap' -i '$MOON_HOME/lib/core/_build/js/release/bundle/hashset/hashset.mi:hashset' -i '$MOON_HOME/lib/core/_build/js/release/bundle/immut/array/array.mi:immut/array' -i '$MOON_HOME/lib/core/_build/js/release/bundle/immut/hashmap/hashmap.mi:immut/hashmap' -i '$MOON_HOME/lib/core/_build/js/release/bundle/immut/hashset/hashset.mi:immut/hashset' -i '$MOON_HOME/lib/core/_build/js/release/bundle/immut/priority_queue/priority_queue.mi:immut/priority_queue' -i '$MOON_HOME/lib/core/_build/js/release/bundle/immut/sorted_map/sorted_map.mi:immut/sorted_map' -i '$MOON_HOME/lib/core/_build/js/release/bundle/immut/sorted_set/sorted_set.mi:immut/sorted_set' -i '$MOON_HOME/lib/core/_build/js/release/bundle/immut/vector/vector.mi:immut/vector' -i '$MOON_HOME/lib/core/_build/js/release/bundle/int/int.mi:int' -i '$MOON_HOME/lib/core/_build/js/release/bundle/int16/int16.mi:int16' -i '$MOON_HOME/lib/core/_build/js/release/bundle/int64/int64.mi:int64' -i '$MOON_HOME/lib/core/_build/js/release/bundle/json/json.mi:json' -i '$MOON_HOME/lib/core/_build/js/release/bundle/lazy/lazy.mi:lazy' -i '$MOON_HOME/lib/core/_build/js/release/bundle/lazy_list/lazy_list.mi:lazy_list' -i '$MOON_HOME/lib/core/_build/js/release/bundle/lexbuf/lexbuf.mi:lexbuf' -i '$MOON_HOME/lib/core/_build/js/release/bundle/list/list.mi:list' -i '$MOON_HOME/lib/core/_build/js/release/bundle/math/math.mi:math' -i '$MOON_HOME/lib/core/_build/js/release/bundle/option/option.mi:option' -i '$MOON_HOME/lib/core/_build/js/release/bundle/prelude/prelude.mi:prelude' -i '$MOON_HOME/lib/core/_build/js/release/bundle/priority_queue/priority_queue.mi:priority_queue' -i '$MOON_HOME/lib/core/_build/js/release/bundle/queue/queue.mi:queue' -i '$MOON_HOME/lib/core/_build/js/release/bundle/quickcheck/quickcheck.mi:quickcheck' -i '$MOON_HOME/lib/core/_build/js/release/bundle/random/random.mi:random' -i '$MOON_HOME/lib/core/_build/js/release/bundle/range/range.mi:range' -i '$MOON_HOME/lib/core/_build/js/release/bundle/ref/ref.mi:ref' -i '$MOON_HOME/lib/core/_build/js/release/bundle/result/result.mi:result' -i '$MOON_HOME/lib/core/_build/js/release/bundle/set/set.mi:set' -i '$MOON_HOME/lib/core/_build/js/release/bundle/sorted_map/sorted_map.mi:sorted_map' -i '$MOON_HOME/lib/core/_build/js/release/bundle/sorted_set/sorted_set.mi:sorted_set' -i '$MOON_HOME/lib/core/_build/js/release/bundle/quickcheck/splitmix/splitmix.mi:splitmix' -i '$MOON_HOME/lib/core/_build/js/release/bundle/strconv/strconv.mi:strconv' -i '$MOON_HOME/lib/core/_build/js/release/bundle/string/string.mi:string' -i '$MOON_HOME/lib/core/_build/js/release/bundle/test/test.mi:test' -i '$MOON_HOME/lib/core/_build/js/release/bundle/tuple/tuple.mi:tuple' -i '$MOON_HOME/lib/core/_build/js/release/bundle/uint/uint.mi:uint' -i '$MOON_HOME/lib/core/_build/js/release/bundle/uint16/uint16.mi:uint16' -i '$MOON_HOME/lib/core/_build/js/release/bundle/uint64/uint64.mi:uint64' -i '$MOON_HOME/lib/core/_build/js/release/bundle/unit/unit.mi:unit' -i '$MOON_HOME/lib/core/_build/js/release/bundle/encoding/utf16/utf16.mi:utf16' -i '$MOON_HOME/lib/core/_build/js/release/bundle/encoding/utf8/utf8.mi:utf8' -i '$MOON_HOME/lib/core/_build/js/release/bundle/v128/v128.mi:v128' -pkg-sources moon/test/single:. -target js -g -O0 -source-map -workspace-path . -all-pkgs ./_build/js/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/_build/js/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/js/release/bundle/core.core' ./_build/js/debug/build/single/single.core -main moon/test/single -o ./_build/js/debug/build/single/single.js -pkg-config-path ./moon.pkg.json -pkg-sources moon/test/single:. -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target js -g -O0 -source-map
            node --enable-source-maps ./_build/js/debug/build/single/single.js
        "#]],
    );

    let output = get_stdout(
        &dir,
        ["run", "a/b/single.mbt", "--target", "js", "--build-only"],
    );
    check(
        &output,
        expect![[r#"
            {"artifacts_path":["$ROOT/a/b/_build/js/debug/build/single/single.js"]}
        "#]],
    );
    assert!(
        dir.join("a/b/_build/js/debug/build/single/single.js")
            .exists()
    );
}

#[test]
fn test_moon_run_single_mbt_file() {
    let dir = TestDir::new("run_single_mbt_file.in");

    let output = get_stdout(&dir, ["run", "a/b/single.mbt"]);
    check(
        &output,
        expect![[r#"
        I am OK
    "#]],
    );

    let output = get_stdout(&dir.join("a").join("b").join("c"), ["run", "../single.mbt"]);
    check(
        &output,
        expect![[r#"
            I am OK
            "#]],
    );

    let output = get_stdout(
        &dir.join("a").join("b"),
        ["run", "single.mbt", "--target", "js"],
    );
    check(
        &output,
        expect![[r#"
        I am OK
        "#]],
    );

    let output = get_stdout(
        &dir.join("a").join("b"),
        ["run", "single.mbt", "--target", "native"],
    );
    // cl have other output
    assert!(output.contains("I am OK"));
}

#[test]
fn test_moon_run_single_mbt_file_inside_a_pkg() {
    let dir = TestDir::new("run_single_mbt_file_inside_pkg.in");

    let output = get_stdout(&dir, ["run", "main/main.mbt"]);
    check(
        &output,
        expect![[r#"
            Hello, world!!!
            root main
        "#]],
    );
    let output = get_stdout(&dir, ["run", "lib/main_in_lib/main.mbt"]);
    check(
        &output,
        expect![[r#"
            Hello, world!!!
            main in lib
        "#]],
    );

    let output = get_stdout(&dir.join("lib"), ["run", "../main/main.mbt"]);
    check(
        &output,
        expect![[r#"
            Hello, world!!!
            root main
        "#]],
    );
    let output = get_stdout(&dir.join("lib"), ["run", "main_in_lib/main.mbt"]);
    check(
        &output,
        expect![[r#"
            Hello, world!!!
            main in lib
        "#]],
    );

    let output = get_stdout(
        &dir.join("lib").join("main_in_lib"),
        ["run", "../../main/main.mbt"],
    );
    check(
        &output,
        expect![[r#"
            Hello, world!!!
            root main
        "#]],
    );
    let output = get_stdout(&dir.join("lib").join("main_in_lib"), ["run", "main.mbt"]);
    check(
        &output,
        expect![[r#"
            Hello, world!!!
            main in lib
        "#]],
    );

    let output = get_stdout(
        &dir.join("lib").join("main_in_lib"),
        ["run", "../../main/main.mbt", "--target", "native"],
    );
    check(
        &output,
        expect![[r#"
            Hello, world!!!
            root main
        "#]],
    );
    let output = get_stdout(
        &dir.join("lib").join("main_in_lib"),
        ["run", "main.mbt", "--target", "native"],
    );
    check(
        &output,
        expect![[r#"
            Hello, world!!!
            main in lib
        "#]],
    );
}

#[test]
#[ignore = "There's conflict between base64 in core and base64 in x"]
fn moon_check_and_test_single_file() {
    let dir = TestDir::new("moon_test_single_file.in");
    let single_mbt = dir.join("single.mbt").display().to_string();
    let single_mbt_md = dir.join("111.mbt.md").display().to_string();

    // .mbt
    {
        // rel path
        check(
            get_stdout(&dir, ["test", "single.mbt", "-i", "0"]),
            expect![[r#"
                ------------------ 11111111 ------------------
                Total tests: 1, passed: 1, failed: 0.
            "#]],
        );
        check(
            get_err_stdout(&dir, ["test", "single.mbt", "-i", "1"]),
            expect![[r#"
                [moon/test] test single/single.mbt:12 (#1) failed
                expect test failed at $ROOT/single.mbt:13:3-13:18
                Diff: (- expected, + actual)
                ----
                +234523
                ----

                Total tests: 1, passed: 0, failed: 1.
            "#]],
        );
        // abs path
        check(
            get_stdout(&dir, ["test", &single_mbt, "-i", "0"]),
            expect![[r#"
                ------------------ 11111111 ------------------
                Total tests: 1, passed: 1, failed: 0.
            "#]],
        );
        let s = get_stdout(&dir, ["test", &single_mbt, "-i", "1", "-u"]);
        let exp = r#"
------------------ 22222222 ------------------
Total tests: 1, passed: 1, failed: 0.
"#
        .trim();
        assert!(
            s.contains(exp),
            "output did not contain expected updated test output"
        ); // FIXME: this is because different versions have different output during update expect

        check(
            get_stderr(&dir, ["check", "single.mbt"]),
            expect![[r#"
                Warning: [0002]
                   ╭─[ $ROOT/single.mbt:8:7 ]
                   │
                 8 │   let single_mbt = 1
                   │       ─────┬────  
                   │            ╰────── Warning (unused_value): Unused variable 'single_mbt'
                ───╯
                Finished. moon: ran 2 tasks, now up to date (1 warnings, 0 errors)
            "#]],
        );
        // abs path
        check(
            get_stderr(&dir, ["check", &single_mbt]),
            expect![[r#"
                Warning: [0002]
                   ╭─[ $ROOT/single.mbt:8:7 ]
                   │
                 8 │   let single_mbt = 1
                   │       ─────┬────  
                   │            ╰────── Warning (unused_value): Unused variable 'single_mbt'
                ───╯
                Finished. moon: ran 1 task, now up to date (1 warnings, 0 errors)
            "#]],
        );
    }

    // .mbt.md
    {
        check(
            get_stdout(&dir, ["test", "222.mbt.md"]),
            expect![[r#"
                222
                Total tests: 1, passed: 1, failed: 0.
            "#]],
        );

        // rel path
        let s = get_stdout(&dir, ["test", "111.mbt.md", "-i", "0"]);
        assert!(
            s.contains("111"),
            "output did not contain expected test output"
        );

        check(
            get_err_stdout(&dir, ["test", "111.mbt.md", "-i", "1"]),
            expect![[r#"
                [moon/test] test single/111.mbt.md:27 (#1) failed
                expect test failed at $ROOT/111.mbt.md:34:5-34:20
                Diff: (- expected, + actual)
                ----
                +234523
                ----

                Total tests: 1, passed: 0, failed: 1.
            "#]],
        );
        // abs path
        check(
            get_stdout(&dir, ["test", &single_mbt_md, "-i", "0"]),
            expect![[r#"
                111
                Total tests: 1, passed: 1, failed: 0.
            "#]],
        );

        let s = get_stdout(&dir, ["test", &single_mbt_md, "-i", "1", "-u"]);
        assert!(
            s.contains("222"),
            "output did not contain expected updated test output"
        );
        assert!(
            s.contains("Total tests: 1, passed: 1, failed: 0."),
            "output did not contain expected updated test output"
        );

        // rel path
        snapbox::assert_data_eq!(
            get_stderr(&dir, ["check", "111.mbt.md"]),
            snapbox::str!(
                r#"
Warning: [0002]
    ╭─[ $ROOT/111.mbt.md:28:9 ]
    │
 28 │     let single_mbt_md = 1
    │         ──────┬──────  
    │               ╰──────── Warning (unused_value): Unused variable 'single_mbt_md'
────╯
..."#
            )
        );

        // abs path
        check(
            get_stderr(&dir, ["check", &single_mbt_md]),
            expect![[r#"
                Warning: [0002]
                    ╭─[ $ROOT/111.mbt.md:28:9 ]
                    │
                 28 │     let single_mbt_md = 1
                    │         ──────┬──────  
                    │               ╰──────── Warning (unused_value): Unused variable 'single_mbt_md'
                ────╯
                Finished. moon: ran 1 task, now up to date (1 warnings, 0 errors)
            "#]],
        );
    }

    // check single file (with or without main func)
    {
        let with_main = dir.join("with_main.mbt").display().to_string();
        snapbox::assert_data_eq!(
            get_stderr(&dir, ["check", &with_main]),
            snapbox::str![[r#"
Warning: [0002]
   ╭─[ $ROOT/with_main.mbt:2:7 ]
   │
 2 │   let with_main = 1
   │       ────┬────  
   │           ╰────── Warning (unused_value): Unused variable 'with_main'
───╯
...
"#]],
        );
        let without_main = dir.join("without_main.mbt").display().to_string();
        snapbox::assert_data_eq!(
            get_stderr(&dir, ["check", &without_main]),
            snapbox::str![[r#"
Warning: [0001]
   ╭─[ $ROOT/without_main.mbt:1:4 ]
   │
 1 │ fn func() -> Unit {
   │    ──┬─  
   │      ╰─── Warning (unused_value): Unused function 'func'
───╯
Warning: [0002]
   ╭─[ $ROOT/without_main.mbt:2:7 ]
   │
 2 │   let without_main = 1
   │       ──────┬─────  
   │             ╰─────── Warning (unused_value): Unused variable 'without_main'
───╯
...
"#]],
        );
    }
}

/// Test that single-file commands properly report errors for non-existent files
/// instead of panicking (issue #1192)
#[test]
fn test_single_file_nonexistent_path_error() {
    // Use temp_dir for cross-platform compatibility
    let temp_dir = std::env::temp_dir();
    let nonexistent_path = std::env::temp_dir()
        .join("nonexistent_file_12345.mbt")
        .display()
        .to_string();

    // Test moon check with non-existent file outside any project
    // Should fail gracefully (exit != 101 which is Rust panic code)
    let check_result = moon_cmd(&temp_dir)
        .args(["check", &nonexistent_path])
        .assert()
        .failure();
    // Verify it's not a panic (exit code 101)
    assert_ne!(
        check_result.get_output().status.code(),
        Some(101),
        "moon check should not panic for non-existent file"
    );

    // Test moon test with non-existent file outside any project
    let test_result = moon_cmd(&temp_dir)
        .args(["test", &nonexistent_path])
        .assert()
        .failure();
    // Verify it's not a panic (exit code 101)
    assert_ne!(
        test_result.get_output().status.code(),
        Some(101),
        "moon test should not panic for non-existent file"
    );

    // Test moon run with non-existent file outside any project
    let run_result = moon_cmd(&temp_dir)
        .args(["run", &nonexistent_path])
        .assert()
        .failure();
    // Verify it's not a panic (exit code 101)
    assert_ne!(
        run_result.get_output().status.code(),
        Some(101),
        "moon run should not panic for non-existent file"
    );
}

#[test]
fn test_single_file_commands_work_with_workspace_disabled() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();

    std::fs::write(
        dir.join("hello.mbt"),
        r#"fn main {
  println("hello")
}
"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("test.mbt"),
        r#"test "x" {
  assert_true(true)
}
"#,
    )
    .unwrap();

    let check_result = moon_cmd(&dir)
        .env(MOON_NO_WORKSPACE, "1")
        .args(["check", "hello.mbt"])
        .assert()
        .success()
        .get_output()
        .stderr
        .clone();
    check(
        String::from_utf8(check_result).unwrap(),
        expect![[r#"
            Warning: `MOON_NO_WORKSPACE` is deprecated. Use `MOON_WORK=off` to disable workspace mode.
            Finished. moon: ran 2 tasks, now up to date
        "#]],
    );

    check(
        get_stdout_with_envs(&dir, ["test", "test.mbt"], [(MOON_NO_WORKSPACE, "1")]),
        expect![[r#"
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout_with_envs(&dir, ["run", "hello.mbt"], [(MOON_NO_WORKSPACE, "1")]),
        expect![[r#"
            hello
        "#]],
    );

    check(
        get_stdout_with_envs(&dir, ["run", "hello.mbt"], [(MOON_WORK_ENV, "off")]),
        expect![[r#"
            hello
        "#]],
    );
}
