// Unified JS runtime glue for moonrun.
// Sections: JS helper API, sys API wiring, WASM instantiation.

const __moonbit_fs_unstable =
    globalThis.__moonbit_fs_unstable ||
    (globalThis.__moonbit_fs_unstable = {});
// Provided by Rust in `sys_api::init_env`; fallback keeps interactive tests safe.
const __moonbit_run_env = globalThis.__moonbit_run_env || {
    env_vars: new Map(),
    args: [],
};

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

// Sys API wiring (env vars + args).
(function init_sys_api(obj, run_env) {
    // Return the value of the environment variable
    function env_get_var(name) {
        return run_env.env_vars.get(name) || ""
    }

    // Get the list of arguments passed to the program
    function args_get() {
        return run_env.args
    }

    obj.env_get_var = env_get_var
    obj.args_get = args_get
})(__moonbit_fs_unstable, __moonbit_run_env);

delete globalThis.__moonbit_run_env;

// WASM instantiation + test harness.
const tag = new WebAssembly.Tag({ parameters: [] });
const console = {
    elog: (x) => console_elog(x),
    log: (x) => console_log(x),
};
const ffiBytesMemory = new WebAssembly.Memory({ initial: 1 });
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
    if (typeof bytes === 'undefined') {
        bytes = read_file_to_bytes(module_name);
    }
    let module = new WebAssembly.Module(bytes, { builtins: ['js-string'], importedStringConstants: "_" });
    let instance = new WebAssembly.Instance(module, spectest);
    if (test_mode) {
        for (param of testParams) {
            try {
                instance.exports.moonbit_test_driver_internal_execute(param[0], parseInt(param[1]));
            } catch (e) {
                console.log("----- BEGIN MOON TEST RESULT -----")
                console.log(`{"package": "${packageName}", "filename": "${param[0]}", "index": "${param[1]}", "test_name": "${param[1]}", "message": "${e.stack.toString().replaceAll("\\", "\\\\").split('\n').join('\\n')}"}`);
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
    for (const line of e.stack.toString().split('\n')) {
        if (!line.includes(BUILTIN_SCRIPT_ORIGIN_PREFIX)) {
            console.elog(line);
        }
        if (no_stack_trace) {
            break;
        }
    }
    __moonbit_sys_unstable.exit(1);

}
