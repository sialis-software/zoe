# Make the Windows packages in target\dist, from a release build:
#   Zoe-<version>-windows-<arch>.zip      always: Zoe.exe alone, to unpack anywhere
#   Zoe-<version>-windows-setup.exe       when Inno Setup (iscc) is installed
# Usage: powershell -File scripts\package-windows.ps1   (run on Windows; unsigned)
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")
cargo build --release -p zoe
if ($LASTEXITCODE -ne 0) { throw "the build failed" }
$version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"(.*)"' | Select-Object -First 1).Matches[0].Groups[1].Value
$arch = if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { "arm64" } else { "x64" }
$dist = "target\dist"
New-Item -ItemType Directory -Force $dist | Out-Null

$zip = "$dist\Zoe-$version-windows-$arch.zip"
if (Test-Path $zip) { Remove-Item $zip }
Compress-Archive -Path "target\release\Zoe.exe" -DestinationPath $zip
Write-Host "Made $zip"

$iscc = Get-Command iscc -ErrorAction SilentlyContinue
if (-not $iscc) {
    $found = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe", "$env:ProgramFiles\Inno Setup 6\ISCC.exe", "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe" | Where-Object { Test-Path $_ } | Select-Object -First 1
    if ($found) { $iscc = @{ Source = $found } }
}
if ($iscc) {
    & $iscc.Source "/DAppVersion=$version" "/DSourceDir=$((Resolve-Path 'target\release').Path)" packaging\windows\zoe.iss
    if ($LASTEXITCODE -ne 0) { throw "the installer failed" }
    Write-Host "Made $dist\Zoe-$version-windows-setup.exe"
} else {
    Write-Host "Skipped the installer: Inno Setup (iscc) is not installed"
}
