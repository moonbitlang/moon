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

//! Embeddable execution support for MoonBit Wasm programs.
//!
//! The interface is experimental. It separates guest termination from
//! host-process termination and selects one Engine Backend at compile time.

#[cfg(not(any(feature = "v8", feature = "wasmtime")))]
compile_error!("moonrun requires an engine feature: `v8` or `wasmtime`");

mod async_api;
mod async_host;
mod async_sys;
mod core_api;
#[cfg(any(feature = "v8", feature = "wasmtime"))]
mod engine;
mod filesystem;
mod guest_memory;
mod memory_sanitizer;
mod network;
mod policy;
mod process;
mod resource;
mod run_signal;
mod run_termination;
mod runtime;
mod source_map;
mod sqlite;
#[cfg(feature = "v8")]
mod util;
#[cfg(feature = "v8")]
mod v8;
mod wasi;
mod wasm_diagnostic;
#[cfg(all(feature = "wasmtime", not(feature = "v8")))]
mod wasmtime;

#[cfg(any(feature = "v8", feature = "wasmtime"))]
pub use engine::{Engine, EngineConfig, Module, RunOptions, RunOutcome};
pub use run_signal::{SignalReceiver, SignalSendError, SignalSender, signal_channel};
pub use runtime::WorkingDirectory;
