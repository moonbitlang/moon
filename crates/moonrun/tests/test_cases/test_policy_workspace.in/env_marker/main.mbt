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
enum HostPlatform {
  Linux = 0
  MacOS = 1
  Windows = 2
}

///|
#unsafe_skip_stub_check
fn host_platform() -> HostPlatform = "moonbitlang/async" "runtime/get_platform"

///|
async fn main {
  assert_eq(@env.get_env_var("MOONRUN_INHERITED_POLICY"), None)
  assert_eq(@env.get_env_var("MOONRUN_POLICY_ENV"), Some("inherited"))
  let (command, args) = if host_platform() is Windows {
    (
      "cmd.exe",
      [
        "/D", "/C", "if defined MOONRUN_INHERITED_POLICY (echo visible) else (echo missing)",
      ],
    )
  } else {
    (
      "/bin/sh",
      [
        "-c", "if [ -n \"${MOONRUN_INHERITED_POLICY+x}\" ]; then printf visible; else printf missing; fi",
      ],
    )
  }
  let (code, output) = @process.collect_stdout(command, args)
  assert_eq(code, 0)
  assert_eq(output.text().trim(char_set="\n\r "), "missing")
}
