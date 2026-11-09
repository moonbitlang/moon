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

# gh authenticates both release lookup and asset downloads, including redirects.
# A failed transfer may leave a partial file; overwrite it on each bounded retry.
: "${GH_TOKEN:?Set GH_TOKEN for authenticated release downloads}"
for attempt in 1 2 3; do
  if gh release download "$@" --clobber; then
    exit 0
  fi
  if (( attempt < 3 )); then
    sleep "$(( attempt * 5 ))"
  fi
done
exit 1
