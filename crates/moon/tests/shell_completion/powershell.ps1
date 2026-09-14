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

param(
    [Parameter(Mandatory = $true)]
    [string]$Moon
)

$ErrorActionPreference = 'Stop'
$moonPath = (Resolve-Path $Moon).Path
$env:PATH = "$(Split-Path $moonPath)$([IO.Path]::PathSeparator)$env:PATH"

# The generated script starts with `using namespace`, which must be parsed from
# a script file rather than evaluated as an expression.
$completionPath = Join-Path ([IO.Path]::GetTempPath()) "moon-completion-$([guid]::NewGuid()).ps1"
try {
    & $moonPath shell-completion --shell powershell |
        Set-Content -Encoding utf8 $completionPath
    if ($LASTEXITCODE -ne 0) {
        throw "failed to generate PowerShell completion"
    }
    . $completionPath
} finally {
    Remove-Item -Force $completionPath
}

function Get-MoonCompletions([string]$Line) {
    $completion = TabExpansion2 -inputScript $Line -cursorColumn $Line.Length
    @($completion.CompletionMatches | ForEach-Object { $_.CompletionText })
}

function Assert-Contains($Completions, [string]$Expected) {
    if ($Completions -notcontains $Expected) {
        throw "missing completion '$Expected' in: $($Completions -join ', ')"
    }
}

function Assert-NotContains($Completions, [string]$Unexpected) {
    if ($Completions -contains $Unexpected) {
        throw "unexpected completion '$Unexpected' in: $($Completions -join ', ')"
    }
}

$ideCompletions = Get-MoonCompletions 'moon ide '
foreach ($command in @(
    'peek-def',
    'find-references',
    'rename',
    'hover',
    'outline',
    'analyze',
    'doc'
)) {
    Assert-Contains $ideCompletions $command
}
Assert-NotContains $ideCompletions '--verbose'

$toolCompletions = Get-MoonCompletions 'moon tool bu'
Assert-Contains $toolCompletions 'build-binary-dep'

$buildCompletions = Get-MoonCompletions 'moon build --ver'
Assert-Contains $buildCompletions '--verbose'
