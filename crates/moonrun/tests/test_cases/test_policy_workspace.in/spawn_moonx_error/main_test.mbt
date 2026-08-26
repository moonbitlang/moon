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
async test "policy transfer failure is reported by the spawn job" {
  let temp_dir = if host_platform() is Windows {
    @env.get_env_var("TEMP")
  } else {
    @env.get_env_var("TMPDIR")
  }
  match temp_dir {
    Some(path) => @fs.rmdir(path)
    None => abort("temporary directory is missing")
  }

  try {
    let _ = @process.collect_stdout("moonx", ["moonbitlang/parser/cmd/moonfmt@0.3.3"])
    abort("moonx was spawned after policy transfer failed")
  } catch {
    @os_error.OSError(_) => ()
    err => abort("unexpected process error: \{err}")
  }
}
