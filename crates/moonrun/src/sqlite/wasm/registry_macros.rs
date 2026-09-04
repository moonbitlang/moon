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

#[cfg(feature = "v8")]
macro_rules! decode_wasm_arg {
    ($args:ident, i32) => {
        $args.next_i32()
    };
    ($args:ident, u32) => {
        $args.next_u32()
    };
    ($args:ident, i64) => {
        $args.next_i64()
    };
    ($args:ident, f64) => {
        $args.next_f64()
    };
    ($args:ident, u64) => {
        $args.next_u64()
    };
}

#[cfg(feature = "v8")]
macro_rules! wasm_arg_count {
    () => {
        0_i32
    };
    ($head:ident $(, $tail:ident)*) => {
        1_i32 + $crate::sqlite::wasm::registry_macros::wasm_arg_count!($($tail),*)
    };
}

#[cfg(feature = "v8")]
macro_rules! decode_sqlite_args {
    ($scope:ident, $args:ident,) => {
        Ok::<(), V8ImportError>(())
    };
    ($scope:ident, $args:ident, $($arg:ident : $arg_ty:ident),+ $(,)?) => {{
        let mut import_args = ImportArgs::new($scope, &$args);
        (|| -> Result<_, V8ImportError> {
            $(
                let $arg = $crate::sqlite::wasm::registry_macros::decode_wasm_arg!(
                    import_args,
                    $arg_ty
                )?;
            )+
            Ok(($($arg,)+))
        })()
    }};
}

#[cfg(feature = "v8")]
macro_rules! set_sqlite_import_return {
    ($scope:ident, $ret:ident, i32, $value:expr) => {
        $ret.set_int32($value)
    };
    ($scope:ident, $ret:ident, u32, $value:expr) => {
        $ret.set_int32($value as i32)
    };
    ($scope:ident, $ret:ident, i64, $value:expr) => {
        $ret.set(v8::BigInt::new_from_i64($scope, $value).into())
    };
    ($scope:ident, $ret:ident, f64, $value:expr) => {
        $ret.set(v8::Number::new($scope, $value).into())
    };
    ($scope:ident, $ret:ident, u64, $value:expr) => {
        $ret.set(v8::BigInt::new_from_u64($scope, $value).into())
    };
    ($scope:ident, $ret:ident, void, $value:expr) => {{
        // Keep the generated callback uniform; wasm ignores the JavaScript
        // return value for a void import.
        let _ = ($value, &mut $ret);
    }};
}

#[cfg(feature = "v8")]
macro_rules! finish_sqlite_import {
    ($scope:ident, $ret:ident, $name:expr, $ret_ty:ident, $result:expr) => {
        match $result {
            Ok(value) => $crate::sqlite::wasm::registry_macros::set_sqlite_import_return!(
                $scope, $ret, $ret_ty, value
            ),
            Err(error) => {
                crate::v8::context::throw_import_error($scope, MOONBIT_SQLITE_MODULE, $name, error)
            }
        }
    };
}

#[cfg(feature = "v8")]
macro_rules! invoke_sqlite_import {
    (
        $scope:ident,
        $args:ident,
        Runtime::$callback:ident,
        ($($arg:ident),*)
    ) => {{
        // SAFETY: `register_imports` installs the retained `V8RunContext`
        // pointer with this callback.
        let context: &crate::v8::context::V8RunContext =
            unsafe { crate::v8::context::callback_context(&$args) };
        Ok::<_, SqliteError>(context.runtime().$callback($($arg),*))
    }};
    (
        $scope:ident,
        $args:ident,
        SqliteHost::$callback:ident,
        ($($arg:ident),*)
    ) => {{
        // SAFETY: `register_imports` installs the retained `V8RunContext`
        // pointer with this callback.
        let context: &crate::v8::context::V8RunContext =
            unsafe { crate::v8::context::callback_context(&$args) };
        context
            .runtime()
            .sqlite()
            .$callback($($arg),*)
            .map_err(SqliteError::from)
    }};
    (
        $scope:ident,
        $args:ident,
        $module:ident::$callback:ident,
        ($($arg:ident),*)
    ) => {{
        // SAFETY: `register_imports` installs the retained `V8RunContext`
        // pointer with this callback.
        let context: &crate::v8::context::V8RunContext =
            unsafe { crate::v8::context::callback_context(&$args) };
        with_memory_context($scope, context, |context| {
            $module::$callback(context, $($arg),*)
        })
    }};
}

#[cfg(feature = "v8")]
macro_rules! register_sqlite_import {
    (
        $obj:ident,
        $registration_scope:ident,
        $context_ptr:ident,
        $module:ident::$callback:ident(
            $($arg:ident : $arg_ty:ident),* $(,)?
        ) -> $ret_ty:ident => $wasm_symbol:literal
    ) => {{
        fn callback(
            scope: &mut v8::HandleScope,
            args: v8::FunctionCallbackArguments,
            mut ret: v8::ReturnValue,
        ) {
            let result = if args.length()
                != $crate::sqlite::wasm::registry_macros::wasm_arg_count!($($arg_ty),*)
            {
                Err(SqliteError::from(V8ImportError::InvalidArgument))
            } else {
                let decoded = $crate::sqlite::wasm::registry_macros::decode_sqlite_args!(
                    scope,
                    args,
                    $($arg : $arg_ty),*
                );

                match decoded {
                    Ok(($($arg,)*)) => {
                        $crate::sqlite::wasm::registry_macros::invoke_sqlite_import!(
                            scope,
                            args,
                            $module::$callback,
                            ($($arg),*)
                        )
                    }
                    Err(error) => Err(SqliteError::from(error)),
                }
            };

            $crate::sqlite::wasm::registry_macros::finish_sqlite_import!(
                scope,
                ret,
                $wasm_symbol,
                $ret_ty,
                result
            );
        }

        crate::v8::context::register_func(
            $obj,
            $registration_scope,
            $wasm_symbol,
            callback,
            $context_ptr,
        );
    }};
}

#[cfg(all(feature = "wasmtime", not(feature = "v8")))]
macro_rules! wasmtime_arg_type {
    (i32) => {
        i32
    };
    (u32) => {
        i32
    };
    (i64) => {
        i64
    };
    (u64) => {
        i64
    };
    (f64) => {
        f64
    };
}

#[cfg(all(feature = "wasmtime", not(feature = "v8")))]
macro_rules! decode_wasmtime_arg {
    ($arg:ident, i32) => {
        $arg
    };
    ($arg:ident, u32) => {
        $arg as u32
    };
    ($arg:ident, i64) => {
        $arg
    };
    ($arg:ident, u64) => {
        $arg as u64
    };
    ($arg:ident, f64) => {
        $arg
    };
}

#[cfg(all(feature = "wasmtime", not(feature = "v8")))]
macro_rules! wasmtime_return_type {
    (void) => {
        ()
    };
    (i32) => {
        i32
    };
    (u32) => {
        i32
    };
    (i64) => {
        i64
    };
    (u64) => {
        i64
    };
    (f64) => {
        f64
    };
}

#[cfg(all(feature = "wasmtime", not(feature = "v8")))]
macro_rules! finish_wasmtime_sqlite_import {
    ($name:expr, void, $result:expr) => {
        $result.map_err(|error| {
            wasmtime::format_err!("{}.{} failed: {error:?}", MOONBIT_SQLITE_MODULE, $name)
        })
    };
    ($name:expr, i32, $result:expr) => {
        $result.map_err(|error| {
            wasmtime::format_err!("{}.{} failed: {error:?}", MOONBIT_SQLITE_MODULE, $name)
        })
    };
    ($name:expr, u32, $result:expr) => {
        $result.map(|value| value as i32).map_err(|error| {
            wasmtime::format_err!("{}.{} failed: {error:?}", MOONBIT_SQLITE_MODULE, $name)
        })
    };
    ($name:expr, i64, $result:expr) => {
        $result.map_err(|error| {
            wasmtime::format_err!("{}.{} failed: {error:?}", MOONBIT_SQLITE_MODULE, $name)
        })
    };
    ($name:expr, u64, $result:expr) => {
        $result.map(|value| value as i64).map_err(|error| {
            wasmtime::format_err!("{}.{} failed: {error:?}", MOONBIT_SQLITE_MODULE, $name)
        })
    };
    ($name:expr, f64, $result:expr) => {
        $result.map_err(|error| {
            wasmtime::format_err!("{}.{} failed: {error:?}", MOONBIT_SQLITE_MODULE, $name)
        })
    };
}

#[cfg(all(feature = "wasmtime", not(feature = "v8")))]
macro_rules! invoke_wasmtime_sqlite_import {
    (
        $caller:ident,
        Runtime::$callback:ident,
        ($($arg:ident),*)
    ) => {
        Ok::<_, SqliteError>($caller.data().runtime().$callback($($arg),*))
    };
    (
        $caller:ident,
        SqliteHost::$callback:ident,
        ($($arg:ident),*)
    ) => {
        $caller
            .data()
            .runtime()
            .sqlite()
            .$callback($($arg),*)
            .map_err(SqliteError::from)
    };
    (
        $caller:ident,
        $module:ident::$callback:ident,
        ($($arg:ident),*)
    ) => {
        with_wasmtime_context(&mut $caller, |context| {
            $module::$callback(context, $($arg),*)
        })
    };
}

#[cfg(all(feature = "wasmtime", not(feature = "v8")))]
macro_rules! register_wasmtime_sqlite_import {
    (
        $linker:ident,
        $module:ident::$callback:ident(
            $($arg:ident : $arg_ty:ident),* $(,)?
        ) -> $ret_ty:ident => $wasm_symbol:literal
    ) => {{
        $linker.func_wrap(
            MOONBIT_SQLITE_MODULE,
            $wasm_symbol,
            |caller: wasmtime::Caller<'_, crate::wasmtime::StoreData>
                $(, $arg: wasmtime_arg_type!($arg_ty))*|
                -> wasmtime::Result<wasmtime_return_type!($ret_ty)> {
                #[allow(unused_mut)]
                let mut caller = caller;
                $(let $arg = decode_wasmtime_arg!($arg, $arg_ty);)*
                let result = invoke_wasmtime_sqlite_import!(
                    caller,
                    $module::$callback,
                    ($($arg),*)
                );
                finish_wasmtime_sqlite_import!($wasm_symbol, $ret_ty, result)
            },
        )?;
    }};
}

macro_rules! declare_sqlite_imports {
    ($(
        $(#[$meta:meta])*
        $module:ident::$callback:ident(
            $($arg:ident : $arg_ty:ident),* $(,)?
        ) -> $ret_ty:ident => $wasm_symbol:literal;
    )*) => {
        /// Register every SQLite import against one retained V8 run context.
        ///
        /// # Safety
        ///
        /// `context_ptr` must remain valid whenever a registered callback can
        /// be invoked.
        #[cfg(feature = "v8")]
        pub(super) unsafe fn register_imports<'s>(
            obj: v8::Local<'s, v8::Object>,
            scope: &mut v8::HandleScope<'s>,
            context_ptr: *const V8RunContext,
        ) {
            $(
                $(#[$meta])*
                $crate::sqlite::wasm::registry_macros::register_sqlite_import!(
                    obj,
                    scope,
                    context_ptr,
                    $module::$callback($($arg : $arg_ty),*)
                        -> $ret_ty => $wasm_symbol
                );
            )*
        }
        #[cfg(all(feature = "wasmtime", not(feature = "v8")))]
        pub(crate) fn register_wasmtime_imports(
            linker: &mut wasmtime::Linker<crate::wasmtime::StoreData>,
        ) -> wasmtime::Result<()> {
            $(
                $(#[$meta])*
                $crate::sqlite::wasm::registry_macros::register_wasmtime_sqlite_import!(
                    linker,
                    $module::$callback($($arg : $arg_ty),*)
                        -> $ret_ty => $wasm_symbol
                );
            )*
            Ok(())
        }
    };
}

pub(super) use declare_sqlite_imports;
#[cfg(feature = "v8")]
pub(super) use {
    decode_sqlite_args, decode_wasm_arg, finish_sqlite_import, invoke_sqlite_import,
    register_sqlite_import, set_sqlite_import_return, wasm_arg_count,
};
#[cfg(all(feature = "wasmtime", not(feature = "v8")))]
pub(super) use {
    decode_wasmtime_arg, finish_wasmtime_sqlite_import, invoke_wasmtime_sqlite_import,
    register_wasmtime_sqlite_import, wasmtime_arg_type, wasmtime_return_type,
};
