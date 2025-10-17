const tag = new WebAssembly.Tag({ parameters: [] });
const console = {
    elog: (x) => console_elog(x),
    log: (x) => console_log(x),
};
let memory;
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
    "wasi_snapshot_preview1": {
        "random_get": (buf, buf_len) => {
            get_random_values(new Uint8Array(memory.buffer, buf, buf_len));
            return 0;
        }
    },
    "wasi:random/insecure-seed@0.2.0": {
        "insecure-seed": (addr) => {
            get_random_values(new Uint8Array(memory.buffer, addr, 16));
        }
    },
    "moonbit:ffi": {
        "insecure_seed": () => {
            let bytes = new BigUint64Array(1);
            get_random_values(new Uint8Array(bytes.buffer));
            return bytes[0];
        }
    }
};

try {
    if (typeof bytes === 'undefined') {
        bytes = read_file_to_bytes(module_name);
    }
    let module = new WebAssembly.Module(bytes, { builtins: ['js-string'], importedStringConstants: "_" });
    let instance = new WebAssembly.Instance(module, spectest);
    memory = instance.exports.memory;
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
