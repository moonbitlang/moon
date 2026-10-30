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

struct CrashBox {
  value : Int
}

fn abort_with_tuple(data : (Int, String), opt : Option[Int]) -> Unit {
  ignore((data, opt))
  abort("abort_with_tuple")
}

fn[T] abort_generic(value : T) -> T {
  ignore(value)
  abort("abort_generic")
}

fn panic_with_result(res : Result[Int, String]) -> Unit {
  ignore(res)
  panic()
}

fn abort_via_closure(seed : Int) -> Unit {
  let inner = fn(_x : Int) -> Int { abort("abort_via_closure") }
  ignore(inner(seed))
}

fn CrashBox::abort_method(self : CrashBox, flag : UInt16) -> UInt {
  ignore((self, flag))
  abort("abort_method")
}

fn default_abort_chain() -> Unit {
  abort_with_tuple((1, "stack"), Some(7))
}

test "stacktrace test abort generic tuple" {
  ignore(abort_generic((9, "tuple-test")))
}

test "stacktrace test abort method" {
  ignore(CrashBox::{ value: 2 }.abort_method(2))
}

test "stacktrace test abort closure" {
  abort_via_closure(13)
}

test "stacktrace test panic result" {
  panic_with_result(Ok(9))
}

fn main {
  let args = @env.args()
  if args.length() >= 2 {
    match args[1] {
      "abort-generic-int" => ignore(abort_generic(42))
      "abort-generic-tuple" => ignore(abort_generic((9, "tuple")))
      "abort-method" => ignore(CrashBox::{ value: 1 }.abort_method(1))
      "abort-closure" => abort_via_closure(11)
      "panic-result" => panic_with_result(Ok(3))
      _ => default_abort_chain()
    }
  } else {
    default_abort_chain()
  }
}
