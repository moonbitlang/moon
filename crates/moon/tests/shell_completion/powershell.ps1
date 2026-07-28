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
