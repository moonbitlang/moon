const tag = new WebAssembly.Tag({ parameters: [] });
const console = {
    elog: (x) => console_elog(x),
    log: (x) => console_log(x),
};
const ffiBytesMemory = new WebAssembly.Memory({ initial: 1});
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
