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

moon_bin=${1:?Usage: bash.sh <moon-binary>}
eval "$("$moon_bin" shell-completion --shell bash)"

complete_moon() {
  COMP_WORDS=("$@")
  COMP_CWORD=$((${#COMP_WORDS[@]} - 1))
  _moon moon "${COMP_WORDS[COMP_CWORD]}" "${COMP_WORDS[COMP_CWORD - 1]}"
  printf '%s\n' "${COMPREPLY[@]}"
}

assert_contains() {
  local completions=$1
  local expected=$2
  grep -Fqx -- "$expected" <<<"$completions" || {
    printf 'missing completion %q in:\n%s\n' "$expected" "$completions" >&2
    return 1
  }
}

assert_not_contains() {
  local completions=$1
  local unexpected=$2
  if grep -Fqx -- "$unexpected" <<<"$completions"; then
    printf 'unexpected completion %q in:\n%s\n' "$unexpected" "$completions" >&2
    return 1
  fi
}

ide_completions=$(complete_moon moon ide "")
for command in peek-def find-references rename hover outline analyze doc; do
  assert_contains "$ide_completions" "$command"
done
assert_not_contains "$ide_completions" --verbose

tool_completions=$(complete_moon moon tool bu)
assert_contains "$tool_completions" build-binary-dep

build_completions=$(complete_moon moon build --ver)
assert_contains "$build_completions" --verbose
