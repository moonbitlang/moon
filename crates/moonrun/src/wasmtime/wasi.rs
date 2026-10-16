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

//! Wasmtime setup for the shared WASIp1 registrations.

use super::StoreData;
use crate::wasi;

pub(super) fn register_imports(linker: &mut wasmtime::Linker<StoreData>) -> wasmtime::Result<()> {
    wasi::imports::register(&mut super::wasm_imports::Registry {
        linker,
        data: StoreData::wasi,
    })
}
