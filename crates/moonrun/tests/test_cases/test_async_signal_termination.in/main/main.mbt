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
fn signal_by_index(index : Int) -> Int =
  "moonbitlang/async" "signal/get_signal_by_index"

///|
fn terminate_process_by_signal(signal : Int) =
  "moonbitlang/async" "runtime/terminate_process_by_signal"

///|
fn main {
  terminate_process_by_signal(signal_by_index(0))
  println("unreachable")
}
