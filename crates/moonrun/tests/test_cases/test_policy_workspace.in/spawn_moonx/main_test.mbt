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
enum HostPlatform {
  Linux = 0
  MacOS = 1
  Windows = 2
}

///|
#unsafe_skip_stub_check
fn host_platform() -> HostPlatform = "moonbitlang/async" "runtime/get_platform"

///|
async test "policy inheritance stops at the direct moonrun" {
  let (command, args) = if host_platform() is Windows {
    (
      "cmd.exe",
      [
        "/D", "/C", "if \"%MOONRUN_INHERITED_POLICY%\"==\"guest\" (echo guest) else (echo changed)",
      ],
    )
  } else {
    (
      "/bin/sh",
      [
        "-c", "if [ \"$MOONRUN_INHERITED_POLICY\" = guest ]; then printf guest; else printf changed; fi",
      ],
    )
  }
  let (code, output) = @process.collect_stdout(command, args)
  assert_eq(code, 0)
  assert_eq(output.text().trim(char_set="\n\r "), "guest")

  let (code, _) = @process.collect_stdout("moonx", [
    "--experimental-policy", "deny-all.toml", "moonbitlang/parser/cmd/moonfmt@0.3.3",
  ])
  assert_eq(code, 0)
}
