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

//! Backend-neutral `moonbitlang/async` import definitions and ABI validation.
//!
//! This layer owns the canonical import list and typed registrations. Backend
//! adapters decode arguments, acquire guest memory, convert results and traps.
//! Implementations use `ImportContext` independently of the selected engine.
//! Ported native-stub behavior belongs in `async_sys`; shared runtime state
//! belongs in `async_host`.

mod c_buffer;
mod context;
mod env_util;
mod event_bus;
mod event_loop;
mod fd_util;
mod fs;
mod io;
mod os_error;
mod os_string;
mod process;
mod provenance;
mod random;
mod registry;
mod runtime;
mod signal;
mod socket;
mod stdio;
mod thread_pool;
mod time;
mod tls;

pub(crate) const MOONBIT_ASYNC_MODULE: &str = "moonbitlang/async";
pub(crate) use registry::register;
