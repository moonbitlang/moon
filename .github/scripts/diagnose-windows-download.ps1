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

# Download-only probe of https://cli.moonbitlang.com/install/powershell.ps1.
# Preserve its web cmdlet options, including the absence of -UseBasicParsing.
# Do not execute the fetched installer, extract archives, or run downloaded tools.
$ErrorActionPreference = 'Stop'
$ErrorView = 'NormalView'
$ProgressPreference = 'SilentlyContinue'
$VerbosePreference = 'Continue'
$downloadFailures = 0

Write-Output "[download-probe] started $(Get-Date -Format o)"
$PSVersionTable | Format-List
Get-ExecutionPolicy -List | Format-Table -AutoSize
Write-Output "[download-probe] TLS defaults: $([Net.ServicePointManager]::SecurityProtocol)"
Write-Output "[download-probe] stdin redirected: $([Console]::IsInputRedirected)"
Write-Output "[download-probe] MOON_HOME=$env:MOON_HOME"
foreach ($assembly in @([psobject].Assembly, (Get-Command Invoke-WebRequest).ImplementingType.Assembly)) {
    Get-Item -LiteralPath $assembly.Location |
        Select-Object FullName, @{Name = 'FileVersion'; Expression = { $_.VersionInfo.FileVersion }} |
        Format-List
}

$version = if ($env:MOONBIT_INSTALL_VERSION) { $env:MOONBIT_INSTALL_VERSION } else { 'latest' }
$version = $version -replace '\+', '%2B'
$baseUrl = 'https://cli.moonbitlang.com'

# Fetch through the same cmdlet as moon upgrade, but retain the script as data.
try {
    Write-Output "[download-probe] fetching $baseUrl/install/powershell.ps1"
    $installer = Invoke-RestMethod -Uri "$baseUrl/install/powershell.ps1"
    $installerPath = Join-Path $env:MOON_HOME 'upstream-installer.ps1'
    [IO.File]::WriteAllText($installerPath, [string]$installer, [Text.UTF8Encoding]::new($false))
    Get-FileHash -Algorithm SHA256 -LiteralPath $installerPath | Format-List
} catch {
    $downloadFailures++
    Write-Output '[download-probe] installer fetch failed'
    $_ | Format-List * -Force
    $_.Exception.ToString()
    $_.InvocationInfo | Format-List * -Force
}

# Continue after each failure so both downloads produce independent diagnostics.
foreach ($download in @(
    @{ Uri = "$baseUrl/binaries/$version/moonbit-windows-x86_64.zip"; Name = 'moonbit.zip' },
    @{ Uri = "$baseUrl/cores/core-$version.zip"; Name = 'core.zip' }
)) {
    $archive = Join-Path $env:MOON_HOME $download.Name
    $timer = [Diagnostics.Stopwatch]::StartNew()
    try {
        Write-Output "[download-probe] GET $($download.Uri) at $(Get-Date -Format o)"
        Invoke-WebRequest -Uri $download.Uri -OutFile $archive
        if ((Get-Item -LiteralPath $archive).Length -eq 0) {
            throw 'Downloaded file is empty.'
        }
        Get-Item -LiteralPath $archive | Select-Object Name, Length | Format-List
        Get-FileHash -Algorithm SHA256 -LiteralPath $archive | Format-List
        Write-Output '[download-probe] download succeeded'
    } catch {
        $downloadFailures++
        Write-Output "[download-probe] download failed: $($download.Uri)"
        $_ | Format-List * -Force
        $_.Exception.ToString()
        $_.InvocationInfo | Format-List * -Force
    } finally {
        Write-Output "[download-probe] elapsed=$($timer.Elapsed.TotalSeconds)s"
    }
}

Write-Output "[download-probe] failures=$downloadFailures"
if ($downloadFailures -ne 0) { exit 1 }
