[CmdletBinding()]
param(
    [string]$InstallRoot = "",
    [switch]$Quiet
)

$ErrorActionPreference = "Stop"

if (-not $InstallRoot) {
    $InstallRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
}
$InstallRoot = [IO.Path]::GetFullPath($InstallRoot)
$binDir = Join-Path $InstallRoot "bin"
$tray = Join-Path $binDir "wakezilla-tray.exe"
$cli = Join-Path $binDir "wakezilla.exe"
$icon = Join-Path $binDir "wakezilla.ico"

try {
    $processes = @(Get-CimInstance Win32_Process -Filter "name = 'wakezilla-tray.exe'" -ErrorAction SilentlyContinue |
        Where-Object { $_.ExecutablePath -and ([IO.Path]::GetFullPath($_.ExecutablePath) -ieq $tray) })
    foreach ($process in $processes) {
        Stop-Process -Id $process.ProcessId -Force -ErrorAction SilentlyContinue
    }
} catch { }

$programs = [Environment]::GetFolderPath([Environment+SpecialFolder]::Programs)
$desktop = [Environment]::GetFolderPath([Environment+SpecialFolder]::Desktop)
foreach ($shortcut in @((Join-Path $programs "Wakezilla.lnk"), (Join-Path $desktop "Wakezilla.lnk"))) {
    if (Test-Path -LiteralPath $shortcut) { Remove-Item -LiteralPath $shortcut -Force }
}
Remove-ItemProperty -Path "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run" -Name WakezillaTray -ErrorAction SilentlyContinue
Remove-Item -Path "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Wakezilla" -Recurse -Force -ErrorAction SilentlyContinue
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath) {
    $keptPath = @($userPath -split ";" | Where-Object { $_ -and ($_.TrimEnd("\") -ine $binDir.TrimEnd("\")) }) -join ";"
    [Environment]::SetEnvironmentVariable("Path", $keptPath, "User")
}
foreach ($path in @($tray, $cli, $icon, (Join-Path $InstallRoot "uninstall-wakezilla.ps1"))) {
    if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Force }
}
if (Test-Path -LiteralPath $binDir) {
    if (@(Get-ChildItem -LiteralPath $binDir -Force).Count -eq 0) { Remove-Item -LiteralPath $binDir -Force }
}
if (Test-Path -LiteralPath $InstallRoot) {
    if (@(Get-ChildItem -LiteralPath $InstallRoot -Force).Count -eq 0) { Remove-Item -LiteralPath $InstallRoot -Force }
}

if (-not $Quiet) {
    Write-Host "Wakezilla graphical integration removed; configuration and data were preserved."
}
