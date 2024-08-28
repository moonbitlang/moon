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
    Instant: {
        now() {
            return instant_now();
        },
        elapsed_as_secs_f64(instant) {
            return instant_elapsed_as_secs_f64(instant);
        }
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
    test: {
        get_file_name: () => globalThis.testParams.fileName,
        get_index: () => globalThis.testParams.index,
    }
};

try {
    let instance = new WebAssembly.Instance(module, spectest);
    if (instance.exports._start) {
        if (test_mode) {
            for (param of test_params) {
                try {
                    globalThis.testParams = {
                        fileName: param[0],
                        index: parseInt(param[1])
                    };
                    instance.exports._start();
                } catch (e) {
                    console.log("----- BEGIN MOON TEST RESULT -----")
                    console.log(`{"package": "${package}", "filename": "${param[0]}", "index": "${param[1]}", "test_name": "${param[1]}", "message": "${e.stack.toString().split('\n').join('\\n')}"}`);
                    console.log("----- END MOON TEST RESULT -----")
                }
            }
        }
        else {
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
