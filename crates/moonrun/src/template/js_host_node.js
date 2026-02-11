// @ts-check
// Host adapter and CLI bootstrap for running moonrun JS glue on Node.js.

const process = require("node:process");
const __moonrun_node_child_process = require("node:child_process");
const __moonrun_node_fs = require("node:fs");
const __moonrun_node_path = require("node:path");

const MOONRUN_BUILTIN_SCRIPT_ORIGIN_PREFIX = "__$moonrun_v8_builtin_script$__";
const MOONRUN_NODE_STACK_SIZE_ENV = "__MOONRUN_NODE_STACK_SIZE_APPLIED";
const MOONRUN_MIN_NODE_MAJOR = 11;

/**
 * @typedef {{
 *   path: string | null,
 *   args: string[],
 *   no_stack_trace: boolean,
 *   test_args: string | null,
 *   stack_size: string | null,
 *   interactive: boolean,
 *   help: boolean,
 *   version: boolean
 * }} MoonrunCliOpts
 */

/**
 * @typedef {{
 *   packageName: string,
 *   testParams: Array<[string, string]>
 * }} MoonrunTestPlan
 */

function moonrun_print_usage() {
    const lines = [
        "Usage: moonrun.js [OPTIONS] <path> [-- <args>...]",
        "",
        "Options:",
        "  --no-stack-trace",
        "  --test-args <json>",
        "  --stack-size <bytes>",
        "  -i, --interactive",
        "  -h, --help",
        "  -V, --version",
    ];
    process.stdout.write(`${lines.join("\n")}\n`);
}

function moonrun_fail(msg) {
    process.stderr.write(`${msg}\n`);
    process.exit(1);
}

function moonrun_check_node_version() {
    const version = process.versions && process.versions.node;
    if (typeof version !== "string") {
        moonrun_fail("failed to detect Node.js version");
    }
    const major = Number.parseInt(version.split(".")[0], 10);
    if (!Number.isFinite(major) || major < MOONRUN_MIN_NODE_MAJOR) {
        moonrun_fail(
            `moonrun.js requires Node.js >= ${MOONRUN_MIN_NODE_MAJOR}, got ${version}`
        );
    }
}

/**
 * Parse an option value in `--opt=value` or `--opt value` form.
 * @param {string} arg
 * @param {string[]} argv
 * @param {number} i
 * @returns {[string, number]}
 */
function moonrun_get_option_value(arg, argv, i) {
    const eq = arg.indexOf("=");
    if (eq >= 0) {
        return [arg.slice(eq + 1), i + 1];
    }
    if (i + 1 >= argv.length) {
        moonrun_fail(`missing option value for ${arg}`);
    }
    return [argv[i + 1], i + 2];
}

/**
 * @param {string[]} argv
 * @returns {MoonrunCliOpts}
 */
function moonrun_parse_cli(argv) {
    const opts = {
        path: null,
        args: [],
        no_stack_trace: false,
        test_args: null,
        stack_size: null,
        interactive: false,
        help: false,
        version: false,
    };

    let i = 0;
    let passthrough = false;
    while (i < argv.length) {
        const arg = argv[i];

        if (passthrough) {
            opts.args.push(arg);
            i += 1;
            continue;
        }

        if (arg === "--") {
            passthrough = true;
            i += 1;
            continue;
        }

        if (arg === "-h" || arg === "--help") {
            opts.help = true;
            i += 1;
            continue;
        }
        if (arg === "-V" || arg === "--version") {
            opts.version = true;
            i += 1;
            continue;
        }
        if (arg === "-i" || arg === "--interactive") {
            opts.interactive = true;
            i += 1;
            continue;
        }
        if (arg === "--no-stack-trace") {
            opts.no_stack_trace = true;
            i += 1;
            continue;
        }
        if (arg === "--test-args" || arg.startsWith("--test-args=")) {
            const parsed = moonrun_get_option_value(arg, argv, i);
            opts.test_args = parsed[0];
            i = parsed[1];
            continue;
        }
        if (arg === "--stack-size" || arg.startsWith("--stack-size=")) {
            const parsed = moonrun_get_option_value(arg, argv, i);
            opts.stack_size = parsed[0];
            i = parsed[1];
            continue;
        }

        if (arg.startsWith("-")) {
            if (opts.path !== null) {
                opts.args.push(arg);
                i += 1;
                continue;
            }
            moonrun_fail(`unknown option: ${arg}`);
        }

        if (opts.path === null) {
            opts.path = arg;
        } else {
            opts.args.push(arg);
        }
        i += 1;
    }
    return opts;
}

/**
 * @param {string | null} raw
 * @returns {MoonrunTestPlan}
 */
function moonrun_parse_test_params(raw) {
    if (raw === null) {
        return {
            packageName: "",
            testParams: [],
        };
    }
    let parsed;
    try {
        parsed = JSON.parse(raw);
    } catch (e) {
        moonrun_fail(`failed to parse --test-args: ${e.message}`);
    }

    const packageName = typeof parsed.package === "string" ? parsed.package : "";
    const testParams = [];
    const fileAndIndex = Array.isArray(parsed.file_and_index)
        ? parsed.file_and_index
        : [];
    for (const pair of fileAndIndex) {
        if (!Array.isArray(pair) || pair.length < 2) {
            continue;
        }
        const file = String(pair[0]);
        const ranges = Array.isArray(pair[1]) ? pair[1] : [];
        for (const range of ranges) {
            const start = Number(range.start);
            const end = Number(range.end);
            if (!Number.isFinite(start) || !Number.isFinite(end)) {
                continue;
            }
            for (let idx = start; idx < end; idx += 1) {
                testParams.push([file, String(idx)]);
            }
        }
    }

    return { packageName, testParams };
}

function moonrun_resolve_source_map_path(wasmPath, sourceMapPath) {
    if (typeof sourceMapPath !== "string" || sourceMapPath.length === 0) {
        return "";
    }
    if (__moonrun_node_path.isAbsolute(sourceMapPath)) {
        return sourceMapPath;
    }
    return __moonrun_node_path.join(
        __moonrun_node_path.dirname(wasmPath),
        sourceMapPath
    );
}

const moonrun_opts = moonrun_parse_cli(process.argv.slice(2));
moonrun_check_node_version();

if (moonrun_opts.help) {
    moonrun_print_usage();
    process.exit(0);
}

if (moonrun_opts.version) {
    process.stdout.write("moonrun.js (node runtime)\n");
    process.exit(0);
}

if (moonrun_opts.interactive) {
    moonrun_fail("interactive mode is not supported by moonrun.js yet");
}

if (moonrun_opts.stack_size !== null && process.env[MOONRUN_NODE_STACK_SIZE_ENV] !== "1") {
    const env = { ...process.env, [MOONRUN_NODE_STACK_SIZE_ENV]: "1" };
    const child_args = [
        `--stack-size=${moonrun_opts.stack_size}`,
        ...process.argv.slice(1),
    ];
    const result = __moonrun_node_child_process.spawnSync(process.execPath, child_args, {
        stdio: "inherit",
        env,
    });
    if (result.error) {
        moonrun_fail(result.error.message);
    }
    process.exit(result.status === null ? 1 : result.status);
}

if (moonrun_opts.path === null) {
    moonrun_fail("no such file");
}

if (!__moonrun_node_fs.existsSync(moonrun_opts.path)) {
    moonrun_fail("no such file");
}

if (__moonrun_node_path.extname(moonrun_opts.path) !== ".wasm") {
    moonrun_fail("Unsupported file type");
}

const moonrun_test = moonrun_parse_test_params(moonrun_opts.test_args);

globalThis.__moonrun_launch = {
    BUILTIN_SCRIPT_ORIGIN_PREFIX: MOONRUN_BUILTIN_SCRIPT_ORIGIN_PREFIX,
    module_name: moonrun_opts.path,
    no_stack_trace: moonrun_opts.no_stack_trace,
    test_mode: moonrun_opts.test_args !== null,
    testParams: moonrun_test.testParams,
    packageName: moonrun_test.packageName,
};

const moonrun_env_vars = new Map();
for (const [k, v] of Object.entries(process.env)) {
    moonrun_env_vars.set(k, v ?? "");
}
globalThis.__moonbit_run_env = {
    env_vars: moonrun_env_vars,
    args: [moonrun_opts.path, ...moonrun_opts.args],
    stderr_is_tty: !!process.stderr.isTTY,
};
globalThis.__moonbit_backtrace_runtime = {
    resolve_source_map_path: moonrun_resolve_source_map_path,
};

let moonrun_stdin_buffer = null;
let moonrun_stdin_byte_idx = 0;

function moonrun_load_stdin_buffer() {
    if (moonrun_stdin_buffer === null) {
        moonrun_stdin_buffer = __moonrun_node_fs.readFileSync(0);
    }
    return moonrun_stdin_buffer;
}

function moonrun_read_char() {
    const buffer = moonrun_load_stdin_buffer();
    if (moonrun_stdin_byte_idx >= buffer.length) {
        return -1;
    }

    const first = buffer[moonrun_stdin_byte_idx];
    let num_bytes;
    if (first <= 0x7f) {
        num_bytes = 1;
    } else if (first >= 0xc0 && first <= 0xdf) {
        num_bytes = 2;
    } else if (first >= 0xe0 && first <= 0xef) {
        num_bytes = 3;
    } else if (first >= 0xf0 && first <= 0xf7) {
        num_bytes = 4;
    } else {
        moonrun_stdin_byte_idx += 1;
        return -1;
    }

    const end = moonrun_stdin_byte_idx + num_bytes;
    if (end > buffer.length) {
        moonrun_stdin_byte_idx = buffer.length;
        return -1;
    }

    let cp;
    if (num_bytes === 1) {
        cp = first;
    } else if (num_bytes === 2) {
        const b1 = buffer[moonrun_stdin_byte_idx + 1];
        if ((b1 & 0xc0) !== 0x80) {
            moonrun_stdin_byte_idx = end;
            return -1;
        }
        cp = ((first & 0x1f) << 6) | (b1 & 0x3f);
        if (cp < 0x80) {
            moonrun_stdin_byte_idx = end;
            return -1;
        }
    } else if (num_bytes === 3) {
        const b1 = buffer[moonrun_stdin_byte_idx + 1];
        const b2 = buffer[moonrun_stdin_byte_idx + 2];
        if ((b1 & 0xc0) !== 0x80 || (b2 & 0xc0) !== 0x80) {
            moonrun_stdin_byte_idx = end;
            return -1;
        }
        cp = ((first & 0x0f) << 12) | ((b1 & 0x3f) << 6) | (b2 & 0x3f);
        if (cp < 0x800 || (cp >= 0xd800 && cp <= 0xdfff)) {
            moonrun_stdin_byte_idx = end;
            return -1;
        }
    } else {
        const b1 = buffer[moonrun_stdin_byte_idx + 1];
        const b2 = buffer[moonrun_stdin_byte_idx + 2];
        const b3 = buffer[moonrun_stdin_byte_idx + 3];
        if ((b1 & 0xc0) !== 0x80 || (b2 & 0xc0) !== 0x80 || (b3 & 0xc0) !== 0x80) {
            moonrun_stdin_byte_idx = end;
            return -1;
        }
        cp = ((first & 0x07) << 18) | ((b1 & 0x3f) << 12) | ((b2 & 0x3f) << 6) | (b3 & 0x3f);
        if (cp < 0x10000 || cp > 0x10ffff) {
            moonrun_stdin_byte_idx = end;
            return -1;
        }
    }

    moonrun_stdin_byte_idx = end;
    return cp;
}

const moonrun_pending_high_surrogate = {
    stdout: null,
    stderr: null,
};

function moonrun_flush_pending_surrogate(stream_key) {
    if (moonrun_pending_high_surrogate[stream_key] === null) {
        return "";
    }
    moonrun_pending_high_surrogate[stream_key] = null;
    return "\uFFFD";
}

function moonrun_char_from_u32(stream_key, c) {
    const pending_high = moonrun_pending_high_surrogate[stream_key];
    if (!Number.isInteger(c) || c < 0 || c > 0x10ffff) {
        const prefix = pending_high === null ? "" : moonrun_flush_pending_surrogate(stream_key);
        return prefix + "\uFFFD";
    }

    // Handle UTF-16 surrogate code units emitted across adjacent write_char/print calls.
    if (c >= 0xd800 && c <= 0xdbff) {
        const prefix = pending_high === null ? "" : moonrun_flush_pending_surrogate(stream_key);
        moonrun_pending_high_surrogate[stream_key] = c;
        return prefix;
    }
    if (c >= 0xdc00 && c <= 0xdfff) {
        if (pending_high === null) {
            return "\uFFFD";
        }
        moonrun_pending_high_surrogate[stream_key] = null;
        return String.fromCharCode(pending_high, c);
    }
    const prefix = pending_high === null ? "" : moonrun_flush_pending_surrogate(stream_key);
    if (c <= 0xffff) {
        return prefix + String.fromCharCode(c);
    }
    return prefix + String.fromCodePoint(c);
}

function moonrun_write_char_to_stream(stream_key, c) {
    const out = moonrun_char_from_u32(stream_key, c);
    if (out.length === 0) {
        return;
    }
    if (stream_key === "stdout") {
        process.stdout.write(out);
    } else {
        process.stderr.write(out);
    }
}

function moonrun_write_string_to_stream(stream_key, text) {
    const pending = moonrun_flush_pending_surrogate(stream_key);
    const out = pending + text;
    if (stream_key === "stdout") {
        process.stdout.write(out);
    } else {
        process.stderr.write(out);
    }
}

process.on("beforeExit", () => {
    const stdout_pending = moonrun_flush_pending_surrogate("stdout");
    if (stdout_pending.length > 0) {
        process.stdout.write(stdout_pending);
    }
    const stderr_pending = moonrun_flush_pending_surrogate("stderr");
    if (stderr_pending.length > 0) {
        process.stderr.write(stderr_pending);
    }
});

globalThis.print = function print(x) {
    if (typeof x === "number") {
        moonrun_write_char_to_stream("stdout", x);
        return;
    }
    moonrun_write_string_to_stream("stdout", String(x));
};

globalThis.console_log = function console_log(x) {
    moonrun_write_string_to_stream("stdout", `${String(x)}\n`);
};

globalThis.console_elog = function console_elog(x) {
    moonrun_write_string_to_stream("stderr", `${String(x)}\n`);
};

globalThis.read_char = function read_char() {
    return moonrun_read_char();
};

globalThis.read_file_to_bytes = function read_file_to_bytes(path) {
    const buf = __moonrun_node_fs.readFileSync(path);
    return new Uint8Array(buf);
};

globalThis.__moonrun_decode_utf8 = function __moonrun_decode_utf8(bytes) {
    const data = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
    return Buffer.from(data).toString("utf8");
};

const moonrun_io = globalThis.__moonbit_io_unstable || (globalThis.__moonbit_io_unstable = {});
moonrun_io.read_bytes_from_stdin = function read_bytes_from_stdin() {
    const buffer = moonrun_load_stdin_buffer();
    if (moonrun_stdin_byte_idx >= buffer.length) {
        return new Uint8Array(0);
    }
    const bytes = buffer.subarray(moonrun_stdin_byte_idx);
    moonrun_stdin_byte_idx = buffer.length;
    return new Uint8Array(bytes);
};
moonrun_io.read_char = function read_char_api() {
    return moonrun_read_char();
};
moonrun_io.write_char = function write_char(fd, c) {
    if (fd === 1) {
        moonrun_write_char_to_stream("stdout", c);
    } else if (fd === 2) {
        moonrun_write_char_to_stream("stderr", c);
    }
};
moonrun_io.flush = function flush(_fd) {};

const moonrun_time = globalThis.__moonbit_time_unstable || (globalThis.__moonbit_time_unstable = {});
moonrun_time.instant_now = function instant_now() {
    return process.hrtime.bigint();
};
moonrun_time.instant_elapsed_as_secs_f64 = function instant_elapsed_as_secs_f64(start) {
    const delta = process.hrtime.bigint() - start;
    return Number(delta) / 1_000_000_000;
};
moonrun_time.now = function now() {
    return [Date.now()];
};

const moonrun_rng = globalThis.__moonbit_rand_unstable || (globalThis.__moonbit_rand_unstable = {});
moonrun_rng.stdrng_seed_from_u64 = function stdrng_seed_from_u64(seed) {
    return { state: BigInt.asUintN(64, BigInt(seed >>> 0)) };
};
moonrun_rng.stdrng_gen_range = function stdrng_gen_range(rng, ubound) {
    if (ubound <= 0) {
        return 0;
    }
    rng.state = BigInt.asUintN(
        64,
        rng.state * 6364136223846793005n + 1442695040888963407n
    );
    return Number((rng.state >> 33n) % BigInt(ubound));
};

const moonrun_sys = globalThis.__moonbit_sys_unstable || (globalThis.__moonbit_sys_unstable = {});
moonrun_sys.exit = function exit(code) {
    process.exit(Number(code) | 0);
};
moonrun_sys.is_windows = function is_windows() {
    return process.platform === "win32" ? 1 : 0;
};

const moonrun_fs = globalThis.__moonbit_fs_unstable || (globalThis.__moonbit_fs_unstable = {});
const moonrun_fs_state = {
    file_content: new Uint8Array(0),
    dir_files: [],
    error_message: "",
};

function moonrun_to_uint8array(contents) {
    if (contents instanceof Uint8Array) {
        return contents;
    }
    if (Buffer.isBuffer(contents)) {
        return new Uint8Array(contents);
    }
    if (Array.isArray(contents)) {
        return new Uint8Array(contents);
    }
    if (contents instanceof ArrayBuffer) {
        return new Uint8Array(contents);
    }
    return new Uint8Array(0);
}

function moonrun_remove_dir_impl(path) {
    if (!__moonrun_node_fs.statSync(path).isDirectory()) {
        throw new Error(`Not a directory: ${path}`);
    }
    __moonrun_node_fs.rmSync(path, { recursive: true, force: false });
}

moonrun_fs.read_file_to_string = function read_file_to_string(path) {
    try {
        return __moonrun_node_fs.readFileSync(path, "utf8");
    } catch (_) {
        throw new Error(`Failed to read file: ${path}`);
    }
};
moonrun_fs.write_string_to_file = function write_string_to_file(path, contents) {
    try {
        __moonrun_node_fs.writeFileSync(path, contents);
    } catch (_) {
        throw new Error(`Failed to write file: ${path}`);
    }
};
moonrun_fs.write_bytes_to_file = function write_bytes_to_file(path, contents) {
    try {
        const bytes = moonrun_to_uint8array(contents);
        __moonrun_node_fs.writeFileSync(path, Buffer.from(bytes));
    } catch (_) {
        throw new Error(`Failed to write file: ${path}`);
    }
};
moonrun_fs.create_dir = function create_dir(path) {
    try {
        __moonrun_node_fs.mkdirSync(path, { recursive: true });
    } catch (_) {
        throw new Error(`Failed to create directory: ${path}`);
    }
};
moonrun_fs.read_dir = function read_dir(path) {
    try {
        return __moonrun_node_fs.readdirSync(path);
    } catch (_) {
        throw new Error(`Failed to read directory: ${path}`);
    }
};
moonrun_fs.is_file = function is_file(path) {
    return __moonrun_node_fs.existsSync(path) && __moonrun_node_fs.statSync(path).isFile();
};
moonrun_fs.is_dir = function is_dir(path) {
    return __moonrun_node_fs.existsSync(path) && __moonrun_node_fs.statSync(path).isDirectory();
};
moonrun_fs.remove_file = function remove_file(path) {
    try {
        __moonrun_node_fs.rmSync(path, { force: false });
    } catch (_) {
        throw new Error(`Failed to remove file: ${path}`);
    }
};
moonrun_fs.remove_dir = function remove_dir(path) {
    try {
        moonrun_remove_dir_impl(path);
    } catch (_) {
        throw new Error(`Failed to remove directory: ${path}`);
    }
};
moonrun_fs.path_exists = function path_exists(path) {
    return __moonrun_node_fs.existsSync(path);
};
moonrun_fs.current_dir = function current_dir() {
    return process.cwd();
};
moonrun_fs.read_file_to_bytes_new = function read_file_to_bytes_new(path) {
    try {
        moonrun_fs_state.file_content = new Uint8Array(__moonrun_node_fs.readFileSync(path));
        return 0;
    } catch (e) {
        moonrun_fs_state.error_message = `Failed to read file ${path}: ${e}`;
        return -1;
    }
};
moonrun_fs.write_bytes_to_file_new = function write_bytes_to_file_new(path, contents) {
    try {
        __moonrun_node_fs.writeFileSync(path, Buffer.from(moonrun_to_uint8array(contents)));
        return 0;
    } catch (e) {
        moonrun_fs_state.error_message = `Failed to write file ${path}: ${e}`;
        return -1;
    }
};
moonrun_fs.get_file_content = function get_file_content() {
    return moonrun_fs_state.file_content;
};
moonrun_fs.get_dir_files = function get_dir_files() {
    return moonrun_fs_state.dir_files;
};
moonrun_fs.get_error_message = function get_error_message() {
    return moonrun_fs_state.error_message;
};
moonrun_fs.create_dir_new = function create_dir_new(path) {
    try {
        __moonrun_node_fs.mkdirSync(path, { recursive: true });
        return 0;
    } catch (e) {
        moonrun_fs_state.error_message = `Failed to create directory ${path}: ${e}`;
        return -1;
    }
};
moonrun_fs.read_dir_new = function read_dir_new(path) {
    try {
        moonrun_fs_state.dir_files = __moonrun_node_fs.readdirSync(path);
        return 0;
    } catch (e) {
        moonrun_fs_state.error_message = `Failed to read directory ${path}: ${e}`;
        return -1;
    }
};
moonrun_fs.is_file_new = function is_file_new(path) {
    try {
        return __moonrun_node_fs.statSync(path).isFile() ? 1 : 0;
    } catch (e) {
        moonrun_fs_state.error_message = `${e}: ${path}`;
        return -1;
    }
};
moonrun_fs.is_dir_new = function is_dir_new(path) {
    try {
        return __moonrun_node_fs.statSync(path).isDirectory() ? 1 : 0;
    } catch (e) {
        moonrun_fs_state.error_message = `${e}: ${path}`;
        return -1;
    }
};
moonrun_fs.remove_file_new = function remove_file_new(path) {
    try {
        __moonrun_node_fs.rmSync(path, { force: false });
        return 0;
    } catch (e) {
        moonrun_fs_state.error_message = `Failed to remove file ${path}: ${e}`;
        return -1;
    }
};
moonrun_fs.remove_dir_new = function remove_dir_new(path) {
    try {
        moonrun_remove_dir_impl(path);
        return 0;
    } catch (e) {
        moonrun_fs_state.error_message = `Failed to remove directory ${path}: ${e}`;
        return -1;
    }
};
moonrun_fs.set_env_var = function set_env_var(key, value) {
    process.env[String(key)] = String(value);
};
moonrun_fs.unset_env_var = function unset_env_var(key) {
    delete process.env[String(key)];
};
moonrun_fs.get_env_vars = function get_env_vars() {
    const result = [];
    for (const [k, v] of Object.entries(process.env)) {
        result.push(k, v ?? "");
    }
    return result;
};
moonrun_fs.get_env_var = function get_env_var(key) {
    return process.env[String(key)] ?? "";
};
moonrun_fs.get_env_var_exists = function get_env_var_exists(key) {
    return process.env[String(key)] !== undefined;
};
