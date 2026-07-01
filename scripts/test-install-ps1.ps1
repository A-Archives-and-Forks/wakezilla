$ErrorActionPreference = "Stop"

$RootDir = Resolve-Path (Join-Path $PSScriptRoot "..")
$Script = Join-Path $RootDir "install.ps1"

$env:WAKEZILLA_INSTALL_PS1_TEST_MODE = "1"
. $Script
Remove-Item Env:WAKEZILLA_INSTALL_PS1_TEST_MODE

function Fail {
    param([string]$Message)
    throw $Message
}

function Assert-Equal {
    param(
        [object]$Expected,
        [object]$Actual,
        [string]$Label
    )
    if ($Expected -ne $Actual) {
        Fail "$Label`: expected '$Expected', got '$Actual'"
    }
}

function Assert-Contains {
    param(
        [string]$Haystack,
        [string]$Needle,
        [string]$Label
    )
    if (-not $Haystack.Contains($Needle)) {
        Fail "$Label`: expected '$Haystack' to contain '$Needle'"
    }
}

function New-TestRelease {
    [pscustomobject]@{
        tag_name   = "v0.1.49"
        prerelease = $false
        assets     = @(
            [pscustomobject]@{
                name                 = "wakezilla-0.1.49-x86_64-unknown-linux-gnu.tar.gz"
                browser_download_url = "https://example.test/wakezilla-0.1.49-x86_64-unknown-linux-gnu.tar.gz"
            },
            [pscustomobject]@{
                name                 = "wakezilla-0.1.49-x86_64-pc-windows-msvc.tar.gz"
                browser_download_url = "https://example.test/wakezilla-0.1.49-x86_64-pc-windows-msvc.tar.gz"
            }
        )
    }
}

function Test-TargetDetection {
    Assert-Equal "custom-target" (Get-WakezillaTarget -TargetOverride "custom-target") "target override"
    Assert-Equal "x86_64-pc-windows-msvc" (Get-WakezillaTarget -Architecture "X64") "windows x64 target"
}

function Test-ReleaseHelpers {
    $release = New-TestRelease
    $version = Get-ReleaseVersion -Release $release
    Assert-Equal "0.1.49" $version "release version"

    $assetUrl = Get-AssetUrl `
        -Release $release `
        -VersionValue $version `
        -TargetValue "x86_64-pc-windows-msvc"
    Assert-Equal "https://example.test/wakezilla-0.1.49-x86_64-pc-windows-msvc.tar.gz" $assetUrl "windows asset url"

    $targets = (Get-AvailableTargets -Release $release -VersionValue $version) -join " "
    Assert-Contains $targets "x86_64-pc-windows-msvc" "available windows target"
}

function Test-ChecksumHelpers {
    $tempDir = New-Item -ItemType Directory -Force -Path (Join-Path ([System.IO.Path]::GetTempPath()) "wakezilla-ps1-checksum-$PID")
    try {
        $file = Join-Path $tempDir "file.txt"
        Set-Content -NoNewline -Path $file -Value "wakezilla"
        $hash = (Get-FileHash -Algorithm SHA256 -Path $file).Hash.ToLowerInvariant()
        $checksums = "$hash  wakezilla-0.1.49-x86_64-pc-windows-msvc.tar.gz`n"
        Assert-Equal $hash (Get-ChecksumForAsset -ChecksumsText $checksums -AssetName "wakezilla-0.1.49-x86_64-pc-windows-msvc.tar.gz") "checksum lookup"
        Assert-Checksum -File $file -ChecksumsText "$hash  file.txt`n" -AssetName "file.txt"
    }
    finally {
        Remove-Item -Recurse -Force $tempDir
    }
}

function Test-ArchiveAndInstall {
    if (-not (Get-Command tar -ErrorAction SilentlyContinue)) {
        Write-Host "SKIP: tar not available"
        return
    }

    $tempDir = New-Item -ItemType Directory -Force -Path (Join-Path ([System.IO.Path]::GetTempPath()) "wakezilla-ps1-archive-$PID")
    try {
        $archiveDir = New-Item -ItemType Directory -Force -Path (Join-Path $tempDir "archive")
        $sourceExe = Join-Path $archiveDir "wakezilla.exe"
        Set-Content -NoNewline -Path $sourceExe -Value "fake exe"

        $archive = Join-Path $tempDir "wakezilla-0.1.49-x86_64-pc-windows-msvc.tar.gz"
        & tar -C $archiveDir -czf $archive "wakezilla.exe"
        if ($LASTEXITCODE -ne 0) {
            Fail "tar archive creation failed"
        }

        $extracted = Expand-WakezillaArchive -Archive $archive -OutDir (Join-Path $tempDir "extract")
        Assert-Equal "fake exe" (Get-Content -Raw -Path $extracted) "extracted binary contents"

        $installDir = Join-Path $tempDir "install-bin"
        $installed = Install-WakezillaBinary -Source $extracted -DestinationDir $installDir
        Assert-Equal (Join-Path $installDir "wakezilla.exe") $installed "installed path"
        Assert-Equal "fake exe" (Get-Content -Raw -Path $installed) "installed binary contents"
    }
    finally {
        Remove-Item -Recurse -Force $tempDir
    }
}

function New-MockService {
    param(
        [string]$Name,
        [string]$Status
    )

    $service = [pscustomobject]@{
        Name   = $Name
        Status = $Status
    }
    $service | Add-Member -MemberType ScriptMethod -Name WaitForStatus -Value {
        param(
            [object]$Status,
            [TimeSpan]$Timeout
        )
        $script:WaitedServices += "$($this.Name):$Status"
        $this.Status = $Status
    }
    $service
}

function Get-Service {
    param(
        [string]$Name,
        [Parameter(ValueFromRemainingArguments = $true)]
        [object[]]$Remaining
    )

    if ($script:MockServices -and $script:MockServices.ContainsKey($Name)) {
        return $script:MockServices[$Name]
    }
}

function Stop-Service {
    param(
        [string]$Name,
        [switch]$Force,
        [Parameter(ValueFromRemainingArguments = $true)]
        [object[]]$Remaining
    )

    $script:StoppedServices += $Name
    $script:MockServices[$Name].Status = "Stopped"
}

function Start-Service {
    param(
        [string]$Name,
        [Parameter(ValueFromRemainingArguments = $true)]
        [object[]]$Remaining
    )

    $script:StartedServices += $Name
}

function Test-ServiceStopAndRestartHelpers {
    $script:MockServices = @{
        "wakezilla-proxy"  = New-MockService -Name "wakezilla-proxy" -Status "Running"
        "wakezilla-client" = New-MockService -Name "wakezilla-client" -Status "Stopped"
    }
    $script:StoppedServices = @()
    $script:StartedServices = @()
    $script:WaitedServices = @()

    $restartServices = @(Stop-WakezillaServicesForInstall -ServiceNames @("wakezilla-proxy", "wakezilla-client"))

    Assert-Equal 1 $restartServices.Count "restart service count"
    Assert-Equal "wakezilla-proxy" $restartServices[0] "restart service name"
    Assert-Equal 1 $script:StoppedServices.Count "stopped service count"
    Assert-Equal "wakezilla-proxy" $script:StoppedServices[0] "stopped service name"
    Assert-Equal "wakezilla-proxy:Stopped" $script:WaitedServices[0] "waited service"

    Restart-WakezillaServicesAfterInstall -ServiceNames $restartServices
    Assert-Equal 1 $script:StartedServices.Count "started service count"
    Assert-Equal "wakezilla-proxy" $script:StartedServices[0] "started service name"
}

Test-TargetDetection
Test-ReleaseHelpers
Test-ChecksumHelpers
Test-ArchiveAndInstall
Test-ServiceStopAndRestartHelpers

Write-Host "install.ps1 tests passed"
