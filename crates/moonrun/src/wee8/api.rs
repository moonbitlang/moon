// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

//! Private numeric Wasm interface over the wee8 archive. All C ownership rules
//! live here. Stores and their borrowed handles stay on their creating thread;
//! Rust host state is retained through callback finalization. No JS handles cross
//! this interface. V8 still uses JS objects internally.

use super::ffi;
use std::{
    cell::{Cell, RefCell},
    ffi::c_void,
    panic::{AssertUnwindSafe, catch_unwind},
    ptr::{self, NonNull},
    rc::Rc,
    sync::OnceLock,
};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Interface(String),
    #[error("{0}")]
    Host(String),
    #[error("{0}")]
    Trap(String),
}

impl From<String> for Error {
    fn from(message: String) -> Self {
        Self::Interface(message)
    }
}

impl From<&str> for Error {
    fn from(message: &str) -> Self {
        Self::Interface(message.into())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    I32,
    I64,
    F32,
    F64,
}

impl Kind {
    fn raw(self) -> u8 {
        match self {
            Self::I32 => 0,
            Self::I64 => 1,
            Self::F32 => 2,
            Self::F64 => 3,
        }
    }

    fn from_raw(kind: u8) -> Result<Self> {
        match kind {
            0 => Ok(Self::I32),
            1 => Ok(Self::I64),
            2 => Ok(Self::F32),
            3 => Ok(Self::F64),
            _ => Err("the MVP supports only numeric host values".into()),
        }
    }
}

impl Value {
    fn raw(self) -> ffi::wasm_val_t {
        use ffi::wasm_val_t__bindgen_ty_1 as Payload;
        match self {
            Self::I32(i32) => ffi::wasm_val_t {
                kind: 0,
                of: Payload { i32_: i32 },
            },
            Self::I64(i64) => ffi::wasm_val_t {
                kind: 1,
                of: Payload { i64_: i64 },
            },
            Self::F32(f32) => ffi::wasm_val_t {
                kind: 2,
                of: Payload { f32_: f32 },
            },
            Self::F64(f64) => ffi::wasm_val_t {
                kind: 3,
                of: Payload { f64_: f64 },
            },
        }
    }

    // The discriminant is checked before accessing the corresponding union field.
    unsafe fn from_raw(value: &ffi::wasm_val_t) -> Result<Self> {
        unsafe {
            match Kind::from_raw(value.kind)? {
                Kind::I32 => Ok(Self::I32(value.of.i32_)),
                Kind::I64 => Ok(Self::I64(value.of.i64_)),
                Kind::F32 => Ok(Self::F32(value.of.f32_)),
                Kind::F64 => Ok(Self::F64(value.of.f64_)),
            }
        }
    }
}

/// V8 initializes one process-wide platform. It must outlive every isolate.
/// Store creation does not mutate the engine; each Store owns a separate isolate.
struct ProcessEngine(NonNull<ffi::wasm_engine_t>);
unsafe impl Send for ProcessEngine {}
unsafe impl Sync for ProcessEngine {}

pub fn initialize() -> Result<()> {
    process_engine().map(|_| ())
}

fn process_engine() -> Result<&'static ProcessEngine> {
    static ENGINE: OnceLock<std::result::Result<ProcessEngine, String>> = OnceLock::new();
    ENGINE
        .get_or_init(|| {
            NonNull::new(unsafe { ffi::wasm_engine_new() })
                .map(ProcessEngine)
                .ok_or_else(|| "failed to initialize wee8".into())
        })
        .as_ref()
        .map_err(|error| Error::Interface(error.clone()))
    // Intentionally no Drop: V8 cannot be reinitialized after engine deletion.
}

struct State<T> {
    data: RefCell<T>,
    executing: Cell<bool>,
    memory: Cell<*mut ffi::wasm_memory_t>,
    host_error: RefCell<Option<String>>,
}

// Both instantiation (which can run a start function) and exported calls enter
// guest code. Reject re-entry through another instance in the same store too:
// it could otherwise grow a memory while a host callback holds its Rust view.
struct Execution<'a, T>(&'a State<T>);

impl<T> State<T> {
    fn enter(&self) -> Result<Execution<'_, T>> {
        if self.executing.replace(true) {
            return Err("wee8 does not support guest re-entry from a host callback yet".into());
        }
        Ok(Execution(self))
    }
}

impl<T> Drop for Execution<'_, T> {
    fn drop(&mut self) {
        self.0.memory.set(ptr::null_mut());
        self.0.executing.set(false);
    }
}

pub struct Store<T> {
    raw: NonNull<ffi::wasm_store_t>,
    state: Rc<State<T>>,
}

impl<T> Store<T> {
    pub fn new(data: T) -> Result<Self> {
        let raw = NonNull::new(unsafe { ffi::wasm_store_new(process_engine()?.0.as_ptr()) })
            .ok_or("failed to create wee8 store")?;
        Ok(Self {
            raw,
            state: Rc::new(State {
                data: RefCell::new(data),
                executing: Cell::new(false),
                memory: Cell::new(ptr::null_mut()),
                host_error: RefCell::new(None),
            }),
        })
    }

    fn take_trap(&self, trap: *mut ffi::wasm_trap_t) -> Error {
        let message = unsafe { take_trap(trap) };
        match self.state.host_error.borrow_mut().take() {
            Some(error) => Error::Host(error),
            None => Error::Trap(message),
        }
    }
}

impl<T> Drop for Store<T> {
    fn drop(&mut self) {
        // Native finalizers still have their own Rc to Rust state during teardown.
        unsafe { ffi::wasm_store_delete(self.raw.as_ptr()) }
    }
}

/// Valid only for the duration of a synchronous host call. No guest re-entry is
/// exposed yet. A memory borrow cannot escape this closure or cross guest growth.
pub struct Caller<'a, T>(&'a State<T>);

impl<T> Caller<'_, T> {
    pub fn with_data<R>(&self, operation: impl FnOnce(&T) -> R) -> R {
        operation(&self.0.data.borrow())
    }

    pub fn with_memory<R>(&self, operation: impl FnOnce(&T, &mut [u8]) -> R) -> Result<R> {
        let memory = self.0.memory.get();
        if memory.is_null() {
            return Err("wee8 host memory requires an exported `memory` and is unavailable during module start".into());
        }
        // The mutable state borrow also rejects nested memory views. Wasm1
        // validation excludes shared memory, and callbacks cannot re-enter Wasm.
        let data = self
            .0
            .data
            .try_borrow_mut()
            .map_err(|_| "nested guest memory borrow")?;
        let size = unsafe { ffi::wasm_memory_data_size(memory) };
        let bytes = if size == 0 {
            &mut []
        } else {
            unsafe { std::slice::from_raw_parts_mut(ffi::wasm_memory_data(memory).cast(), size) }
        };
        Ok(operation(&data, bytes))
    }
}

type Callback<T> = dyn Fn(Caller<'_, T>, &[Value]) -> Result<Vec<Value>>;

struct CallbackEnvironment<T> {
    state: Rc<State<T>>,
    store: *mut ffi::wasm_store_t,
    results: Vec<Kind>,
    callback: Box<Callback<T>>,
}

unsafe extern "C" fn finalize_callback<T>(env: *mut c_void) {
    // The engine owns the environment until finalization, which may happen when
    // the store is deleted, later than deletion of the host's function handle.
    // Never let user capture destructors unwind through C++.
    if catch_unwind(AssertUnwindSafe(|| unsafe {
        drop(Box::from_raw(env.cast::<CallbackEnvironment<T>>()));
    }))
    .is_err()
    {
        std::process::abort();
    }
}

unsafe extern "C" fn call_host<T>(
    env: *mut c_void,
    args: *const ffi::wasm_val_vec_t,
    results: *mut ffi::wasm_val_vec_t,
) -> *mut ffi::wasm_trap_t {
    unsafe {
        let env = &*env.cast::<CallbackEnvironment<T>>();
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            let args = &*args;
            let args = if args.size == 0 {
                &[]
            } else {
                std::slice::from_raw_parts(args.data, args.size)
            };
            let values = args
                .iter()
                .map(|value| Value::from_raw(value))
                .collect::<Result<Vec<_>>>()?;
            let values = (env.callback)(Caller(&env.state), &values)?;
            if values.len() != env.results.len()
                || values
                    .iter()
                    .zip(&env.results)
                    .any(|(v, k)| v.raw().kind != k.raw())
            {
                return Err("host callback returned the wrong result signature".into());
            }
            // Both implementations supply result slots. This descriptor is
            // borrowed; do not replace or free it, even for a zero-result call.
            if (*results).size != values.len() {
                return Err("invalid callback result capacity".into());
            }
            for (index, value) in values.into_iter().enumerate() {
                (*results).data.add(index).write(value.raw());
            }
            Ok::<(), Error>(())
        }));
        let mut error = match outcome {
            Ok(Ok(())) => return ptr::null_mut(),
            Ok(Err(error)) => error.to_string(),
            Err(_) => "Rust host callback panicked".into(),
        };
        // Wasm C API trap messages include the trailing NUL in their size.
        *env.state.host_error.borrow_mut() = Some(error.clone());
        error.push('\0');
        let message = ffi::wasm_byte_vec_t {
            size: error.len(),
            data: error.as_ptr().cast_mut().cast(),
        };
        ffi::wasm_trap_new(env.store, &message)
    }
}

impl<T: 'static> Store<T> {
    pub fn compile<'store>(&'store self, bytes: &[u8]) -> Result<Module<'store, T>> {
        // Engines may accept proposals that their C adapters cannot describe.
        // Validate our supported scope before any engine-side introspection can
        // reach an unimplemented conversion (tags, GC types, SIMD, etc.). This
        // also excludes shared memory, whose concurrent mutation would make our
        // copied memory access unsafe. MoonBit's linear-memory backend uses
        // bulk memory and loops with parameters (multi-value). Neither requires
        // reference values at the C boundary. Host calls remain single-result.
        let features = wasmparser::WasmFeatures::WASM1
            | wasmparser::WasmFeatures::BULK_MEMORY
            | wasmparser::WasmFeatures::MULTI_VALUE;
        wasmparser::Validator::new_with_features(features)
            .validate_all(bytes)
            .map_err(|error| {
                format!("outside wee8's supported numeric Wasm subset or invalid Wasm: {error}")
            })?;
        let bytes = ffi::wasm_byte_vec_t {
            size: bytes.len(),
            data: bytes.as_ptr().cast_mut().cast(),
        };
        let raw = NonNull::new(unsafe { ffi::wasm_module_new(self.raw.as_ptr(), &bytes) })
            .ok_or("module validation or compilation failed")?;
        Ok(Module { raw, store: self })
    }

    pub fn host_func<'store>(
        &'store self,
        params: &[Kind],
        results: &[Kind],
        callback: impl Fn(Caller<'_, T>, &[Value]) -> Result<Vec<Value>> + 'static,
    ) -> Result<HostFunc<'store, T>> {
        if results.len() > 1 {
            return Err("wee8 host functions return at most one value".into());
        }
        unsafe {
            let mut params_vec = ffi::wasm_valtype_vec_t {
                size: 0,
                data: ptr::null_mut(),
            };
            let mut results_vec = ffi::wasm_valtype_vec_t {
                size: 0,
                data: ptr::null_mut(),
            };
            let params_raw: Vec<_> = params
                .iter()
                .map(|k| ffi::wasm_valtype_new(k.raw()))
                .collect();
            let results_raw: Vec<_> = results
                .iter()
                .map(|k| ffi::wasm_valtype_new(k.raw()))
                .collect();
            ffi::wasm_valtype_vec_new(&mut params_vec, params_raw.len(), params_raw.as_ptr());
            ffi::wasm_valtype_vec_new(&mut results_vec, results_raw.len(), results_raw.as_ptr());
            // The constructors consume elements, then vectors. Do not delete
            // consumed vectors again (V8 need not reset the descriptors to zero).
            let ty = ffi::wasm_functype_new(&mut params_vec, &mut results_vec);
            if ty.is_null() {
                return Err("failed to create host function type".into());
            }
            let env = Box::into_raw(Box::new(CallbackEnvironment {
                state: Rc::clone(&self.state),
                store: self.raw.as_ptr(),
                results: results.to_vec(),
                callback: Box::new(callback),
            }));
            let raw = ffi::wasm_func_new_with_env(
                self.raw.as_ptr(),
                ty,
                Some(call_host::<T>),
                env.cast(),
                Some(finalize_callback::<T>),
            );
            ffi::wasm_functype_delete(ty);
            let raw = NonNull::new(raw).ok_or("failed to create host function")?;
            Ok(HostFunc { raw, store: self })
        }
    }
}

/// V8's C API shares compiled modules by serialization. Keep those bytes private:
/// deserializing arbitrary input as compiled code would be unsafe. This permits
/// Module reuse across fresh stores; it does not promise a zero-copy code cache.
#[derive(Debug)]
pub struct Compiled(Vec<u8>);

impl<T> Module<'_, T> {
    pub fn serialize(&self) -> Compiled {
        unsafe {
            let mut bytes = ffi::wasm_byte_vec_t {
                size: 0,
                data: ptr::null_mut(),
            };
            ffi::wasm_module_serialize(self.raw.as_ptr(), &mut bytes);
            let result = if bytes.size == 0 {
                Vec::new()
            } else {
                std::slice::from_raw_parts(bytes.data.cast(), bytes.size).to_vec()
            };
            ffi::wasm_byte_vec_delete(&mut bytes);
            Compiled(result)
        }
    }
}

impl<T> Store<T> {
    pub fn module(&self, compiled: &Compiled) -> Result<Module<'_, T>> {
        let bytes = ffi::wasm_byte_vec_t {
            size: compiled.0.len(),
            data: compiled.0.as_ptr().cast_mut().cast(),
        };
        let raw = NonNull::new(unsafe { ffi::wasm_module_deserialize(self.raw.as_ptr(), &bytes) })
            .ok_or("failed to obtain compiled module in new store")?;
        Ok(Module { raw, store: self })
    }
}

#[derive(Debug)]
pub struct Import {
    pub module: String,
    pub name: String,
    pub params: Vec<Kind>,
    pub results: Vec<Kind>,
}

impl<T> Module<'_, T> {
    pub fn imports(&self) -> Result<Vec<Import>> {
        unsafe {
            let mut imports = ffi::wasm_importtype_vec_t {
                size: 0,
                data: ptr::null_mut(),
            };
            ffi::wasm_module_imports(self.raw.as_ptr(), &mut imports);
            let result = (0..imports.size)
                .map(|index| {
                    let import = *imports.data.add(index);
                    let read_name = |name: *const ffi::wasm_byte_vec_t| {
                        if (*name).size == 0 {
                            String::new()
                        } else {
                            String::from_utf8_lossy(std::slice::from_raw_parts(
                                (*name).data.cast(),
                                (*name).size,
                            ))
                            .into_owned()
                        }
                    };
                    let module = read_name(ffi::wasm_importtype_module(import));
                    let name = read_name(ffi::wasm_importtype_name(import));
                    let function =
                        ffi::wasm_externtype_as_functype_const(ffi::wasm_importtype_type(import));
                    if function.is_null() {
                        return Err(Error::Interface(format!(
                            "unsupported non-function import {module}::{name}"
                        )));
                    }
                    let read_kinds = |types: *const ffi::wasm_valtype_vec_t| {
                        (0..(*types).size)
                            .map(|i| Kind::from_raw(ffi::wasm_valtype_kind(*(*types).data.add(i))))
                            .collect::<Result<Vec<_>>>()
                    };
                    Ok(Import {
                        module,
                        name,
                        params: read_kinds(ffi::wasm_functype_params(function))?,
                        results: read_kinds(ffi::wasm_functype_results(function))?,
                    })
                })
                .collect();
            ffi::wasm_importtype_vec_delete(&mut imports);
            result
        }
    }
}

pub struct HostFunc<'store, T> {
    raw: NonNull<ffi::wasm_func_t>,
    store: &'store Store<T>,
}

impl<T> Drop for HostFunc<'_, T> {
    fn drop(&mut self) {
        unsafe { ffi::wasm_func_delete(self.raw.as_ptr()) }
    }
}

pub struct Module<'store, T> {
    raw: NonNull<ffi::wasm_module_t>,
    store: &'store Store<T>,
}

impl<T> Drop for Module<'_, T> {
    fn drop(&mut self) {
        unsafe { ffi::wasm_module_delete(self.raw.as_ptr()) }
    }
}

impl<'store, T> Module<'store, T> {
    /// Imports follow module declaration order. The MVP accepts function imports.
    pub fn instantiate(&self, imports: &[&HostFunc<'_, T>]) -> Result<Instance<'store, T>> {
        let _execution = self.store.state.enter()?;
        if imports.iter().any(|f| !ptr::eq(f.store, self.store)) {
            return Err("imports must belong to the module's store".into());
        }
        unsafe {
            let mut types = ffi::wasm_importtype_vec_t {
                size: 0,
                data: ptr::null_mut(),
            };
            ffi::wasm_module_imports(self.raw.as_ptr(), &mut types);
            let count = types.size;
            ffi::wasm_importtype_vec_delete(&mut types);
            if count != imports.len() {
                return Err("import count mismatch".into());
            }
            let mut raw_imports: Vec<_> = imports
                .iter()
                .map(|f| ffi::wasm_func_as_extern(f.raw.as_ptr()))
                .collect();
            let imports = ffi::wasm_extern_vec_t {
                size: raw_imports.len(),
                data: raw_imports.as_mut_ptr(),
            };
            let mut trap = ptr::null_mut();
            let raw = ffi::wasm_instance_new(
                self.store.raw.as_ptr(),
                self.raw.as_ptr(),
                &imports,
                &mut trap,
            );
            if !trap.is_null() {
                return Err(self.store.take_trap(trap));
            }
            let raw = NonNull::new(raw).ok_or("instantiation failed")?;
            let mut exports = ffi::wasm_extern_vec_t {
                size: 0,
                data: ptr::null_mut(),
            };
            ffi::wasm_instance_exports(raw.as_ptr(), &mut exports);
            let mut types = ffi::wasm_exporttype_vec_t {
                size: 0,
                data: ptr::null_mut(),
            };
            ffi::wasm_module_exports(self.raw.as_ptr(), &mut types);
            let names = (0..types.size)
                .map(|index| {
                    let name = &*ffi::wasm_exporttype_name(*types.data.add(index));
                    if name.size == 0 {
                        Vec::new()
                    } else {
                        std::slice::from_raw_parts(name.data.cast(), name.size).to_vec()
                    }
                })
                .collect();
            ffi::wasm_exporttype_vec_delete(&mut types);
            Ok(Instance {
                raw,
                exports,
                names,
                store: self.store,
            })
        }
    }
}

pub struct Instance<'store, T> {
    raw: NonNull<ffi::wasm_instance_t>,
    exports: ffi::wasm_extern_vec_t,
    names: Vec<Vec<u8>>,
    store: &'store Store<T>,
}

impl<T> Drop for Instance<'_, T> {
    fn drop(&mut self) {
        unsafe {
            ffi::wasm_extern_vec_delete(&mut self.exports);
            ffi::wasm_instance_delete(self.raw.as_ptr());
        }
    }
}

impl<T> Instance<'_, T> {
    pub fn has_export(&self, name: &str) -> bool {
        self.names.iter().any(|export| export == name.as_bytes())
    }

    fn export(&self, name: &str) -> Result<*mut ffi::wasm_extern_t> {
        let index = self
            .names
            .iter()
            .position(|n| n == name.as_bytes())
            .ok_or_else(|| format!("missing export {name}"))?;
        if index >= self.exports.size {
            return Err("export metadata mismatch".into());
        }
        Ok(unsafe { *self.exports.data.add(index) })
    }

    pub fn call(&mut self, name: &str, args: &[Value]) -> Result<Vec<Value>> {
        let _execution = self.store.state.enter()?;
        unsafe {
            let func = ffi::wasm_extern_as_func(self.export(name)?);
            if func.is_null() {
                return Err("export is not a function".into());
            }
            let ty = ffi::wasm_func_type(func);
            if ty.is_null() {
                return Err("failed to inspect function signature".into());
            }
            let params = &*ffi::wasm_functype_params(ty);
            let returns = &*ffi::wasm_functype_results(ty);
            if returns.size > 1 {
                ffi::wasm_functype_delete(ty);
                return Err("wee8 calls support at most one return value".into());
            }
            let signature_valid = params.size == args.len()
                && args
                    .iter()
                    .enumerate()
                    .all(|(i, v)| v.raw().kind == ffi::wasm_valtype_kind(*params.data.add(i)));
            let kinds = (0..returns.size)
                .map(|i| Kind::from_raw(ffi::wasm_valtype_kind(*returns.data.add(i))))
                .collect::<Result<Vec<_>>>();
            ffi::wasm_functype_delete(ty);
            if !signature_valid {
                return Err("argument signature mismatch".into());
            }
            let kinds = kinds?;
            let mut args_raw: Vec<_> = args.iter().map(|v| v.raw()).collect();
            let mut result_raw: Vec<_> = kinds
                .iter()
                .map(|k| ffi::wasm_val_t {
                    kind: k.raw(),
                    of: ffi::wasm_val_t__bindgen_ty_1 { i64_: 0 },
                })
                .collect();
            let args = ffi::wasm_val_vec_t {
                size: args_raw.len(),
                data: args_raw.as_mut_ptr(),
            };
            let mut results = ffi::wasm_val_vec_t {
                size: result_raw.len(),
                data: result_raw.as_mut_ptr(),
            };
            let memory = self
                .names
                .iter()
                .position(|name| name == b"memory")
                .map(|index| ffi::wasm_extern_as_memory(*self.exports.data.add(index)))
                .unwrap_or(ptr::null_mut());
            self.store.state.memory.set(memory);
            let trap = ffi::wasm_func_call(func, &args, &mut results);
            if !trap.is_null() {
                return Err(self.store.take_trap(trap));
            }
            result_raw.iter().map(|v| Value::from_raw(v)).collect()
        }
    }
}

unsafe fn take_trap(trap: *mut ffi::wasm_trap_t) -> String {
    unsafe {
        let mut message = ffi::wasm_byte_vec_t {
            size: 0,
            data: ptr::null_mut(),
        };
        ffi::wasm_trap_message(trap, &mut message);
        let text = if message.size == 0 {
            "Wasm trapped".into()
        } else {
            String::from_utf8_lossy(std::slice::from_raw_parts(
                message.data.cast(),
                message.size,
            ))
            .trim_end_matches('\0')
            .to_owned()
        };
        ffi::wasm_byte_vec_delete(&mut message);
        ffi::wasm_trap_delete(trap);
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guest_extensions_preserve_the_single_result_host_boundary() {
        let store = Store::new(()).unwrap();
        let module = store
            .compile(
                &wat::parse_str(
                    r#"(module
                    (memory 1)
                    (func $pair (export "pair") (result i32 i32)
                        i32.const 20 i32.const 22)
                    (func (export "run") (result i32)
                        i32.const 0 i32.const 42 i32.const 1 memory.fill
                        i32.const 1 i32.const 0 i32.const 1 memory.copy
                        i32.const 1 i32.load8_u
                        call $pair i32.add i32.ne if unreachable end
                        call $pair i32.add))"#,
                )
                .unwrap(),
            )
            .unwrap();
        let mut instance = module.instantiate(&[]).unwrap();
        assert!(matches!(
            instance.call("pair", &[]),
            Err(Error::Interface(message)) if message.contains("at most one return value")
        ));
        assert_eq!(instance.call("run", &[]).unwrap(), [Value::I32(42)]);
        assert!(matches!(
            store.host_func(&[], &[Kind::I32, Kind::I32], |_, _| unreachable!()),
            Err(Error::Interface(message)) if message.contains("at most one value")
        ));
    }

    #[test]
    fn retained_callbacks_keep_rust_state_alive_and_contain_panics() {
        struct Data(Rc<Cell<usize>>);
        impl Drop for Data {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }
        let drops = Rc::new(Cell::new(0));
        {
            let store = Store::new(Data(Rc::clone(&drops))).unwrap();
            let module = store
                .compile(
                    &wat::parse_str(
                        r#"(module
                (import "host" "check" (func $check (param i32) (result i32)))
                (func (export "run") (param i32) (result i32) local.get 0 call $check))"#,
                    )
                    .unwrap(),
                )
                .unwrap();
            let host = store
                .host_func(&[Kind::I32], &[Kind::I32], |caller, args| {
                    caller.with_data(|data| assert_eq!(data.0.get(), 0));
                    match args {
                        [Value::I32(0)] => panic!("host panic to contain"),
                        [Value::I32(1)] => Ok(vec![Value::I64(1)]),
                        [Value::I32(value)] => Ok(vec![Value::I32(*value + 1)]),
                        _ => unreachable!(),
                    }
                })
                .unwrap();
            let mut instance = module.instantiate(&[&host]).unwrap();
            drop(host);
            drop(module);
            assert_eq!(
                instance.call("run", &[Value::I32(41)]).unwrap(),
                [Value::I32(42)]
            );
            assert!(
                matches!(instance.call("run", &[Value::I32(0)]), Err(Error::Host(message)) if message.contains("panicked"))
            );
            assert!(
                matches!(instance.call("run", &[Value::I32(1)]), Err(Error::Host(message)) if message.contains("wrong result signature"))
            );
            assert!(instance.call("run", &[Value::I64(41)]).is_err());
            assert_eq!(
                instance.call("run", &[Value::I32(41)]).unwrap(),
                [Value::I32(42)]
            );
            assert_eq!(drops.get(), 0);
        }
        assert_eq!(drops.get(), 1);
    }

    #[test]
    fn host_memory_borrow_rejects_nested_views_and_store_reentry() {
        let store = Rc::new(Store::new(()).unwrap());
        let module = store
            .compile(
                &wat::parse_str(
                    r#"(module
            (import "host" "check" (func $check))
            (memory (export "memory") 1)
            (func (export "run") call $check))"#,
                )
                .unwrap(),
            )
            .unwrap();
        let weak_store = Rc::downgrade(&store);
        let nested_bytes = wat::parse_str("(module)").unwrap();
        let host = store.host_func(&[], &[], move |caller, _| {
            caller.with_memory(|_, memory| {
                memory[0] = 42;
                assert!(caller.with_memory(|_, _| ()).is_err());
                let store = weak_store.upgrade().unwrap();
                let module = store.compile(&nested_bytes).unwrap();
                assert!(matches!(module.instantiate(&[]), Err(Error::Interface(message)) if message.contains("re-entry")));
                assert_eq!(memory[0], 42);
            })?;
            Ok(vec![])
        }).unwrap();
        let mut instance = module.instantiate(&[&host]).unwrap();
        instance.call("run", &[]).unwrap();
        // A rejected nested operation must not release the outer call's guard
        // or leave the store permanently active after that outer call returns.
        instance.call("run", &[]).unwrap();
    }
}
