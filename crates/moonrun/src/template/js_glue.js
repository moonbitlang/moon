// Unified JS runtime glue for moonrun.
// Sections: JS helper API, sys API wiring, WASM instantiation.

const __moonbit_fs_unstable =
    globalThis.__moonbit_fs_unstable ||
    (globalThis.__moonbit_fs_unstable = {});
// JS helper API attached to __moonbit_fs_unstable.
(function init_js_api(obj) {
    // String ops
    function begin_create_string() {
        return { s: "" }
    }

    function string_append_char(handle, ch) {
        handle.s += String.fromCharCode(ch)
    }

    function finish_create_string(handle) {
        return handle.s
    }

    function begin_read_string(s) {
        return { s: s, i: 0 }
    }

    function string_read_char(handle) {
        if (handle.i >= handle.s.length) {
            return -1
        }
        return handle.s.charCodeAt(handle.i++)
    }

    function finish_read_string(handle) {
        return
    }

    function begin_read_byte_array(arr) {
        return { arr: arr, i: 0 }
    }

    function byte_array_read_byte(handle) {
        if (handle.i >= handle.arr.length) {
            return -1
        }
        return handle.arr[handle.i++]
    }

    function finish_read_byte_array(handle) {
        return
    }

    function begin_create_byte_array() {
        return { arr: [] }
    }

    function byte_array_append_byte(handle, byte) {
        handle.arr.push(byte)
    }

    function finish_create_byte_array(handle) {
        return new Uint8Array(handle.arr)
    }

    function begin_read_string_array(arr) {
        return { arr: arr, i: 0 }
    }

    function string_array_read_string(handle) {
        if (handle.i >= handle.arr.length) {
            return "ffi_end_of_/string_array"
        }
        return handle.arr[handle.i++]
    }

    function finish_read_string_array(handle) {
        return
    }

    // Array ops
    function array_len(arr) {
        return arr.length
    }

    function array_get(arr, idx) {
        return arr[idx]
    }

    // JSValue
    function jsvalue_is_string(v) {
        return typeof v === "string"
    }

    obj.begin_create_string = begin_create_string
    obj.string_append_char = string_append_char
    obj.finish_create_string = finish_create_string
    obj.begin_read_string = begin_read_string
    obj.string_read_char = string_read_char
    obj.finish_read_string = finish_read_string

    obj.begin_read_byte_array = begin_read_byte_array
    obj.byte_array_read_byte = byte_array_read_byte
    obj.finish_read_byte_array = finish_read_byte_array
    obj.begin_create_byte_array = begin_create_byte_array
    obj.byte_array_append_byte = byte_array_append_byte
    obj.finish_create_byte_array = finish_create_byte_array

    obj.begin_read_string_array = begin_read_string_array
    obj.string_array_read_string = string_array_read_string
    obj.finish_read_string_array = finish_read_string_array

    obj.array_len = array_len
    obj.array_get = array_get

    obj.jsvalue_is_string = jsvalue_is_string
})(__moonbit_fs_unstable);

const __moonbit_wasi_unstable = globalThis.__moonbit_wasi_unstable || {};
const __moonrun_v8_import = globalThis.__moonrun_v8_import || {};
const moonbitlang_async = globalThis["moonbitlang/async"] || {};
const moonbitlang_sqlite = globalThis["moonbitlang/sqlite"] || {};
const moonbit_ffi_memory_sanitizer =
    globalThis["moonbit:ffi/memory-sanitizer"] || {};

function demangleMangledFunctionName(funcName) {
    if (typeof __moonbit_demangle_mangled_function_name === "function") {
        return __moonbit_demangle_mangled_function_name(funcName);
    }
    return funcName;
}

const BASE64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const BASE64_INDEX = (() => {
    const map = Object.create(null);
    for (let i = 0; i < BASE64.length; i++) {
        map[BASE64[i]] = i;
    }
    return map;
})();

function decodeVLQSegment(seg) {
    let i = 0;
    const out = [];
    while (i < seg.length) {
        let value = 0;
        let shift = 0;
        let cont = false;
        do {
            if (i >= seg.length) return null;
            const digit = BASE64_INDEX[seg[i]];
            if (digit === undefined) return null;
            i += 1;
            cont = (digit & 32) !== 0;
            value |= (digit & 31) << shift;
            shift += 5;
        } while (cont);
        const neg = (value & 1) !== 0;
        value >>= 1;
        out.push(neg ? -value : value);
    }
    return out;
}

function parseWasmSourceMap(rawMap) {
    if (!rawMap || typeof rawMap.mappings !== "string" || !Array.isArray(rawMap.sources)) {
        return null;
    }
    const text = rawMap.mappings;
    const mappings = [];
    let generatedLine = 0;
    let generatedColumn = 0;
    let source = 0;
    let originalLine = 0;
    let originalColumn = 0;

    let i = 0;
    while (i < text.length) {
        const ch = text[i];
        if (ch === ";") {
            generatedLine += 1;
            generatedColumn = 0;
            i += 1;
            continue;
        }
        if (ch === ",") {
            i += 1;
            continue;
        }

        let j = i;
        while (j < text.length && text[j] !== "," && text[j] !== ";") {
            j += 1;
        }
        const seg = decodeVLQSegment(text.slice(i, j));
        if (!seg || seg.length < 1) return null;

        generatedColumn += seg[0];
        if (seg.length >= 4) {
            source += seg[1];
            originalLine += seg[2];
            originalColumn += seg[3];
            // moon_wat2wasm encodes address into generated column.
            if (generatedLine === 0) {
                mappings.push({
                    addr: generatedColumn,
                    source,
                    line: originalLine + 1,
                    col: originalColumn + 1,
                });
            }
        }
        i = j;
    }
    return { sources: rawMap.sources, mappings };
}

let parsedSourceMap = null;
try {
    if (typeof source_map === "string") {
        parsedSourceMap = parseWasmSourceMap(JSON.parse(source_map));
    }
} catch (_) {
    parsedSourceMap = null;
}

function sourcePosForOffset(offset) {
    const sm = parsedSourceMap;
    if (!sm || !Array.isArray(sm.mappings) || sm.mappings.length === 0) {
        return null;
    }

    let lo = 0;
    let hi = sm.mappings.length - 1;
    let best = -1;
    while (lo <= hi) {
        const mid = (lo + hi) >> 1;
        if (sm.mappings[mid].addr <= offset) {
            best = mid;
            lo = mid + 1;
        } else {
            hi = mid - 1;
        }
    }
    if (best < 0) return null;

    const m = sm.mappings[best];
    if (m.source < 0 || m.source >= sm.sources.length) {
        return null;
    }
    const sourceFile = sm.sources[m.source];
    return `${sourceFile}:${m.line}`;
}

function sourcePosForWasmLocation(location) {
    const m = /:0x([0-9a-fA-F]+)\s*$/.exec(location);
    if (!m) return null;
    const offset = parseInt(m[1], 16);
    if (!Number.isFinite(offset)) return null;
    return sourcePosForOffset(offset);
}

function formatStackLine(line) {
    // Typical v8 frame: "    at <func> (<loc>)"
    const withLoc = line.match(/^(\s*)at\s+(.+?)(\s+\((.*)\)\s*)$/);
    if (withLoc) {
        const fn = demangleMangledFunctionName(withLoc[2]);
        const srcPos = sourcePosForWasmLocation(withLoc[4]);
        const src = srcPos ? ` ${srcPos}` : "";
        return `${withLoc[1]}at ${fn}${src}`;
    }

    // Fallback: "    at <func>"
    const noLoc = line.match(/^(\s*)at\s+(\S+)(\s*)$/);
    if (noLoc) {
        const fn = demangleMangledFunctionName(noLoc[2]);
        return `${noLoc[1]}at ${fn}${noLoc[3]}`;
    }
    return line;
}

// WASM instantiation + test harness.
const tag = new WebAssembly.Tag({ parameters: [] });
const console = {
    elog: (x) => console_elog(x),
    log: (x) => console_log(x),
};
const ffiBytesMemory = new WebAssembly.Memory({ initial: 1 });
const wasi_snapshot_preview1 = __moonbit_wasi_unstable;
const spectest = {
    spectest: {
        print_char: (x) => print(x),
        read_char: () => read_char(),
    },
    __moonbit_fs_unstable: __moonbit_fs_unstable,
    __moonbit_rand_unstable: __moonbit_rand_unstable,
    __moonbit_io_unstable: __moonbit_io_unstable,
    __moonbit_sys_unstable: __moonbit_sys_unstable,
    __moonbit_time_unstable: __moonbit_time_unstable,
    "moonbitlang/async": moonbitlang_async,
    "moonbitlang/sqlite": moonbitlang_sqlite,
    "moonbit:ffi/memory-sanitizer": moonbit_ffi_memory_sanitizer,
    wasi_snapshot_preview1: wasi_snapshot_preview1,
    moonbit: {
        string_to_js_string() {
            print(arguments[0]);
        }
    },
    exception: {
        tag: tag,
        throw: () => {
            throw new WebAssembly.Exception(tag, [], { traceStack: true })
        },
    },
    console: {
        log: (x) => console.log(x),
    },
    "ffi-bytes": {
        from_memory: (offset, length) => new Uint8Array(ffiBytesMemory.buffer.slice(offset, offset + length)),
        new: (length) => new Uint8Array(length),
        get: (bytes, index) => bytes[index],
        set: (bytes, index, value) => bytes[index] = value,
        copy: (dst, dst_off, src, src_off, len) => dst.set(src.subarray(src_off, src_off + len), dst_off),
        fill: (bytes, start, value, len) => bytes.fill(value, start, start + len),
        length: (bytes) => bytes.length,
        equals: (a, b) => a.length === b.length && a.every((val, index) => val === b[index]) ? 1 : 0,
        asString: (bytes, start, len) => String.fromCharCode(...bytes.subarray(start, start + len).reduce((acc, byte, i) => i % 2 == 0 ? [...acc, byte | (bytes[i + start + 1] << 8)] : acc, [])),
        memory: ffiBytesMemory,
    },
};

try {
    if (!(module instanceof WebAssembly.Module)) {
        throw new Error(`failed to load compiled wasm module: ${module_name}`);
    }
    let instance = new WebAssembly.Instance(module, spectest);
    const memory = instance.exports.memory;
    if (memory instanceof WebAssembly.Memory) {
        __moonrun_v8_import.bind_memory(memory);
    }
    if (test_mode) {
        for (param of testParams) {
            try {
                instance.exports.moonbit_test_driver_internal_execute(param[0], parseInt(param[1]));
            } catch (e) {
                const stack = e && e.stack ? e.stack.toString() : String(e);
                const formatted = stack.split('\n').map(formatStackLine).join('\n');
                console.log("----- BEGIN MOON TEST RESULT -----")
                console.log(JSON.stringify({
                    type: "result",
                    file: param[0],
                    index: parseInt(param[1]),
                    message: formatted,
                }));
                console.log("----- END MOON TEST RESULT -----")
            }
        }
        instance.exports.moonbit_test_driver_finish();
    }
    else {
        if (instance.exports._start) {
            instance.exports._start();
        }
    }
}
catch (e) {
    const stack = e && e.stack ? e.stack.toString() : String(e);
    for (const line of stack.split('\n')) {
        const formatted = formatStackLine(line);
        if (!line.includes(BUILTIN_SCRIPT_ORIGIN_PREFIX)) {
            console.elog(formatted);
        }
        if (no_stack_trace) {
            break;
        }
    }
    __moonbit_sys_unstable.exit(1);

}
