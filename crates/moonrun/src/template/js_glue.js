// Unified JS runtime glue for moonrun.
// Sections: JS helper API, sys API wiring, WASM instantiation.

const __moonbit_fs_unstable =
    globalThis.__moonbit_fs_unstable ||
    (globalThis.__moonbit_fs_unstable = {});
const __moonbit_backtrace_unstable =
    globalThis.__moonbit_backtrace_unstable ||
    (globalThis.__moonbit_backtrace_unstable = {});
// Provided by Rust in `sys_api::init_env`; fallback keeps interactive tests safe.
const __moonbit_run_env = globalThis.__moonbit_run_env || {
    env_vars: new Map(),
    args: [],
    backtrace_color_enabled: false,
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

function isDigit(ch) {
    return ch >= "0" && ch <= "9";
}

function parseU32(s, i) {
    if (i >= s.length || !isDigit(s[i])) {
        return null;
    }
    let v = 0;
    while (i < s.length && isDigit(s[i])) {
        v = v * 10 + (s.charCodeAt(i) - 48);
        i += 1;
    }
    return [v, i];
}

function hexValue(ch) {
    const code = ch.charCodeAt(0);
    if (code >= 48 && code <= 57) return code - 48;
    if (code >= 97 && code <= 102) return 10 + code - 97;
    if (code >= 65 && code <= 70) return 10 + code - 65;
    return -1;
}

function parseIdentifier(s, i) {
    const parsedLen = parseU32(s, i);
    if (!parsedLen) return null;
    const [n, start] = parsedLen;
    if (start + n > s.length) return null;

    const raw = s.slice(start, start + n);
    let out = "";
    for (let k = 0; k < raw.length; k++) {
        const c = raw[k];
        if (c !== "_") {
            out += c;
            continue;
        }
        if (k + 1 >= raw.length) return null;
        const next = raw[k + 1];
        if (next === "_") {
            out += "_";
            k += 1;
            continue;
        }
        if (k + 2 >= raw.length) return null;
        const hi = hexValue(raw[k + 1]);
        const lo = hexValue(raw[k + 2]);
        if (hi < 0 || lo < 0) return null;
        out += String.fromCharCode((hi << 4) | lo);
        k += 2;
    }

    return [out, start + n];
}

function parsePackageSegments(s, i, count) {
    const segs = [];
    for (let idx = 0; idx < count; idx++) {
        const seg = parseIdentifier(s, i);
        if (!seg) return null;
        segs.push(seg[0]);
        i = seg[1];
    }
    return [segs.join("/"), i];
}

function parsePackage(s, i) {
    if (s[i] !== "P") return null;
    i += 1;

    const countStart = i;
    const parsed = parseU32(s, i);
    if (!parsed) return null;
    let [count, j] = parsed;
    const pkg = parsePackageSegments(s, j, count);
    if (pkg) return pkg;

    // Backward-compatible fallback: single-digit package segment count.
    i = countStart;
    if (i >= s.length || !isDigit(s[i])) return null;
    count = s.charCodeAt(i) - 48;
    i += 1;
    return parsePackageSegments(s, i, count);
}

function isCorePackage(pkg) {
    if (!pkg) return false;
    const prefix = "moonbitlang/core";
    return pkg === prefix || pkg.startsWith(`${prefix}/`);
}

function stripSuffix(s, suffix) {
    return s.endsWith(suffix) ? s.slice(0, -suffix.length) : s;
}

function appendTypePath(s, i, omitCorePrefix) {
    const pkgParsed = parsePackage(s, i);
    if (!pkgParsed) return null;
    let [pkg, j] = pkgParsed;

    const typeParsed = parseIdentifier(s, j);
    if (!typeParsed) return null;
    let [typeName, k] = typeParsed;

    if (k < s.length && s[k] === "L") {
        const localParsed = parseIdentifier(s, k + 1);
        if (!localParsed) return null;
        typeName = `${typeName}.${localParsed[0]}`;
        k = localParsed[1];
    }

    if (omitCorePrefix && isCorePackage(pkg)) {
        pkg = "";
    }

    const out = `@${pkg ? `${pkg}.` : ""}${typeName}`;
    return [out, k];
}

function parseTypeArg(s, i) {
    if (i >= s.length) return null;
    const c = s[i];
    switch (c) {
        case "i": return ["Int", i + 1];
        case "l": return ["Int64", i + 1];
        case "h": return ["Int16", i + 1];
        case "j": return ["UInt", i + 1];
        case "k": return ["UInt16", i + 1];
        case "m": return ["UInt64", i + 1];
        case "d": return ["Double", i + 1];
        case "f": return ["Float", i + 1];
        case "b": return ["Bool", i + 1];
        case "c": return ["Char", i + 1];
        case "s": return ["String", i + 1];
        case "u": return ["Unit", i + 1];
        case "y": return ["Byte", i + 1];
        case "z": return ["Bytes", i + 1];
        case "A": {
            const inner = parseTypeArg(s, i + 1);
            if (!inner) return null;
            return [`FixedArray[${inner[0]}]`, inner[1]];
        }
        case "O": {
            const inner = parseTypeArg(s, i + 1);
            if (!inner) return null;
            return [`Option[${inner[0]}]`, inner[1]];
        }
        case "U": {
            let j = i + 1;
            const elems = [];
            while (j < s.length && s[j] !== "E") {
                const elem = parseTypeArg(s, j);
                if (!elem) return null;
                elems.push(elem[0]);
                j = elem[1];
            }
            if (j >= s.length || s[j] !== "E") return null;
            return [`(${elems.join(", ")})`, j + 1];
        }
        case "V": {
            const fn = parseFnType(s, i + 1, true);
            if (!fn) return null;
            return fn;
        }
        case "W":
            return parseFnType(s, i, false);
        case "R":
            return parseTypeRef(s, i);
        default:
            return null;
    }
}

function parseTypeArgs(s, i) {
    if (i < s.length && s[i] === "H") {
        const raised = parseTypeArg(s, i + 1);
        if (!raised) return null;
        return [` raise ${raised[0]}`, raised[1]];
    }

    if (i >= s.length || s[i] !== "G") {
        return ["", i];
    }

    let j = i + 1;
    const args = [];
    while (j < s.length && s[j] !== "E") {
        const arg = parseTypeArg(s, j);
        if (!arg) return null;
        args.push(arg[0]);
        j = arg[1];
    }
    if (j >= s.length || s[j] !== "E") return null;
    j += 1;

    let suffix = "";
    if (j < s.length && s[j] === "H") {
        const raised = parseTypeArg(s, j + 1);
        if (!raised) return null;
        suffix = ` raise ${raised[0]}`;
        j = raised[1];
    }
    return [`[${args.join(", ")}]${suffix}`, j];
}

function parseFnType(s, i, asyncMark) {
    if (i >= s.length || s[i] !== "W") return null;
    let j = i + 1;
    const params = [];
    while (j < s.length && s[j] !== "E") {
        const p = parseTypeArg(s, j);
        if (!p) return null;
        params.push(p[0]);
        j = p[1];
    }
    if (j >= s.length || s[j] !== "E") return null;
    j += 1;

    const ret = parseTypeArg(s, j);
    if (!ret) return null;
    j = ret[1];

    let raises = "";
    if (j < s.length && s[j] === "Q") {
        const raised = parseTypeArg(s, j + 1);
        if (!raised) return null;
        raises = ` raise ${raised[0]}`;
        j = raised[1];
    }

    const prefix = asyncMark ? "async " : "";
    return [`${prefix}(${params.join(", ")}) -> ${ret[0]}${raises}`, j];
}

function parseTypeRef(s, i) {
    if (i >= s.length || s[i] !== "R") return null;
    const path = appendTypePath(s, i + 1, false);
    if (!path) return null;
    let [text, j] = path;
    if (j < s.length && s[j] === "G") {
        const args = parseTypeArgs(s, j);
        if (!args) return null;
        text += args[0];
        j = args[1];
    }
    return [text, j];
}

function demangleTagF(s, i) {
    const pkg = parsePackage(s, i);
    if (!pkg) return null;
    const name = parseIdentifier(s, pkg[1]);
    if (!name) return null;
    let text = `@${pkg[0] ? `${pkg[0]}.` : ""}${name[0]}`;
    let j = name[1];

    while (j < s.length && s[j] === "N") {
        const nested = parseIdentifier(s, j + 1);
        if (!nested) return null;
        text += `.${nested[0]}`;
        j = nested[1];
    }

    if (j < s.length && s[j] === "C") {
        j += 1;
        const start = j;
        while (j < s.length && isDigit(s[j])) j += 1;
        if (start === j) return null;
        const idx = s.slice(start, j);
        text += `.${idx} (the ${idx}-th anonymous-function)`;
    }

    if (j < s.length && (s[j] === "G" || s[j] === "H")) {
        const args = parseTypeArgs(s, j);
        if (!args) return null;
        text += args[0];
        j = args[1];
    }

    return [text, j];
}

function demangleTagM(s, i) {
    const pkg = parsePackage(s, i);
    if (!pkg) return null;
    const typeName = parseIdentifier(s, pkg[1]);
    if (!typeName) return null;
    const method = parseIdentifier(s, typeName[1]);
    if (!method) return null;

    let text = `@${pkg[0] ? `${pkg[0]}.` : ""}${typeName[0]}::${method[0]}`;
    let j = method[1];

    if (j < s.length && (s[j] === "G" || s[j] === "H")) {
        const args = parseTypeArgs(s, j);
        if (!args) return null;
        text += args[0];
        j = args[1];
    }
    return [text, j];
}

function demangleTagI(s, i) {
    const implType = appendTypePath(s, i, false);
    if (!implType) return null;
    const traitType = appendTypePath(s, implType[1], false);
    if (!traitType) return null;
    const method = parseIdentifier(s, traitType[1]);
    if (!method) return null;

    let j = method[1];
    let typeArgs = "";
    if (j < s.length && (s[j] === "G" || s[j] === "H")) {
        const args = parseTypeArgs(s, j);
        if (!args) return null;
        typeArgs = args[0];
        j = args[1];
    }

    const text = `impl ${traitType[0]} for ${implType[0]}${typeArgs} with ${method[0]}`;
    return [text, j];
}

function demangleTagE(s, i) {
    const typePkg = parsePackage(s, i);
    if (!typePkg) return null;
    const typeName = parseIdentifier(s, typePkg[1]);
    if (!typeName) return null;
    const methodPkg = parsePackage(s, typeName[1]);
    if (!methodPkg) return null;
    const methodName = parseIdentifier(s, methodPkg[1]);
    if (!methodName) return null;

    const typePkgUse = isCorePackage(typePkg[0]) ? "" : typePkg[0];
    let text = `@${methodPkg[0] ? `${methodPkg[0]}.` : ""}${typePkgUse ? `${typePkgUse}.` : ""}${typeName[0]}::${methodName[0]}`;
    let j = methodName[1];

    if (j < s.length && (s[j] === "G" || s[j] === "H")) {
        const args = parseTypeArgs(s, j);
        if (!args) return null;
        text += args[0];
        j = args[1];
    }
    return [text, j];
}

function demangleTagT(s, i) {
    return appendTypePath(s, i, false);
}

function demangleTagL(s, i) {
    let j = i;
    if (j < s.length && s[j] === "m") j += 1;
    const ident = parseIdentifier(s, j);
    if (!ident) return null;
    j = ident[1];

    if (j >= s.length || s[j] !== "S") return null;
    j += 1;
    if (j >= s.length || !isDigit(s[j])) return null;
    while (j < s.length && isDigit(s[j])) j += 1;

    const noDollar = ident[0].startsWith("$") ? ident[0].slice(1) : ident[0];
    return [`@${stripSuffix(noDollar, ".fn")}`, j];
}

function demangleMangledFunctionName(funcName) {
    if (typeof funcName !== "string" || funcName.length === 0) {
        return funcName;
    }
    let i = 0;
    if (funcName[0] === "$") i = 1;
    if (funcName.length - i < 3) return funcName;
    if (funcName[i] !== "_" || funcName[i + 1] !== "M" || funcName[i + 2] !== "0") {
        return funcName;
    }
    i += 3;
    if (i >= funcName.length) return funcName;

    const tag = funcName[i];
    i += 1;

    let parsed = null;
    switch (tag) {
        case "F":
            parsed = demangleTagF(funcName, i);
            break;
        case "M":
            parsed = demangleTagM(funcName, i);
            break;
        case "I":
            parsed = demangleTagI(funcName, i);
            break;
        case "E":
            parsed = demangleTagE(funcName, i);
            break;
        case "T":
            parsed = demangleTagT(funcName, i);
            break;
        case "L":
            parsed = demangleTagL(funcName, i);
            break;
        default:
            return funcName;
    }
    if (!parsed) return funcName;

    const [text, j] = parsed;
    if (j < funcName.length) {
        const c = funcName[j];
        if (c !== "." && c !== "$" && c !== "@") {
            return funcName;
        }
    }
    return text;
}

const STACKTRACE_COLOR_ENABLED = !!__moonbit_run_env.backtrace_color_enabled;

const ANSI_RESET = "\x1b[0m";
const ANSI_RED_BOLD = "\x1b[1;31m";
const ANSI_GREY = "\x1b[90m";
const ANSI_CYAN = "\x1b[36m";
const ANSI_BOLD = "\x1b[1m";
const ANSI_PURPLE = "\x1b[35m";
const ANSI_BLUE = "\x1b[34m";
const ANSI_YELLOW = "\x1b[33m";
const ANSI_WHITE = "\x1b[37m";

function colorize(s, ansi) {
    if (!STACKTRACE_COLOR_ENABLED || !s) return s;
    return `${ansi}${s}${ANSI_RESET}`;
}

function colorizeTypePath(s) {
    if (!s || s[0] !== "@") return colorize(s, ANSI_WHITE);
    const dot = s.indexOf(".");
    if (dot <= 1) return colorize(s, ANSI_WHITE);

    const pkg = s.slice(1, dot);
    const rest = s.slice(dot + 1);
    const methodSep = rest.indexOf("::");
    if (methodSep < 0) {
        return `@${colorize(pkg, ANSI_BLUE)}.${colorize(rest, ANSI_WHITE)}`;
    }

    const typeName = rest.slice(0, methodSep);
    const method = rest.slice(methodSep + 2);
    return `@${colorize(pkg, ANSI_BLUE)}.${colorize(typeName, ANSI_YELLOW)}::${colorize(method, ANSI_WHITE)}`;
}

function colorizeDemangledName(s) {
    if (!s) return s;
    if (s.startsWith("impl ")) {
        const forPos = s.indexOf(" for ");
        const withPos = s.indexOf(" with ");
        if (forPos > 5 && withPos > forPos + 5) {
            const trait = s.slice(5, forPos);
            const implTy = s.slice(forPos + 5, withPos);
            const method = s.slice(withPos + 6);
            return `${colorize("impl", ANSI_PURPLE)} ${colorizeTypePath(trait)} ${colorize("for", ANSI_PURPLE)} ${colorizeTypePath(implTy)} ${colorize("with", ANSI_PURPLE)} ${colorize(method, ANSI_WHITE)}`;
        }
    }
    if (s[0] === "@") {
        return colorizeTypePath(s);
    }
    return colorize(s, ANSI_WHITE);
}

const BASE64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const BASE64_INDEX = (() => {
    const map = Object.create(null);
    for (let i = 0; i < BASE64.length; i++) {
        map[BASE64[i]] = i;
    }
    return map;
})();

const SOURCE_MAP_CACHE = new Map();

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

function bytesToString(bytes) {
    if (typeof TextDecoder !== "undefined") {
        return new TextDecoder("utf-8").decode(bytes);
    }
    let s = "";
    for (let i = 0; i < bytes.length; i++) {
        s += String.fromCharCode(bytes[i]);
    }
    return s;
}

function dirname(path) {
    const slash = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
    return slash >= 0 ? path.slice(0, slash) : ".";
}

function isAbsolutePath(path) {
    return path.startsWith("/") || /^[A-Za-z]:[\\/]/.test(path);
}

function joinPath(baseDir, relPath) {
    if (!relPath) return baseDir;
    if (isAbsolutePath(relPath)) return relPath;
    if (baseDir === "." || baseDir === "") return relPath;
    const sep = baseDir.includes("\\") ? "\\" : "/";
    if (baseDir.endsWith("/") || baseDir.endsWith("\\")) {
        return `${baseDir}${relPath}`;
    }
    return `${baseDir}${sep}${relPath}`;
}

function formatSourcePathAuto(path) {
    if (typeof path !== "string" || path.length === 0) {
        return "";
    }

    try {
        if (
            __moonbit_backtrace_unstable &&
            typeof __moonbit_backtrace_unstable.format_source_path_auto === "function"
        ) {
            return __moonbit_backtrace_unstable.format_source_path_auto(path);
        }
    } catch (_) {
        // Keep stack formatting robust even if helper lookup fails.
    }

    return path;
}

function readULEB128(buf, start) {
    let i = start;
    let n = 0;
    let shift = 0;
    while (i < buf.length) {
        const b = buf[i];
        i += 1;
        n |= (b & 0x7f) << shift;
        if ((b & 0x80) === 0) {
            return [n, i];
        }
        shift += 7;
    }
    return null;
}

function extractSourceMapURLFromWasm(buf) {
    const name = "sourceMappingURL";
    let pos = 8; // skip wasm magic and version
    while (pos < buf.length) {
        const idRead = readULEB128(buf, pos);
        if (!idRead) return null;
        const secId = idRead[0];
        const sizeRead = readULEB128(buf, idRead[1]);
        if (!sizeRead) return null;
        const secSize = sizeRead[0];
        const bodyPos = sizeRead[1];
        const secEnd = bodyPos + secSize;
        if (secEnd > buf.length) return null;

        if (secId === 0) {
            const nameLenRead = readULEB128(buf, bodyPos);
            if (!nameLenRead) return null;
            const secNameLen = nameLenRead[0];
            const secNamePos = nameLenRead[1];
            const secNameEnd = secNamePos + secNameLen;
            if (secNameEnd > secEnd) return null;
            const secName = bytesToString(buf.slice(secNamePos, secNameEnd));
            if (secName === name) {
                const valLenRead = readULEB128(buf, secNameEnd);
                if (!valLenRead) return null;
                const valLen = valLenRead[0];
                const valPos = valLenRead[1];
                const valEnd = valPos + valLen;
                if (valEnd > secEnd) return null;
                return bytesToString(buf.slice(valPos, valEnd));
            }
        }
        pos = secEnd;
    }
    return null;
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

function loadSourceMapForModule(wasmPath) {
    if (!wasmPath) return null;
    if (SOURCE_MAP_CACHE.has(wasmPath)) {
        return SOURCE_MAP_CACHE.get(wasmPath);
    }

    let parsed = null;
    try {
        const wasmBytes = read_file_to_bytes(wasmPath);
        let mapPath = null;
        const embedded = extractSourceMapURLFromWasm(wasmBytes);
        if (
            embedded &&
            !embedded.startsWith("data:") &&
            !embedded.startsWith("http://") &&
            !embedded.startsWith("https://")
        ) {
            mapPath = isAbsolutePath(embedded) ? embedded : joinPath(dirname(wasmPath), embedded);
        }
        if (!mapPath) {
            mapPath = `${wasmPath}.map`;
        }
        const mapBytes = read_file_to_bytes(mapPath);
        const rawMap = JSON.parse(bytesToString(mapBytes));
        parsed = parseWasmSourceMap(rawMap);
    } catch (_) {
        parsed = null;
    }
    SOURCE_MAP_CACHE.set(wasmPath, parsed);
    return parsed;
}

function sourcePosForOffset(offset) {
    if (typeof module_name !== "string" || !module_name) {
        return null;
    }
    const sm = loadSourceMapForModule(module_name);
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
    const sourceFile = formatSourcePathAuto(sm.sources[m.source]);
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
        const fn = colorizeDemangledName(demangleMangledFunctionName(withLoc[2]));
        const srcPos = sourcePosForWasmLocation(withLoc[4]);
        const src = srcPos ? ` ${colorize(srcPos, ANSI_GREY)}` : "";
        return `${withLoc[1]}${colorize("at", ANSI_CYAN)} ${colorize(fn, ANSI_BOLD)}${src}`;
    }

    // Fallback: "    at <func>"
    const noLoc = line.match(/^(\s*)at\s+(\S+)(\s*)$/);
    if (noLoc) {
        const fn = colorizeDemangledName(demangleMangledFunctionName(noLoc[2]));
        return `${noLoc[1]}${colorize("at", ANSI_CYAN)} ${colorize(fn, ANSI_BOLD)}${noLoc[3]}`;
    }

    if (/^\s*(RuntimeError|TypeError|ReferenceError|RangeError|SyntaxError|Error)\b/.test(line)) {
        return colorize(line, ANSI_RED_BOLD);
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
