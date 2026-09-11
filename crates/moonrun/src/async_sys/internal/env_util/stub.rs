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

use crate::async_sys::ported_fns;

ported_fns! {
    #[ported(
        source = "src/internal/env_util/stub.c",
        original = "moonbitlang_async_getpid"
    )]
    pub(crate) fn get_pid() -> u32 {
        std::process::id()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_pid_matches_current_process() {
        assert_eq!(get_pid(), std::process::id());
    }
}
