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
fn assert_true(cond : Bool, message : String) -> Unit {
  if !cond {
    abort(message)
  }
}

///|
fn main {
  let ptr = 1024U
  @memory_sanitizer.register_object_alloc(16U, ptr)
  assert_true(@memory_sanitizer.object_is_valid(ptr), "object should be valid")
  @memory_sanitizer.register_object_free(ptr)
  assert_true(!@memory_sanitizer.object_is_valid(ptr), "object should be freed")
  println("ok")
}
