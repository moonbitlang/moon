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

///|
fn host_register_object_alloc(size : UInt, ptr : UInt) -> Unit = "moonbit:ffi/memory-sanitizer" "register-object-alloc"

///|
fn host_register_object_free(ptr : UInt) -> Unit = "moonbit:ffi/memory-sanitizer" "register-object-free"

///|
fn host_object_is_valid(ptr : UInt) -> Bool = "moonbit:ffi/memory-sanitizer" "object-is-valid"

///|
pub fn register_object_alloc(size : UInt, ptr : UInt) -> Unit {
  host_register_object_alloc(size, ptr)
}

///|
pub fn register_object_free(ptr : UInt) -> Unit {
  host_register_object_free(ptr)
}

///|
pub fn object_is_valid(ptr : UInt) -> Bool {
  host_object_is_valid(ptr)
}
