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

struct CrashBox {
  value : Int
}

fn abort_with_tuple(data : (Int, String), opt : Option[Int]) -> Unit {
  ignore((data, opt))
  abort("abort_with_tuple")
}

fn abort_generic[T](value : T) -> T {
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
