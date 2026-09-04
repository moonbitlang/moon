// V8-native value shims and module import wiring for moonrun.

const __moonbit_fs_unstable = module_imports.__moonbit_fs_unstable;
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

const tag = new WebAssembly.Tag({ parameters: [] });
const ffiBytesMemory = new WebAssembly.Memory({ initial: 1 });
module_imports.exception = {
    tag: tag,
    throw: () => {
        throw new WebAssembly.Exception(tag, [], { traceStack: true })
    },
};
module_imports["ffi-bytes"] = {
    from_memory: (offset, length) => new Uint8Array(ffiBytesMemory.buffer.slice(offset, offset + length)),
    new: (length) => new Uint8Array(length),
    get: (bytes, index) => bytes[index],
    set: (bytes, index, value) => bytes[index] = value,
    copy: (dst, dst_off, src, src_off, len) => dst.set(src.subarray(src_off, src_off + len), dst_off),
    fill: (bytes, start, value, len) => bytes.fill(value, start, start + len),
    length: (bytes) => bytes.length,
    equals: (a, b) => a.length === b.length && a.every((val, index) => val === b[index]) ? 1 : 0,
    asString: (bytes, start, len) => {
        const slice = bytes.subarray(start, start + len);
        return String.fromCharCode(...Array.from(
            { length: Math.floor(slice.length / 2) },
            (_, i) => slice[i * 2] | (slice[i * 2 + 1] << 8),
        ));
    },
    memory: ffiBytesMemory,
};
