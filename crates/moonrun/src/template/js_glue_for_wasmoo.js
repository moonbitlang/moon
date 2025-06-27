(js => async (module_name) => {
  "use strict";
  const math = {
    cos: Math.cos,
    sin: Math.sin,
    tan: Math.tan,
    acos: Math.acos,
    asin: Math.asin,
    atan: Math.atan,
    cosh: Math.cosh,
    sinh: Math.sinh,
    tanh: Math.tanh,
    acosh: Math.acosh,
    asinh: Math.asinh,
    atanh: Math.atanh,
    cbrt: Math.cbrt,
    exp: Math.exp,
    expm1: Math.expm1,
    log: Math.log,
    log1p: Math.log1p,
    log2: Math.log2,
    log10: Math.log10,
    atan2: Math.atan2,
    hypot: Math.hypot,
    pow: Math.pow,
    fmod: (x, y) => x % y,
  };
  
  const typed_arrays = [
    Float32Array,
    Float64Array,
    Int8Array,
    Uint8Array,
    Int16Array,
    Uint16Array,
    Int32Array,
    Int32Array,
    Int32Array,
    Int32Array,
    Float32Array,
    Float64Array,
    Uint8Array,
    Uint16Array,
    Uint8ClampedArray,
  ];
  
  const on_windows = false;
  
  var out_channels = {
    map: new WeakMap(),
    set: new Set(),
    finalization: new FinalizationRegistry((ref) =>
      out_channels.set.delete(ref),
    ),
  };
  
  function register_channel(ch) {
    const ref = new WeakRef(ch);
    out_channels.map.set(ch, ref);
    out_channels.set.add(ref);
    out_channels.finalization.register(ch, ref, ch);
  }
  
  function unregister_channel(ch) {
    const ref = out_channels.map.get(ch);
    if (ref) {
      out_channels.map.delete(ch);
      out_channels.set.delete(ref);
      out_channels.finalization.unregister(ch);
    }
  }
  
  function channel_list() {
    return [...out_channels.set].map((ref) => ref.deref()).filter((ch) => ch);
  }
  
  var start_fiber;
  
  function hash_int(h, d) {
    d = Math.imul(d, 0xcc9e2d51 | 0);
    d = (d << 15) | (d >>> 17); // ROTL32(d, 15);
    d = Math.imul(d, 0x1b873593);
    h ^= d;
    h = (h << 13) | (h >>> 19); //ROTL32(h, 13);
    return (((h + (h << 2)) | 0) + (0xe6546b64 | 0)) | 0;
  }
  function hash_string(h, s) {
    for (var i = 0; i < s.length; i++) h = hash_int(h, s.charCodeAt(i));
    return h ^ s.length;
  }
  
  let record_backtrace_flag = 0;
  
  for (const l of getenv("OCAMLRUNPARAM")?.split(",") || []) {
    if (l === "b") record_backtrace_flag = 1;
    if (l.startsWith("b=")) record_backtrace_flag = +l.slice(2) ? 1 : 0;
  }
  
  function alloc_stat(s, large) {
    let kind = s.kind;
    // if (s.is_file) {
    //   kind = 0;
    // } else if (s.is_directory) {
    //   kind = 1;
    // } else if (s.is_character_device) {
    //   kind = 2;
    // } else if (s.is_block_device) {
    //   kind = 3;
    // } else if (s.is_symbolic_link) {
    //   kind = 4;
    // } else if (s.is_fifo) {
    //   kind = 5;
    // } else if (s.is_socket) {
    //   kind = 6;
    // }
    return caml_alloc_stat(
      large,
      s.dev,
      s.ino | 0,
      kind,
      s.mode,
      s.nlink,
      s.uid,
      s.gid,
      s.rdev,
      BigInt(s.size),
      s.atimeMs / 1000,
      s.mtimeMs / 1000,
      s.ctimeMs / 1000,
    );
  }
  
  const bindings = {
    jstag: WebAssembly.JSTag,
    identity: (x) => x,
    from_bool: (x) => !!x,
    get: (x, y) => x[y],
    set: (x, y, z) => (x[y] = z),
    delete: (x, y) => delete x[y],
    instanceof: (x, y) => x instanceof y,
    typeof: (x) => typeof x,
    equals: (x, y) => x == y,
    strict_equals: (x, y) => x === y,
    fun_call: (f, o, args) => f.apply(o, args),
    meth_call: (o, f, args) => o[f].apply(o, args),
    new_array: (n) => new Array(n),
    new_obj: () => ({}),
    new: (c, args) => new c(...args),
    global_this: globalThis,
    iter_props: (o, f) => {
      for (var nm in o) if (Object.hasOwn(o, nm)) f(nm);
    },
    array_length: (a) => a.length,
    array_get: (a, i) => a[i],
    array_set: (a, i, v) => (a[i] = v),
    read_string: (l) => decode_utf8(new Uint8Array(buffer, 0, l)),
    read_string_stream: (l, stream) => { console_log("TODO: read_string_stream") },
    append_string: (s1, s2) => s1 + s2,
    write_string: (s) => {
      var start = 0,
        len = s.length;
      for (; ;) {
        const { read, written } = encode_into(
          s.slice(start),
          out_buffer,
        );
        len -= read;
        if (!len) return written;
        caml_extract_bytes(written);
        start += read;
      }
    },
    ta_create: (k, sz) => new typed_arrays[k](sz),
    ta_normalize: (a) =>
      a instanceof Uint32Array
        ? new Int32Array(a.buffer, a.byteOffset, a.length)
        : a,
    ta_kind: (a) => typed_arrays.findIndex((c) => a instanceof c),
    ta_length: (a) => a.length,
    ta_get_f64: (a, i) => a[i],
    ta_get_f32: (a, i) => a[i],
    ta_get_i32: (a, i) => a[i],
    ta_get_i16: (a, i) => a[i],
    ta_get_ui16: (a, i) => a[i],
    ta_get_i8: (a, i) => a[i],
    ta_get_ui8: (a, i) => a[i],
    ta_get16_ui8: (a, i) => a[i] | (a[i + 1] << 8),
    ta_get32_ui8: (a, i) =>
      a[i] | (a[i + 1] << 8) | (a[i + 2] << 16) | (a[i + 3] << 24),
    ta_set_f64: (a, i, v) => (a[i] = v),
    ta_set_f32: (a, i, v) => (a[i] = v),
    ta_set_i32: (a, i, v) => (a[i] = v),
    ta_set_i16: (a, i, v) => (a[i] = v),
    ta_set_ui16: (a, i, v) => (a[i] = v),
    ta_set_i8: (a, i, v) => (a[i] = v),
    ta_set_ui8: (a, i, v) => (a[i] = v),
    ta_set16_ui8: (a, i, v) => {
      a[i] = v;
      a[i + 1] = v >> 8;
    },
    ta_set32_ui8: (a, i, v) => {
      a[i] = v;
      a[i + 1] = v >> 8;
      a[i + 2] = v >> 16;
      a[i + 3] = v >> 24;
    },
    ta_fill: (a, v) => a.fill(v),
    ta_blit: (s, d) => d.set(s),
    ta_subarray: (a, i, j) => a.subarray(i, j),
    ta_set: (a, b, i) => a.set(b, i),
    ta_new: (len) => new Uint8Array(len),
    ta_copy: (ta, t, s, e) => ta.copyWithin(t, s, e),
    ta_bytes: (a) =>
      new Uint8Array(a.buffer, a.byteOffset, a.length * a.BYTES_PER_ELEMENT),
    ta_blit_from_string: (a, b, c, d, e) => { for (let f = 0; f < e; f++) c[d + f] = string_get(a, b + f) },
    ta_blit_to_string: (a, b, c, d, e) => { for (let f = 0; f < e; f++) string_set(c, d + f, a[b + f]) },
    wrap_callback: (f) =>
      function (...args) {
        if (args.length === 0) {
          args = [undefined];
        }
        return caml_callback(f, args.length, args, 1);
      },
    wrap_callback_args: (f) =>
      function (...args) {
        return caml_callback(f, 1, [args], 0);
      },
    wrap_callback_strict: (arity, f) =>
      function (...args) {
        args.length = arity;
        return caml_callback(f, arity, args, 0);
      },
    wrap_callback_unsafe: (f) =>
      function (...args) {
        return caml_callback(f, args.length, args, 2);
      },
    wrap_meth_callback: (f) =>
      function (...args) {
        args.unshift(this);
        return caml_callback(f, args.length, args, 1);
      },
    wrap_meth_callback_args: (f) =>
      function (...args) {
        return caml_callback(f, 2, [this, args], 0);
      },
    wrap_meth_callback_strict: (arity, f) =>
      function (...args) {
        args.length = arity;
        args.unshift(this);
        return caml_callback(f, args.length, args, 0);
      },
    wrap_meth_callback_unsafe: (f) =>
      function (...args) {
        args.unshift(this);
        return caml_callback(f, args.length, args, 2);
      },
    wrap_fun_arguments: (f) =>
      function (...args) {
        return f(args);
      },
    format_float: (prec, conversion, pad, x) => {
      function toFixed(x, dp) {
        if (Math.abs(x) < 1.0) {
          return x.toFixed(dp);
        } else {
          var e = Number.parseInt(x.toString().split("+")[1]);
          if (e > 20) {
            e -= 20;
            x /= Math.pow(10, e);
            x += new Array(e + 1).join("0");
            if (dp > 0) {
              x = x + "." + new Array(dp + 1).join("0");
            }
            return x;
          } else return x.toFixed(dp);
        }
      }
      switch (conversion) {
        case 0:
          var s = x.toExponential(prec);
          // exponent should be at least two digits
          var i = s.length;
          if (s.charAt(i - 3) === "e")
            s = s.slice(0, i - 1) + "0" + s.slice(i - 1);
          break;
        case 1:
          s = toFixed(x, prec);
          break;
        case 2:
          prec = prec ? prec : 1;
          s = x.toExponential(prec - 1);
          var j = s.indexOf("e");
          var exp = +s.slice(j + 1);
          if (exp < -4 || x >= 1e21 || x.toFixed(0).length > prec) {
            // remove trailing zeroes
            var i = j - 1;
            while (s.charAt(i) === "0") i--;
            if (s.charAt(i) === ".") i--;
            s = s.slice(0, i + 1) + s.slice(j);
            i = s.length;
            if (s.charAt(i - 3) === "e")
              s = s.slice(0, i - 1) + "0" + s.slice(i - 1);
            break;
          } else {
            var p = prec;
            if (exp < 0) {
              p -= exp + 1;
              s = x.toFixed(p);
            } else while (((s = x.toFixed(p)), s.length > prec + 1)) p--;
            if (p) {
              // remove trailing zeroes
              var i = s.length - 1;
              while (s.charAt(i) === "0") i--;
              if (s.charAt(i) === ".") i--;
              s = s.slice(0, i + 1);
            }
          }
          break;
      }
      return pad ? " " + s : s;
    },
    gettimeofday: () => new Date().getTime() / 1000,
    times: () => {
      if (globalThis.process?.cpuUsage) {
        var t = globalThis.process.cpuUsage();
        return caml_alloc_times(t.user / 1e6, t.system / 1e6);
      } else {
        var t = performance.now() / 1000;
        return caml_alloc_times(t, t);
      }
    },
    gmtime: (t) => {
      var d = new Date(t * 1000);
      var d_num = d.getTime();
      var januaryfirst = new Date(Date.UTC(d.getUTCFullYear(), 0, 1)).getTime();
      var doy = Math.floor((d_num - januaryfirst) / 86400000);
      return caml_alloc_tm(
        d.getUTCSeconds(),
        d.getUTCMinutes(),
        d.getUTCHours(),
        d.getUTCDate(),
        d.getUTCMonth(),
        d.getUTCFullYear() - 1900,
        d.getUTCDay(),
        doy,
        false,
      );
    },
    localtime: (t) => {
      var d = new Date(t * 1000);
      var d_num = d.getTime();
      var januaryfirst = new Date(d.getFullYear(), 0, 1).getTime();
      var doy = Math.floor((d_num - januaryfirst) / 86400000);
      var jan = new Date(d.getFullYear(), 0, 1);
      var jul = new Date(d.getFullYear(), 6, 1);
      var stdTimezoneOffset = Math.max(
        jan.getTimezoneOffset(),
        jul.getTimezoneOffset(),
      );
      return caml_alloc_tm(
        d.getSeconds(),
        d.getMinutes(),
        d.getHours(),
        d.getDate(),
        d.getMonth(),
        d.getFullYear() - 1900,
        d.getDay(),
        doy,
        d.getTimezoneOffset() < stdTimezoneOffset,
      );
    },
    mktime: (year, month, day, h, m, s) =>
      new Date(year, month, day, h, m, s).getTime(),
    random_seed: () => crypto.getRandomValues(new Int32Array(12)),
    access,
    open,
    close,
    write,
    read,
    fsync,
    file_size,
    register_channel,
    unregister_channel,
    channel_list,
    exit,
    argv: () => process_argv,
    on_windows: () => on_windows,
    getenv,
    backtrace_status: () => record_backtrace_flag,
    record_backtrace: (b) => (record_backtrace_flag = b),
    system,
    isatty,
    time: () => performance.now(),
    getcwd,
    chdir,
    mkdir,
    rmdir,
    // link,
    // symlink: (t, p, _) => symlink(t, p),
    // readlink,
    unlink,
    readdir: (p) => read_dir(p),
    // opendir:
    // readdir,
    // closedir,
    stat: (p, l) => alloc_stat(stat(p), l),
    lstat: (p, l) => alloc_stat(lstat(p), l),
    fstat: (fd, l) => alloc_stat(fstat(fd), l),
    chmod,
    fchmod,
    file_exists,
    is_directory,
    is_file,
    utimes,
    truncate,
    ftruncate,
    rename,
    throw: (a) => { throw a },
    start_fiber: (x) => start_fiber(x),
    suspend_fiber: (f, env) => new Promise((k) => f(k, env)),
    resume_fiber: (k, v) => k(v),
    weak_new: (v) => new WeakRef(v),
    weak_deref: (w) => {
      var v = w.deref();
      return v === undefined ? null : v;
    },
    weak_map_new: () => new WeakMap(),
    map_new: () => new Map(),
    map_get: (m, x) => {
      var v = m.get(x);
      return v === undefined ? null : v;
    },
    map_set: (m, x, v) => m.set(x, v),
    map_delete: (m, x) => m.delete(x),
    log: (x) => console_log(x),
  };
  
  const string_ops = {
    test: (v) => +(typeof v === "string"),
    compare: (s1, s2) => (s1 < s2 ? -1 : +(s1 > s2)),
    hash: hash_string,
    decodeStringFromUTF8Array: () => "",
    encodeStringToUTF8Array: () => 0,
    fromCharCodeArray: () => "",
  };
  
  const importObject = {
    Math: math,
    bindings,
    js,
    "wasm:js-string": string_ops,
    "wasm:text-decoder": string_ops,
    "wasm:text-encoder": string_ops,
    env: {},
  };
  
  const options = { builtins: ["js-string", "text-decoder", "text-encoder"] };
  
  // start load wasm
  
  importObject.OCaml = {};
  
  let bytes = read_file_to_bytes(module_name);
  let wasm_module = new WebAssembly.Module(bytes, options);
  let instance = new WebAssembly.Instance(wasm_module, importObject);
  
  
  Object.assign(
    importObject.OCaml,
    instance.exports,
  );
  
  var {
    caml_callback,
    caml_alloc_times,
    caml_alloc_tm,
    caml_alloc_stat,
    caml_start_fiber,
    caml_handle_uncaught_exception,
    caml_buffer,
    caml_extract_bytes,
    string_get,
    string_set,
    _initialize,
  } = instance.exports;
  
  start_fiber = caml_start_fiber
  
  var buffer = caml_buffer?.buffer;
  var out_buffer = buffer && new Uint8Array(buffer, 0, buffer.length);
  
  await _initialize();
})(function (_) { "use strict"; return {} }(globalThis))(module_name);