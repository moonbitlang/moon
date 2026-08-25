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

///|
async fn main {
  let args = @env.args()
  let moonx_args = if args.length() > 1 && args[1] == "native" {
    ["--target", "native", "moonrun/policy-test/cmd/env@1.0.0"]
  } else {
    ["moonrun/policy-test/cmd/env@1.0.0"]
  }
  let (code, output) = @process.collect_stdout("moonx", moonx_args)
  if code != 0 {
    println("nested moonx exited with code \{code}")
    return
  }
  println(output.text().trim(char_set="\n\r "))
}
