#!/usr/bin/env zsh
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

set -eu

readonly moon_bin=${1:?Usage: zsh.zsh <moon-binary>}
readonly completion_dir=$(mktemp -d)
readonly prompt='MOON-COMPLETION> '

cleanup() {
  zpty -d moon-completion 2>/dev/null || true
  rm -rf -- "$completion_dir"
}
trap cleanup EXIT

"$moon_bin" shell-completion --shell zsh >"$completion_dir/_moon"

zmodload zsh/zpty
zpty -b moon-completion zsh -f
zpty -w moon-completion \
  "PS1='$prompt'; fpath=(${(q)completion_dir} \$fpath); autoload -Uz compinit; compinit -d ${(q)completion_dir}/.zcompdump; bindkey '^I' complete-word"

read_until() {
  local expected=$1
  local chunk
  local attempt
  REPLY=

  for attempt in {1..100}; do
    while zpty -r moon-completion chunk; do
      REPLY+=$chunk
      if [[ $REPLY == *$expected* ]]; then
        return
      fi
    done
    sleep 0.05
  done

  print -u2 -r -- "timed out waiting for '$expected' in:"
  print -u2 -r -- "$REPLY"
  return 1
}

drain_until_quiet() {
  local chunk
  local attempt
  local quiet_polls=0

  for attempt in {1..100}; do
    if zpty -r moon-completion chunk; then
      REPLY+=$chunk
      quiet_polls=0
      continue
    fi

    quiet_polls=$((quiet_polls + 1))
    if ((quiet_polls == 3)); then
      return
    fi
    sleep 0.05
  done

  print -u2 -r -- 'completion output did not settle'
  return 1
}

read_until "$prompt"

complete_moon() {
  local line=$1
  local expected=$2

  zpty -w -n moon-completion "${line}"$'\t\t'
  read_until "$expected"
  drain_until_quiet
  local completions=$REPLY

  zpty -w -n moon-completion $'\003'
  read_until "$prompt"
  REPLY+=$completions
}

assert_contains() {
  local completions=$1
  local expected=$2
  if [[ $completions != *$expected* ]]; then
    print -u2 -r -- "missing completion '$expected' in:"
    print -u2 -r -- "$completions"
    return 1
  fi
}

assert_not_contains() {
  local completions=$1
  local unexpected=$2
  if [[ $completions == *$unexpected* ]]; then
    print -u2 -r -- "unexpected completion '$unexpected' in:"
    print -u2 -r -- "$completions"
    return 1
  fi
}

complete_moon 'moon ide ' peek-def
ide_completions=$REPLY
for command in peek-def find-references rename hover outline analyze doc; do
  assert_contains "$ide_completions" "$command"
done
assert_not_contains "$ide_completions" --verbose

complete_moon 'moon tool bu' build-binary-dep
tool_completions=$REPLY
assert_contains "$tool_completions" build-binary-dep

complete_moon 'moon build --ver' --verbose
build_completions=$REPLY
assert_contains "$build_completions" --verbose
