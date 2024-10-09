const tag = new WebAssembly.Tag({ parameters: [] });
const console = {
    elog: (x) => console_elog(x),
    log: (x) => console_log(x),
};
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
};

try {
    let bytes = read_file_to_bytes(module_name);
    let module = new WebAssembly.Module(bytes, { builtins: ['js-string'], importedStringConstants: "moonbit:constant_strings" });
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
