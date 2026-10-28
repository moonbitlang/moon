#!/usr/bin/env bash
# moon: The build system and package manager for MoonBit.
# Copyright (C) 2026 International Digital Economy Academy
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
# either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.
#
# For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

set -euo pipefail

# rust-cache hashes every installed toolchain. Keep only the pinned compiler
# selected by the caller so runner image updates do not split compatible caches.
active_toolchain=$(rustup show active-toolchain)
active_toolchain=${active_toolchain%% *}
installed_toolchains=$(rustup toolchain list --quiet)
while IFS= read -r toolchain; do
  toolchain=${toolchain%$'\r'}
  if [[ -n "$toolchain" && "$toolchain" != "$active_toolchain" ]]; then
    rustup toolchain uninstall "$toolchain"
  fi
done <<< "$installed_toolchains"
