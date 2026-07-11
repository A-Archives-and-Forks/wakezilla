$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$wakezilla = Join-Path $PSScriptRoot "..\target\debug\wakezilla.exe"
$wakezilla = (Resolve-Path $wakezilla).Path
$configRoot = Join-Path $env:ProgramData "wakezilla"
$serviceRoot = Join-Path $env:ProgramFiles "Wakezilla"
$firstPort = 39101
$secondPort = 39102

function Invoke-Wakezilla {
    param([string[]]$Arguments)

    & $wakezilla @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "wakezilla $($Arguments -join ' ') exited with code $LASTEXITCODE"
    }
}

if (Test-Path $configRoot) {
    throw "refusing to run service setup integration test because $configRoot already exists"
}
if (Test-Path $serviceRoot) {
    throw "refusing to run service setup integration test because $serviceRoot already exists"
}

try {
    Invoke-Wakezilla @("--no-update-check", "setup", "--mode", "client", "--port", "$firstPort", "--yes")
    Invoke-Wakezilla @("service", "status", "--mode", "client")

    Invoke-Wakezilla @("--no-update-check", "setup", "--mode", "client", "--port", "$secondPort", "--yes")
    Invoke-Wakezilla @("service", "status", "--mode", "client")
}
finally {
    & $wakezilla "uninstall"
    if ($LASTEXITCODE -ne 0) {
        Write-Warning "wakezilla uninstall exited with code $LASTEXITCODE during integration-test cleanup"
    }
    Remove-Item -Recurse -Force $configRoot -ErrorAction SilentlyContinue
    Remove-Item -Recurse -Force $serviceRoot -ErrorAction SilentlyContinue
}
