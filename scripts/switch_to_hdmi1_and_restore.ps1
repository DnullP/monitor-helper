param(
    [int]$Display = 1,
    [int]$Duration = 10,
    [string]$Target = 'hdmi-2',
    [string]$MonitorHelper = $env:MONITOR_HELPER,
    [string]$ProfileKey = $env:MONITOR_PROFILE
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$script:RepoRoot = Split-Path -Parent $script:ScriptDir

function Show-Usage {
    @'
Usage: .\scripts\switch_to_hdmi1_and_restore.ps1 [-Display N] [-Duration SECONDS] [-Target VALUE] [-ProfileKey KEY]

Temporarily switches the monitor input to HDMI 1 and restores the original input after a delay.
This command refuses to run when the current input readback is ambiguous and cannot be restored safely.

Parameters:
  -Display N         Monitor index passed to monitor-helper. Default: 1
  -Duration SECONDS  Seconds to wait before restoring the original input. Default: 10
  -Target VALUE      Input value or alias to switch to. Default: hdmi-1
  -ProfileKey KEY    Optional profile override such as dell-p2711v

Environment:
  MONITOR_HELPER     Path to monitor-helper.exe. Default: target\debug\monitor-helper.exe
  MONITOR_PROFILE    Optional profile override used by monitor-helper
'@ | Write-Host
}

function Invoke-External {
    param(
        [Parameter(Mandatory = $true)]
        [string]$FilePath,

        [string[]]$Arguments = @()
    )

    & $FilePath @Arguments
    $exitCode = $LASTEXITCODE
    if ($exitCode -ne 0) {
        throw "Command failed with exit code ${exitCode}: $FilePath $($Arguments -join ' ')"
    }
}

function Get-BinaryPath {
    if ($MonitorHelper) {
        return $MonitorHelper
    }

    return Join-Path $script:RepoRoot 'target\debug\monitor-helper.exe'
}

function Get-CurrentInputWritebackInfo {
    param(
        [Parameter(Mandatory = $true)]
        [string]$BinaryPath,

        [Parameter(Mandatory = $true)]
        [int]$DisplayIndex
    )

    $currentOutput = & $BinaryPath 'get' '--display' $DisplayIndex 'input'
    $exitCode = $LASTEXITCODE
    if ($exitCode -ne 0) {
        throw "Failed to read current input from display $DisplayIndex."
    }

    $writebackValueLine = $currentOutput | Where-Object { $_ -match '^writeback_value=' } | Select-Object -First 1
    $writebackSafeLine = $currentOutput | Where-Object { $_ -match '^writeback_safe=' } | Select-Object -First 1
    $writebackReasonLine = $currentOutput | Where-Object { $_ -match '^writeback_reason=' } | Select-Object -First 1

    [PSCustomObject]@{
        Value = if ($writebackValueLine) { $writebackValueLine.Substring('writeback_value='.Length) } else { $null }
        Safe = if ($writebackSafeLine) { $writebackSafeLine.Substring('writeback_safe='.Length).ToLowerInvariant() -eq 'true' } else { $false }
        Reason = if ($writebackReasonLine) { $writebackReasonLine.Substring('writeback_reason='.Length) } else { $null }
        Output = $currentOutput
    }
}

if ($Target -in @('-h', '--help')) {
    Show-Usage
    exit 0
}

if ($Display -lt 1) {
    throw 'Display indices start at 1.'
}

if ($Duration -lt 0) {
    throw 'Duration must be a non-negative integer.'
}

$binaryPath = Get-BinaryPath
if (-not (Test-Path -LiteralPath $binaryPath)) {
    throw "monitor-helper binary not found at: $binaryPath`nRun '.\build.ps1 build' first or set MONITOR_HELPER to the correct path."
}

$previousProfile = $env:MONITOR_PROFILE
if ($ProfileKey) {
    $env:MONITOR_PROFILE = $ProfileKey
}

$restored = $false
$currentCode = $null

try {
    $writeback = Get-CurrentInputWritebackInfo -BinaryPath $binaryPath -DisplayIndex $Display
    if (-not $writeback.Value -or $writeback.Value -notmatch '^[0-9]+$') {
        throw "Failed to determine a restorable input value.`n$($writeback.Output -join [Environment]::NewLine)"
    }
    if (-not $writeback.Safe) {
        $reasonText = if ($writeback.Reason) { $writeback.Reason } else { 'unknown' }
        throw "Refusing to switch display $Display with auto-restore because the current input readback is ambiguous ($reasonText). Use a direct 'set input' command for one-way switching instead."
    }

    $currentCode = $writeback.Value

    Write-Host "Current input VCP value: $currentCode"
    Write-Host "Switching display $Display to $Target for ${Duration}s"
    Invoke-External -FilePath $binaryPath -Arguments @('set', '--display', $Display, 'input', $Target)
    Start-Sleep -Seconds $Duration
}
finally {
    if (-not $restored -and $currentCode -match '^[0-9]+$') {
        $restored = $true
        Write-Host "Restoring input to VCP value $currentCode"
        try {
            Invoke-External -FilePath $binaryPath -Arguments @('set', '--display', $Display, 'input', $currentCode)
            Write-Host 'Input restored.'
        }
        catch {
            Write-Error "Failed to restore the original input. Try running: $binaryPath set --display $Display input $currentCode"
            throw
        }
    }

    if ($ProfileKey) {
        $env:MONITOR_PROFILE = $previousProfile
    }
}