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

use crate::async_host::{AsyncHost, AsyncHostError, AsyncHostResult};
use crate::run_termination::{RunTermination, TerminationRequest};
use crate::v8::context::{V8ImportError, V8MemoryBinding, V8RunContext};

pub(super) use crate::v8::context::ImportArgs;

pub(super) fn callback_context<'s>(args: &v8::FunctionCallbackArguments<'s>) -> &'s V8RunContext {
    // SAFETY: every async callback is registered with the pointer to the V8
    // run context retained by the V8 host imports for the complete run.
    unsafe { crate::v8::context::callback_context(args) }
}

pub(super) struct ImportContext<'a, 'scope> {
    pub(super) scope: &'a mut v8::HandleScope<'scope>,
    pub(super) host: &'a AsyncHost,
    memory_binding: &'a V8MemoryBinding,
    termination_request: &'a TerminationRequest,
}

impl<'a, 'scope> ImportContext<'a, 'scope> {
    pub(super) fn new(scope: &'a mut v8::HandleScope<'scope>, context: &'a V8RunContext) -> Self {
        Self {
            scope,
            host: context.runtime().async_state(),
            memory_binding: context.memory_binding(),
            termination_request: context.termination_request(),
        }
    }

    pub(super) fn request_termination(&mut self, termination: RunTermination) {
        self.termination_request.request(termination);
        // Termination cannot be caught by the guest's JavaScript glue. The run
        // loop converts the recorded request into its runtime outcome.
        self.scope.terminate_execution();
    }

    pub(super) fn with_host_and_memory_mut<T>(
        &mut self,
        f: impl FnOnce(&AsyncHost, &mut [u8]) -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        let host = self.host;
        self.memory_binding
            .with_memory_mut(self.scope, |memory| f(host, memory))
    }

    pub(super) fn with_memory_mut<T>(
        &mut self,
        f: impl FnOnce(&mut [u8]) -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        self.with_host_and_memory_mut(|_, memory| f(memory))
    }
}

impl From<V8ImportError> for AsyncHostError {
    fn from(error: V8ImportError) -> Self {
        match error {
            V8ImportError::Fault => Self::Fault,
            V8ImportError::InvalidArgument => Self::Inval,
        }
    }
}

pub(super) fn throw_import_error(
    scope: &mut v8::HandleScope,
    import_name: &str,
    error: AsyncHostError,
) {
    crate::v8::context::throw_import_error(scope, super::MOONBIT_ASYNC_MODULE, import_name, error);
}

pub(super) trait FinishVoid {
    fn finish_void(self, scope: &mut v8::HandleScope, ret: &mut v8::ReturnValue, import_name: &str);
}

impl FinishVoid for () {
    fn finish_void(
        self,
        _scope: &mut v8::HandleScope,
        ret: &mut v8::ReturnValue,
        _import_name: &str,
    ) {
        ret.set_undefined();
    }
}

impl FinishVoid for AsyncHostResult<()> {
    fn finish_void(
        self,
        scope: &mut v8::HandleScope,
        ret: &mut v8::ReturnValue,
        import_name: &str,
    ) {
        match self {
            Ok(()) => ret.set_undefined(),
            Err(error) => throw_import_error(scope, import_name, error),
        }
    }
}

pub(super) trait FinishI32 {
    fn finish_i32(self, scope: &mut v8::HandleScope, ret: &mut v8::ReturnValue, import_name: &str);
}

impl FinishI32 for i32 {
    fn finish_i32(
        self,
        _scope: &mut v8::HandleScope,
        ret: &mut v8::ReturnValue,
        _import_name: &str,
    ) {
        ret.set_int32(self);
    }
}

impl FinishI32 for u32 {
    fn finish_i32(
        self,
        _scope: &mut v8::HandleScope,
        ret: &mut v8::ReturnValue,
        _import_name: &str,
    ) {
        ret.set_uint32(self);
    }
}

impl FinishI32 for AsyncHostResult<i32> {
    fn finish_i32(self, scope: &mut v8::HandleScope, ret: &mut v8::ReturnValue, import_name: &str) {
        match self {
            Ok(value) => ret.set_int32(value),
            Err(error) => throw_import_error(scope, import_name, error),
        }
    }
}

impl FinishI32 for AsyncHostResult<u32> {
    fn finish_i32(self, scope: &mut v8::HandleScope, ret: &mut v8::ReturnValue, import_name: &str) {
        match self {
            Ok(value) => ret.set_uint32(value),
            Err(error) => throw_import_error(scope, import_name, error),
        }
    }
}

pub(super) trait FinishI64 {
    fn finish_i64(self, scope: &mut v8::HandleScope, ret: &mut v8::ReturnValue, import_name: &str);
}

impl FinishI64 for i64 {
    fn finish_i64(
        self,
        scope: &mut v8::HandleScope,
        ret: &mut v8::ReturnValue,
        _import_name: &str,
    ) {
        ret.set(v8::BigInt::new_from_i64(scope, self).into());
    }
}

impl FinishI64 for u64 {
    fn finish_i64(
        self,
        scope: &mut v8::HandleScope,
        ret: &mut v8::ReturnValue,
        _import_name: &str,
    ) {
        ret.set(v8::BigInt::new_from_u64(scope, self).into());
    }
}

impl FinishI64 for AsyncHostResult<i64> {
    fn finish_i64(self, scope: &mut v8::HandleScope, ret: &mut v8::ReturnValue, import_name: &str) {
        match self {
            Ok(value) => value.finish_i64(scope, ret, import_name),
            Err(error) => throw_import_error(scope, import_name, error),
        }
    }
}

impl FinishI64 for AsyncHostResult<u64> {
    fn finish_i64(self, scope: &mut v8::HandleScope, ret: &mut v8::ReturnValue, import_name: &str) {
        match self {
            Ok(value) => value.finish_i64(scope, ret, import_name),
            Err(error) => throw_import_error(scope, import_name, error),
        }
    }
}
