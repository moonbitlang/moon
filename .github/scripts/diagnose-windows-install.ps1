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

# Execute the official installer, changing only its diagnostic output and retaining
# native bundle exit codes. Preserve download, extraction, and module lookup behavior.
$ErrorActionPreference = 'Continue'
$ErrorView = 'NormalView'
$ProgressPreference = 'SilentlyContinue'
$VerbosePreference = 'Continue'
$FormatEnumerationLimit = -1
$probeBundleExitCodes = @()

# The installer or Expand-Archive may throw again while rolling back. Snapshot
# $Error at the catch site, before diagnostics can add errors or overwrite $_.
# Use .NET output first: PowerShell formatting/module failures must not hide it.
function Write-InstallFailure($OriginalError, $History) {
    [Console]::Error.WriteLine('[install-probe] CAUGHT ERROR')
    [Console]::Error.WriteLine($OriginalError.Exception.ToString())
    [Console]::Error.WriteLine([string]$OriginalError.InvocationInfo.PositionMessage)
    [Console]::Error.WriteLine([string]$OriginalError.ScriptStackTrace)
    [Console]::Error.WriteLine('[install-probe] ERROR HISTORY: oldest first, including errors before rollback')
    for ($index = $History.Count - 1; $index -ge 0; $index--) {
        $record = $History[$index]
        [Console]::Error.WriteLine("[install-probe] error id=$($record.FullyQualifiedErrorId)")
        [Console]::Error.WriteLine($record.Exception.ToString())
        [Console]::Error.WriteLine([string]$record.ErrorDetails.Message)
        [Console]::Error.WriteLine([string]$record.InvocationInfo.PositionMessage)
        [Console]::Error.WriteLine([string]$record.ScriptStackTrace)
    }
    try {
        $OriginalError | Format-List * -Force
        $OriginalError.InvocationInfo | Format-List * -Force
        Get-Module Microsoft.PowerShell.Archive | Select-Object Name, Version, Path | Format-List
    } catch {
        [Console]::Error.WriteLine("[install-probe] ADDITIONAL DIAGNOSTIC ERROR: $($_.Exception)")
    }
}

Write-Output "[install-probe] started $(Get-Date -Format o)"
$PSVersionTable | Format-List
Write-Output "[install-probe] PSModulePath=$env:PSModulePath"
Write-Output "[install-probe] WinPSModulePath=$env:WinPSModulePath"
Write-Output "[install-probe] TLS defaults: $([Net.ServicePointManager]::SecurityProtocol)"
Write-Output "[install-probe] stdin redirected: $([Console]::IsInputRedirected)"
Write-Output "[install-probe] MOON_HOME=$env:MOON_HOME"
foreach ($assembly in @([psobject].Assembly, (Get-Command Invoke-WebRequest).ImplementingType.Assembly)) {
    Get-Item -LiteralPath $assembly.Location |
        Select-Object FullName, @{Name = 'FileVersion'; Expression = { $_.VersionInfo.FileVersion }} |
        Format-List
}

$Error.Clear()
# Match moon upgrade's wrapper, including non-terminating errors before the
# installer sets ErrorActionPreference=Stop. Retain them in the error history.
Set-ExecutionPolicy RemoteSigned -Scope CurrentUser

try {
    $installer = [string](Invoke-RestMethod -Uri 'https://cli.moonbitlang.com/install/powershell.ps1' -ErrorAction Stop)
    $installerPath = Join-Path $env:MOON_HOME 'upstream-installer.ps1'
    [IO.File]::WriteAllText($installerPath, $installer, [Text.UTF8Encoding]::new($false))
    $sha256 = [Security.Cryptography.SHA256]::Create()
    try {
        $hash = [BitConverter]::ToString($sha256.ComputeHash([Text.Encoding]::UTF8.GetBytes($installer))).Replace('-', '')
        Write-Output "[install-probe] upstream installer SHA256=$hash"
    } finally {
        $sha256.Dispose()
    }

    # Fail explicitly if upstream changes the catch shape: never silently lose
    # the enhanced diagnostics. Capture before the installer's own cleanup.
    $catchPattern = '(?m)^catch \{\r?\n'
    $catchMatches = [regex]::Matches($installer, $catchPattern)
    if ($catchMatches.Count -ne 1) {
        throw 'Expected exactly one upstream installer catch block.'
    }
    $installer = $installer.Replace($catchMatches[0].Value, @'
catch {
  $originalInstallError = $_
  $originalInstallHistory = @($Error)
  Write-InstallFailure $originalInstallError $originalInstallHistory

'@)

    # The official script doesn't check native command exits. Record each one
    # immediately, before a later bundle can overwrite LASTEXITCODE.
    foreach ($command in @(
        '  moon.exe bundle --warn-list -a --all',
        '  moon.exe bundle --warn-list -a --target wasm-gc --quiet'
    )) {
        if ([regex]::Matches($installer, [regex]::Escape($command)).Count -ne 1) {
            throw "Expected one upstream bundle command: $command"
        }
        $installer = $installer.Replace($command, $command + @'

  $probeBundleExitCodes += $LASTEXITCODE
  [Console]::Error.WriteLine("[install-probe] bundle exit=$LASTEXITCODE")
'@)
    }

    Write-Output '[install-probe] starting complete official installer'
    Invoke-Expression $installer
    if ($probeBundleExitCodes.Count -ne 2 -or @($probeBundleExitCodes | Where-Object { $_ -ne 0 }).Count -ne 0) {
        throw "Bundle commands did not both succeed: $probeBundleExitCodes"
    }
    Write-Output '[install-probe] complete installation succeeded'
} catch {
    $originalProbeError = $_
    $originalProbeHistory = @($Error)
    Write-InstallFailure $originalProbeError $originalProbeHistory
    exit 1
}
