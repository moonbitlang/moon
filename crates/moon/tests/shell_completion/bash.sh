#!/usr/bin/env bash
# moon: The build system and package manager for MoonBit.
# Copyright (C) 2024 International Digital Economy Academy
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU Affero General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU Affero General Public License for more details.
#
# You should have received a copy of the GNU Affero General Public License
# along with this program.  If not, see <https://www.gnu.org/licenses/>.
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
