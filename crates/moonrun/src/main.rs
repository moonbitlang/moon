// moon: The build system and package manager for MoonBit.
// Copyright (C) 2024 International Digital Economy Academy
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

use clap::Parser;
use std::any::Any;
use std::io::{self, Write};
use std::path::Path;
use std::{cell::Cell, io::Read, path::PathBuf, time::Instant};
use v8::V8::set_flags_from_string;

mod fs_api_temp;
mod js;
mod sys_api;
mod util;

use rand::Rng;
use rand::SeedableRng;
use rand::rngs::StdRng;

const BUILTIN_SCRIPT_ORIGIN_PREFIX: &str = "__$moonrun_v8_builtin_script$__";

#[derive(Default)]
struct PrintEnv {
    dangling_high_half: Cell<Option<u32>>,
}

fn now(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = v8::Array::new(scope, 1);

    let now = std::time::SystemTime::now();
    let duration = now
        .duration_since(std::time::UNIX_EPOCH)
        .expect("Time went backwards");

    let secs = v8::Number::new(scope, duration.as_millis() as f64).into();
    result.set_index(scope, 0, secs).unwrap();

    ret.set(result.into());
}

fn instant_now(
    scope: &mut v8::HandleScope,
    mut args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let now = Box::new(Instant::now());
    let ptr = Box::<Instant>::leak(now) as *mut Instant;
    let weak_rc = std::rc::Rc::new(std::cell::Cell::new(None));
    let weak = v8::Weak::with_finalizer(
        unsafe { args.get_isolate() },
        v8::External::new(scope, ptr as *mut std::ffi::c_void),
        Box::new({
            let weak_rc = weak_rc.clone();
            move |isolate| unsafe {
                drop(Box::from_raw(ptr));
                drop(v8::Weak::from_raw(isolate, weak_rc.get()));
            }
        }),
    );
    let local = weak.to_local(scope).unwrap();
    weak_rc.set(weak.into_raw());
    ret.set(local.into());
}

fn instant_elapsed_as_secs_f64(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let arg = args.get(0);
    let instant: v8::Local<v8::External> = arg.try_into().unwrap();
    let instant = unsafe { &*(instant.value() as *mut Instant) };
    let elapsed = instant.elapsed().as_secs_f64();
    ret.set(v8::Number::new(scope, elapsed).into());
}

fn print_char(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let print_env = {
        let data = args.data();
        assert!(data.is_external());
        let data: v8::Local<v8::Data> = data.into();
        let ptr = v8::Local::<v8::External>::try_from(data).unwrap().value();
        unsafe { &*(ptr as *const PrintEnv) }
    };

    let arg = args.get(0);
    let c = arg.integer_value(scope).unwrap() as u32;
    if (0xd800..=0xdbff).contains(&c) {
        // high surrogate
        let high = c - 0xd800;
        if print_env.dangling_high_half.get().is_some() {
            // Print previous char as invalid unicode
            print!("{}", std::char::from_u32(0xfffd).unwrap());
        }
        print_env.dangling_high_half.set(Some(high));
    } else {
        let c = {
            if (0xdc00..=0xdfff).contains(&c) {
                // low surrogate
                if let Some(high) = print_env.dangling_high_half.take() {
                    0x10000 + (high << 10) + (c - 0xdc00)
                } else {
                    0xfffd
                }
            } else {
                c
            }
        };
        let c = std::char::from_u32(c).unwrap();
        print!("{c}");
    }
    ret.set_undefined()
}

fn console_elog(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut _ret: v8::ReturnValue,
) {
    let arg = args.get(0);
    let arg = arg.to_string(scope).unwrap();
    let arg = arg.to_rust_string_lossy(scope);
    eprintln!("{arg}");
}

fn console_log(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut _ret: v8::ReturnValue,
) {
    let arg = args.get(0);
    let arg = arg.to_string(scope).unwrap();
    let arg = arg.to_rust_string_lossy(scope);
    println!("{arg}");
}

pub fn get_array_buffer_ptr(ab: v8::Local<v8::ArrayBuffer>) -> *mut u8 {
    unsafe { std::mem::transmute(ab.data()) }
}

fn read_utf8_char() -> io::Result<Option<char>> {
    let mut buffer = [0; 4];
    let stdin = io::stdin();
    let mut handle = stdin.lock();

    let size = handle.read(&mut buffer[0..1])?;
    if size == 0 {
        return Ok(None);
    }

    let num_bytes = match buffer[0] {
        0..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF7 => 4,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid UTF-8 first byte",
            ));
        }
    };

    if num_bytes > 1 {
        handle.read_exact(&mut buffer[1..num_bytes])?;
    }

    let char = std::str::from_utf8(&buffer[..num_bytes])
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
        .chars()
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no character found"))?;

    Ok(Some(char))
}

fn read_char(
    _scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = read_utf8_char();
    match result {
        Ok(Some(c)) => {
            ret.set_int32(c as i32);
        }
        _ => ret.set_int32(-1),
    }
}

fn read_bytes_from_stdin(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let mut buffer = Vec::new();
    let stdin = io::stdin();
    let mut handle = stdin.lock();

    let size = handle.read_to_end(&mut buffer).unwrap();

    if size == 0 {
        let empty_array_buffer = v8::ArrayBuffer::new(scope, 0);
        let empty_uint8_array = v8::Uint8Array::new(scope, empty_array_buffer, 0, 0).unwrap();
        ret.set(empty_uint8_array.into());
    } else {
        let array_buffer = v8::ArrayBuffer::new(scope, size);
        let uint8_array = v8::Uint8Array::new(scope, array_buffer, 0, size).unwrap();
        unsafe {
            std::ptr::copy(buffer.as_ptr(), get_array_buffer_ptr(array_buffer), size);
        }
        ret.set(uint8_array.into());
    }
}

fn read_file_to_bytes(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let arg = args.get(0);
    let path = PathBuf::from(arg.to_rust_string_lossy(scope));
    let bytes = std::fs::read(path).unwrap();
    let buffer = v8::ArrayBuffer::new(scope, bytes.len());
    let ab = v8::Uint8Array::new(scope, buffer, 0, bytes.len()).unwrap();

    unsafe {
        std::ptr::copy(bytes.as_ptr(), get_array_buffer_ptr(buffer), bytes.len());
    }

    ret.set(ab.into());
}

fn write_char(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut _ret: v8::ReturnValue,
) {
    let fd = args.get(0).int32_value(scope).unwrap();
    let c = args.get(1).integer_value(scope).unwrap() as u32;
    let c = std::char::from_u32(c).unwrap();
    match fd {
        1 => print!("{c}"),
        2 => eprint!("{c}"),
        _ => {}
    }
}

fn flush(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut _ret: v8::ReturnValue,
) {
    let fd = args.get(0).int32_value(scope).unwrap();
    match fd {
        1 => std::io::stdout().flush().unwrap(),
        2 => std::io::stderr().flush().unwrap(),
        _ => {}
    }
}

fn stdrng_seed_from_u64(
    scope: &mut v8::HandleScope,
    mut args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let seed = args.get(0).int32_value(scope).unwrap_or(0) as u64;
    let rng = Box::new(StdRng::seed_from_u64(seed));
    let ptr = Box::<StdRng>::leak(rng) as *mut StdRng;
    let weak_rc = std::rc::Rc::new(std::cell::Cell::new(None));
    let weak = v8::Weak::with_finalizer(
        unsafe { args.get_isolate() },
        v8::External::new(scope, ptr as *mut std::ffi::c_void),
        Box::new({
            let weak_rc = weak_rc.clone();
            move |isolate| unsafe {
                drop(Box::from_raw(ptr));
                drop(v8::Weak::from_raw(isolate, weak_rc.get()));
            }
        }),
    );
    let local = weak.to_local(scope).unwrap();
    weak_rc.set(weak.into_raw());
    ret.set(local.into());
}

fn stdrng_gen_range(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let arg = args.get(0);
    let rng: v8::Local<v8::External> = arg.try_into().unwrap();
    let rng = unsafe { &mut *(rng.value() as *mut StdRng) };

    let ubound = args.get(1).int32_value(scope).unwrap();
    let num = rng.gen_range(0..ubound);
    ret.set_int32(num);
}

fn exit(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut _ret: v8::ReturnValue,
) {
    let code = args.get(0).to_int32(scope).unwrap();
    std::process::exit(code.value());
}

fn init_env(
    dtors: &mut Vec<Box<dyn Any>>,
    scope: &mut v8::HandleScope,
    wasm_file_name: &str,
    args: &[String],
) {
    let global_proxy = scope.get_current_context().global(scope);

    let print_env_box = Box::<PrintEnv>::default();
    let identifier = v8::String::new(scope, "print").unwrap();
    let print_env = &*print_env_box as *const PrintEnv;
    let print_env = v8::External::new(scope, print_env as *mut std::ffi::c_void);
    let value = v8::Function::builder(print_char)
        .data(print_env.into())
        .build(scope)
        .unwrap();
    global_proxy.set(scope, identifier.into(), value.into());
    dtors.push(print_env_box);

    {
        let identifier = v8::String::new(scope, "console_elog").unwrap();
        let value = v8::Function::builder(console_elog).build(scope).unwrap();
        global_proxy.set(scope, identifier.into(), value.into());
    }

    {
        let identifier = v8::String::new(scope, "console_log").unwrap();
        let value = v8::Function::builder(console_log).build(scope).unwrap();
        global_proxy.set(scope, identifier.into(), value.into());
    }

    {
        let identifier = v8::String::new(scope, "__moonbit_time_unstable").unwrap();
        let obj = v8::Object::new(scope);
        global_proxy.set(scope, identifier.into(), obj.into());

        let identifier = v8::String::new(scope, "instant_now").unwrap();
        let value = v8::Function::builder(instant_now).build(scope).unwrap();
        obj.set(scope, identifier.into(), value.into());

        let identifier = v8::String::new(scope, "instant_elapsed_as_secs_f64").unwrap();
        let value = v8::Function::builder(instant_elapsed_as_secs_f64)
            .build(scope)
            .unwrap();
        obj.set(scope, identifier.into(), value.into());

        let identifier = v8::String::new(scope, "now").unwrap();
        let value = v8::Function::builder(now).build(scope).unwrap();
        obj.set(scope, identifier.into(), value.into());
    }

    // API for the fs module
    let identifier = v8::String::new(scope, "__moonbit_fs_unstable").unwrap();
    let obj = v8::Object::new(scope);
    let obj: v8::Local<'_, v8::Object> = js::init_env(obj, scope);
    let obj = sys_api::init_env(obj, scope, wasm_file_name, args);
    let obj: v8::Local<'_, v8::Object> = fs_api_temp::init_fs(obj, scope);
    global_proxy.set(scope, identifier.into(), obj.into());

    {
        let identifier = v8::String::new(scope, "read_file_to_bytes").unwrap();
        let value = v8::Function::builder(read_file_to_bytes)
            .build(scope)
            .unwrap();
        global_proxy.set(scope, identifier.into(), value.into());
    }

    {
        let identifier = v8::String::new(scope, "__moonbit_io_unstable").unwrap();
        let obj = v8::Object::new(scope);
        global_proxy.set(scope, identifier.into(), obj.into());

        let identifier = v8::String::new(scope, "read_bytes_from_stdin").unwrap();
        let value = v8::Function::builder(read_bytes_from_stdin)
            .build(scope)
            .unwrap();
        obj.set(scope, identifier.into(), value.into());

        let identifier = v8::String::new(scope, "read_char").unwrap();
        let value = v8::Function::builder(read_char).build(scope).unwrap();
        obj.set(scope, identifier.into(), value.into());

        let identifier = v8::String::new(scope, "write_char").unwrap();
        let value = v8::Function::builder(write_char).build(scope).unwrap();
        obj.set(scope, identifier.into(), value.into());

        let identifier = v8::String::new(scope, "flush").unwrap();
        let value = v8::Function::builder(flush).build(scope).unwrap();
        obj.set(scope, identifier.into(), value.into());
    }

    {
        let identifier = v8::String::new(scope, "__moonbit_rand_unstable").unwrap();
        let obj = v8::Object::new(scope);
        global_proxy.set(scope, identifier.into(), obj.into());

        let identifier = v8::String::new(scope, "stdrng_seed_from_u64").unwrap();
        let value = v8::Function::builder(stdrng_seed_from_u64)
            .build(scope)
            .unwrap();
        obj.set(scope, identifier.into(), value.into());

        let identifier = v8::String::new(scope, "stdrng_gen_range").unwrap();
        let value = v8::Function::builder(stdrng_gen_range)
            .build(scope)
            .unwrap();
        obj.set(scope, identifier.into(), value.into());
    }

    {
        let identifier = v8::String::new(scope, "__moonbit_sys_unstable").unwrap();
        let obj = v8::Object::new(scope);
        global_proxy.set(scope, identifier.into(), obj.into());

        let exit = v8::FunctionTemplate::new(scope, exit);
        let exit = exit.get_function(scope).unwrap();
        let ident = v8::String::new(scope, "exit").unwrap();
        obj.set(scope, ident.into(), exit.into());
    }
}

fn create_script_origin<'s>(scope: &mut v8::HandleScope<'s>, name: &str) -> v8::ScriptOrigin<'s> {
    let name = format!("{BUILTIN_SCRIPT_ORIGIN_PREFIX}{name}");
    let name = v8::String::new(scope, &name).unwrap();
    v8::ScriptOrigin::new(
        scope,
        name.into(),
        0,
        0,
        false,
        0,
        None,
        false,
        false,
        false,
        None,
    )
}

enum Source<'a> {
    File(&'a Path),
    Bytes(Vec<u8>),
}

fn wasm_mode(
    source: Source,
    args: &[String],
    no_stack_trace: bool,
    test_args: Option<String>,
) -> anyhow::Result<()> {
    let isolate = &mut v8::Isolate::new(Default::default());
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope, Default::default());
    let scope = &mut v8::ContextScope::new(scope, context);

    let mut script =
        format!(r#"const BUILTIN_SCRIPT_ORIGIN_PREFIX = "{BUILTIN_SCRIPT_ORIGIN_PREFIX}";"#);

    let global_proxy = scope.get_current_context().global(scope);
    let wasm_file_name = match &source {
        Source::File(file) => file.to_string_lossy().to_string(),
        Source::Bytes(_) => "<eval>".to_string(),
    };

    match source {
        Source::File(file) => {
            let module_key = v8::String::new(scope, "module_name").unwrap().into();
            let module_name = v8::String::new(scope, file.to_string_lossy().as_ref())
                .unwrap()
                .into();
            global_proxy.set(scope, module_key, module_name);
            script.push_str("let bytes;");
        }
        Source::Bytes(bytes) => {
            let len = bytes.len();

            let bytes_key = v8::String::new(scope, "bytes").unwrap().into();
            let buf = v8::ArrayBuffer::new_backing_store_from_vec(bytes);
            let buf = v8::ArrayBuffer::with_backing_store(scope, &buf.make_shared());
            let u8arr = v8::Uint8Array::new(scope, buf, 0, len)
                .expect("Failed to create buffer for WASM program");

            global_proxy.set(scope, bytes_key, u8arr.cast());
        }
    }
    let mut dtors = Vec::new();
    init_env(&mut dtors, scope, &wasm_file_name, args);

    if let Some(ref test_args) = test_args {
        let test_args = serde_json_lenient::from_str::<TestArgs>(test_args).unwrap();
        let file_and_index = test_args.file_and_index;

        let mut test_params: Vec<[String; 2]> = vec![];
        for (file, index) in file_and_index {
            for range in index {
                for i in range {
                    test_params.push([file.clone(), i.to_string()]);
                }
            }
        }
        script.push_str(&format!("const packageName = {:?};", test_args.package));
        script.push_str(&format!("const testParams = {test_params:?};"));
    }
    script.push_str(&format!("const no_stack_trace = {no_stack_trace};"));
    script.push_str(&format!("const test_mode = {};", test_args.is_some()));
    let js_glue = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/template/js_glue.js"
    ));
    script.push_str(js_glue);

    let code = v8::String::new(scope, &script).unwrap();
    let script_origin = create_script_origin(scope, "wasm_mode_entry");
    let script = v8::Script::compile(scope, code, Some(&script_origin)).unwrap();

    script.run(scope);
    drop(dtors);
    Ok(())
}

#[derive(serde::Deserialize, Clone)]
pub struct TestArgs {
    pub package: String,
    pub file_and_index: Vec<(String, Vec<std::ops::Range<u32>>)>,
}

pub fn get_moonrun_version() -> String {
    format!(
        "{} ({} {})",
        env!("CARGO_PKG_VERSION"),
        env!("VERGEN_GIT_SHA"),
        std::env!("VERGEN_BUILD_DATE")
    )
}

#[derive(Debug, clap::Parser)]
#[command(version = get_moonrun_version())]
struct Commandline {
    /// The path of the file to run
    #[clap(required_unless_present = "interactive")]
    path: Option<PathBuf>,

    /// Additional arguments
    #[clap(allow_hyphen_values = true)]
    args: Vec<String>,

    /// Don't print stack trace
    #[clap(long)]
    no_stack_trace: bool,

    #[clap(long)]
    test_args: Option<String>,

    #[clap(long)]
    stack_size: Option<String>,

    #[clap(short, long)]
    interactive: bool,
}

fn run_interactive() -> anyhow::Result<()> {
    loop {
        let stdin = io::stdin();
        let mut handle = stdin.lock();

        // read the length (4 bytes) first
        let mut len_bytes = [0u8; 4];
        handle.read_exact(&mut len_bytes)?;
        let length = i32::from_le_bytes(len_bytes);

        // read the wasm byte sequence
        let mut input = vec![0u8; length as usize];
        handle.read_exact(&mut input)?;

        wasm_mode(Source::Bytes(input), &[], false, None)?;
        const END_MARKER: [u8; 4] = [0xFF, 0xFE, 0xFD, 0xFC];
        io::stdout().write_all(&END_MARKER)?;
        io::stdout().write_all(b"\n")?;

        io::stdout().flush()?;
    }
}

fn initialize_v8() -> anyhow::Result<()> {
    v8::V8::set_flags_from_string("--experimental-wasm-exnref");
    v8::V8::set_flags_from_string("--experimental-wasm-imported-strings");
    let platform = v8::new_default_platform(0, false).make_shared();
    v8::V8::initialize_platform(platform);
    v8::V8::initialize();
    Ok(())
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::FmtSubscriber::builder()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .compact()
        .init();

    let matches = Commandline::parse();

    if matches.interactive {
        initialize_v8()?;
        run_interactive()
    } else {
        let file = matches.path.as_ref().unwrap();

        if !file.exists() {
            anyhow::bail!("no such file");
        }

        if let Some(stack_size) = matches.stack_size {
            set_flags_from_string(&format!("--stack-size={stack_size}"));
        }

        match file.extension().unwrap().to_str() {
            Some("wasm") => {
                initialize_v8()?;
                wasm_mode(
                    Source::File(file),
                    &matches.args,
                    matches.no_stack_trace,
                    matches.test_args,
                )
            }
            _ => anyhow::bail!("Unsupported file type"),
        }
    }
}
