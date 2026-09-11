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
