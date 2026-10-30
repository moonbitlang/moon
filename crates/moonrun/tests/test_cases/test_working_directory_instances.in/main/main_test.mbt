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
async test "ambient cwd is observed by each process" {
  let input = @fs.read_file("input.txt").text()
  let output = if input == "first" { b"first" } else { b"second" }
  @fs.write_file("output.txt", output, create_mode=CreateOrTruncate)
  match @env.current_dir() {
    Some(cwd) => @fs.write_file(
      "cwd.txt",
      cwd,
      create_mode=CreateOrTruncate,
    )
    None => abort("current directory is unavailable")
  }
}
